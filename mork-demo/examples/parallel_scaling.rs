//! Measure parallel query scaling of the MORK snapshot from 1 to 32 threads, and separate the
//! per-query *atom construction* cost from the *query path* cost. Each thread pre-builds its own
//! query atoms (Atom is !Send/!Sync, so they cannot be shared), then times only the query loop
//! over them -- so the reported throughput is the codec+kernel query path, not format!/Atom build.
//! A second pass times build+query together, exposing how much of the ceiling is atom construction.
//!
//! Run:  cargo run --release -p mork-demo --example parallel_scaling

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use hyperon_atom::Atom;
use hyperon_space::{Space, SpaceMut};
use metta_on_mork::{MorkSnapshot, MorkSpace};

#[derive(Clone, Copy)]
enum Mode {
    /// `count_matches`: re-encode the pattern on every call (the per-query allocation).
    Count,
    /// `prepare` once per pattern, then `count_prepared` reusing the encoded bytes.
    Prepared,
}

const N: usize = 2_000_000;
const PER_THREAD: usize = 30_000; // query atoms per thread
const REPS: usize = 8; // passes over them in the timed loop

fn edge(i: usize) -> Atom {
    Atom::expr([
        Atom::sym("edge"),
        Atom::sym(format!("n{i}")),
        Atom::sym(format!("n{}", i + 1)),
    ])
}

fn q(k: usize) -> Atom {
    Atom::expr([Atom::sym("edge"), Atom::sym(format!("n{k}")), Atom::var("d")])
}

/// Run `threads` workers; each pre-builds its own query atoms, then times only the query loop.
/// `Count` re-encodes every call; `Prepared` encodes each pattern once and reruns the bytes.
/// Wall-clock throughput uses the slowest worker.
fn run(snap: &Arc<MorkSnapshot>, threads: usize, mode: Mode) -> f64 {
    let handles: Vec<_> = (0..threads)
        .map(|tid| {
            let snap = Arc::clone(snap);
            thread::spawn(move || {
                let base = tid * PER_THREAD;
                let atoms: Vec<Atom> =
                    (0..PER_THREAD).map(|j| q((base + j).wrapping_mul(2_654_435_761) % N)).collect();
                let mut hits = 0usize;
                let t = Instant::now();
                match mode {
                    Mode::Count => {
                        for _ in 0..REPS {
                            for a in &atoms {
                                hits += snap.count_matches(a);
                            }
                        }
                    }
                    Mode::Prepared => {
                        // Encode each pattern once, outside the timed reuse loop's per-call cost.
                        let prepared: Vec<_> = atoms.iter().filter_map(|a| snap.prepare(a)).collect();
                        for _ in 0..REPS {
                            for p in &prepared {
                                hits += snap.count_prepared(p);
                            }
                        }
                    }
                }
                (PER_THREAD * REPS, t.elapsed().as_secs_f64(), hits)
            })
        })
        .collect();
    let (mut total_q, mut max_s) = (0usize, 0.0f64);
    for h in handles {
        let (qd, s, _hh) = h.join().unwrap();
        total_q += qd;
        max_s = max_s.max(s);
    }
    total_q as f64 / max_s
}

fn main() {
    println!("Loading {N} edges...");
    let mut s = MorkSpace::new();
    for i in 0..N {
        s.add(edge(i));
    }
    let snap = Arc::new(s.snapshot());
    println!("cores={}  PER_THREAD={PER_THREAD}  REPS={REPS}\n", std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0));

    for (label, mode) in [("count_matches (encode every query)", Mode::Count), ("count_prepared (encode once, reuse)", Mode::Prepared)] {
        println!("== {label} ==");
        let mut base = 0.0;
        for (i, &t) in [1usize, 2, 4, 8, 16, 32].iter().enumerate() {
            let qps = run(&snap, t, mode);
            if i == 0 {
                base = qps;
            }
            println!("  {t:>2} threads  {qps:>11.0} q/s  {:>5.1}x  ({:>4.0}% of linear)", qps / base, 100.0 * (qps / base) / t as f64);
        }
        println!();
    }
}
