//! Single-config parallel point-query harness for `perf stat`. One run = one fixed
//! (threads, access-pattern) so the perf counters isolate it. Answers: is the 8->16
//! scaling ceiling an all-core clock drop (hardware floor) or LLC-miss/bandwidth from a
//! scattered working set (fixable by locality-preserving sharding)?
//!
//!   scatter: each thread point-queries 30k keys spread across the whole 2M trie (hash
//!            spread, the current parallel_scaling pattern) -- no locality, both CCDs drag
//!            the whole trie through their L3s.
//!   local:   each thread owns a contiguous, disjoint 30k-key block -- compact per-thread
//!            working set whose trie paths share prefixes, so it stays CCD-local.
//!
//! Run:  cargo run --release -p mork-demo --example perf_parallel -- <threads> <scatter|local>
//! Perf: perf stat -d -d target/release/examples/perf_parallel 16 scatter

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

fn main() {
    let mut a = std::env::args().skip(1);
    let threads: usize = a.next().and_then(|s| s.parse().ok()).unwrap_or(16);
    let local = a.next().map(|s| s == "local").unwrap_or(false);

    eprintln!("loading {N}; threads={threads}; pattern={}", if local { "local" } else { "scatter" });
    let mut s = MorkSpace::new();
    for i in 0..N {
        s.add(edge(i));
    }
    let snap = Arc::new(s.snapshot());
    eprintln!("starting");

    let handles: Vec<_> = (0..threads)
        .map(|tid| {
            let snap: Arc<MorkSnapshot> = Arc::clone(&snap);
            thread::spawn(move || {
                let base = tid * PER_THREAD;
                // local: contiguous disjoint block. scatter: hash-spread across all N.
                let keys: Vec<usize> = (0..PER_THREAD)
                    .map(|j| if local { (base + j) % N } else { (base + j).wrapping_mul(2_654_435_761) % N })
                    .collect();
                let prepared: Vec<_> = keys.iter().filter_map(|&k| snap.prepare(&q(k))).collect();
                let t = Instant::now();
                let mut hits = 0usize;
                for _ in 0..REPS {
                    for p in &prepared {
                        hits += snap.count_prepared(p);
                    }
                }
                (PER_THREAD * REPS, t.elapsed().as_secs_f64(), hits)
            })
        })
        .collect();

    let (mut total, mut max_s, mut hits) = (0usize, 0.0f64, 0usize);
    for h in handles {
        let (q, s, hh) = h.join().unwrap();
        total += q;
        max_s = max_s.max(s);
        hits += hh;
    }
    println!("{threads} threads  {:.0} q/s  (hits={hits})", total as f64 / max_s);
}
