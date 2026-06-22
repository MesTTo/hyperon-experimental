//! In-process proof of topology-aware (CCD/L3-domain) sharding with thread affinity.
//!
//! The two-process experiment showed the 9950X sustains ~22M q/s when each CCD owns its own
//! data, vs ~3.5M when 16 cores share one snapshot across both CCDs (cross-CCD snoop latency
//! collapses IPC to 0.7). This realizes the same thing INSIDE one process:
//!   - detect L3 domains from /sys (one per CCD; one on a single-CCD chip -> degrades to no-op),
//!   - build one shard (its own snapshot, disjoint key range) per domain,
//!   - pin a worker pool to each domain's physical cores; each worker only touches its shard,
//!     so each domain's L3 caches a disjoint working set and no cache line is shared across CCDs.
//! Compares aggregate q/s to the shared-snapshot baseline (all workers, one snapshot, no affinity).
//!
//! Build: RUSTFLAGS="-C target-cpu=native" cargo +nightly build --release -p mork-demo --example perf_ccd_sharded
//! Run:   target/release/examples/perf_ccd_sharded

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use hyperon_atom::Atom;
use hyperon_space::SpaceMut;
use metta_on_mork::{MorkSnapshot, MorkSpace};

const N: usize = 2_000_000;
const PER_THREAD: usize = 30_000;
const REPS: usize = 8;

fn edge(i: usize) -> Atom {
    Atom::expr([Atom::sym("edge"), Atom::sym(format!("n{i}")), Atom::sym(format!("n{}", i + 1))])
}
fn q(k: usize) -> Atom {
    Atom::expr([Atom::sym("edge"), Atom::sym(format!("n{k}")), Atom::var("d")])
}

/// Pin the calling thread to the given logical CPUs.
fn pin(cpus: &[usize]) {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        for &c in cpus {
            libc::CPU_SET(c, &mut set);
        }
        let _ = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set as *const _);
    }
}

/// Parse a Linux cpu list like "0-7,16-23" into [0,1,..,7,16,..,23].
fn parse_cpu_list(s: &str) -> Vec<usize> {
    let mut out = Vec::new();
    for part in s.split(',') {
        if let Some((a, b)) = part.split_once('-') {
            if let (Ok(a), Ok(b)) = (a.trim().parse::<usize>(), b.trim().parse::<usize>()) {
                out.extend(a..=b);
            }
        } else if let Ok(a) = part.trim().parse::<usize>() {
            out.push(a);
        }
    }
    out
}

/// Distinct L3 cache domains (one per CCD). Falls back to a single all-CPU domain.
fn l3_domains() -> Vec<Vec<usize>> {
    let n = thread::available_parallelism().map(|x| x.get()).unwrap_or(1);
    let mut seen: Vec<Vec<usize>> = Vec::new();
    for cpu in 0..n {
        let p = format!("/sys/devices/system/cpu/cpu{cpu}/cache/index3/shared_cpu_list");
        if let Ok(s) = std::fs::read_to_string(&p) {
            let list = parse_cpu_list(s.trim());
            if !list.is_empty() && !seen.contains(&list) {
                seen.push(list);
            }
        }
    }
    if seen.is_empty() {
        seen.push((0..n).collect());
    }
    seen
}

/// Physical cores of a domain: lower half on standard x86 Linux enumeration (SMT siblings come after).
fn physical_half(domain: &[usize]) -> Vec<usize> {
    let mut d = domain.to_vec();
    d.sort_unstable();
    let h = (d.len() / 2).max(1);
    d.into_iter().take(h).collect()
}

/// Each worker prepares its key block once, then times REPS passes of prepared point queries.
/// `affinity` pins it to its domain's cpus. Returns (queries, elapsed_secs).
fn worker(
    snap: Arc<MorkSnapshot>,
    key_base: usize,
    affinity: Option<Vec<usize>>,
) -> (usize, f64) {
    if let Some(cpus) = affinity {
        pin(&cpus);
    }
    let prepared: Vec<_> =
        (0..PER_THREAD).filter_map(|j| snap.prepare(&q(key_base + j))).collect();
    let t = Instant::now();
    let mut hits = 0usize;
    for _ in 0..REPS {
        for p in &prepared {
            hits += snap.count_prepared(p);
        }
    }
    let _ = hits;
    (PER_THREAD * REPS, t.elapsed().as_secs_f64())
}

fn main() {
    let domains = l3_domains();
    let phys: Vec<Vec<usize>> = domains.iter().map(|d| physical_half(d)).collect();
    let total_workers: usize = phys.iter().map(|p| p.len()).sum();
    println!("detected {} L3 domain(s); physical cores per domain: {:?}", domains.len(), phys);
    println!("total physical workers = {total_workers}\n");

    // ---- Baseline: one shared snapshot, all workers, NO affinity (the collapse case) ----
    println!("building shared space ({N} edges)...");
    let mut shared = MorkSpace::new();
    for i in 0..N {
        shared.add(edge(i));
    }
    let shared_snap = Arc::new(shared.snapshot());

    let run_shared = || {
        let handles: Vec<_> = (0..total_workers)
            .map(|w| {
                let snap = Arc::clone(&shared_snap);
                // scatter across the whole space, like the original parallel benchmark
                let base = (w * PER_THREAD).wrapping_mul(2_654_435_761) % N;
                thread::spawn(move || worker(snap, base, None))
            })
            .collect();
        let (mut tot, mut max_s) = (0usize, 0.0f64);
        for h in handles {
            let (qd, s) = h.join().unwrap();
            tot += qd;
            max_s = max_s.max(s);
        }
        tot as f64 / max_s
    };
    let shared_qps = run_shared();
    println!("shared snapshot, {total_workers} workers, no affinity:  {shared_qps:>12.0} q/s\n");

    // ---- Sharded + affinity: one shard per domain, workers pinned, disjoint key ranges ----
    let nd = phys.len();
    let chunk = N / nd;
    println!("building {nd} shard(s) of {chunk} edges each...");
    let shards: Vec<Arc<MorkSnapshot>> = (0..nd)
        .map(|d| {
            let mut s = MorkSpace::new();
            for i in (d * chunk)..((d + 1) * chunk) {
                s.add(edge(i));
            }
            Arc::new(s.snapshot())
        })
        .collect();

    let run_sharded = || {
        let mut handles = Vec::new();
        for (d, cores) in phys.iter().enumerate() {
            for (li, _) in cores.iter().enumerate() {
                let snap = Arc::clone(&shards[d]);
                let cpus = cores.clone();
                // disjoint contiguous block within this shard (CCD-local working set)
                let key_base = d * chunk + (li * PER_THREAD) % chunk.max(1);
                handles.push(thread::spawn(move || worker(snap, key_base, Some(cpus))));
            }
        }
        let (mut tot, mut max_s) = (0usize, 0.0f64);
        for h in handles {
            let (qd, s) = h.join().unwrap();
            tot += qd;
            max_s = max_s.max(s);
        }
        tot as f64 / max_s
    };
    let sharded_qps = run_sharded();
    println!("sharded + CCD affinity, {total_workers} workers:        {sharded_qps:>12.0} q/s\n");

    println!("=> sharded/shared speedup: {:.2}x", sharded_qps / shared_qps);
}
