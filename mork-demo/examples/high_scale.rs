//! High-scale single-backend benchmark for the MORK-backed atomspace, into the tens of millions
//! of atoms -- a range GroundingSpace cannot reach (its trie panics, #1076, at ~1,500 same-head
//! atoms). For each N: load time + rate, exact `atom_count`, point-query latency, and parallel
//! query throughput over one shared `Arc<MorkSnapshot>` on 8 threads.
//!
//! Run:  cargo run --release -p mork-demo --example high_scale

use std::io::{stdout, Write};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use hyperon_atom::Atom;
use hyperon_space::{Space, SpaceMut};
use metta_on_mork::MorkSpace;

const SIZES: &[usize] = &[1_000_000, 5_000_000, 20_000_000];
const PAR_QUERIES: usize = 800_000;
const THREADS: usize = 8;

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

fn main() {
    println!(
        "MORK-backed atomspace at scale (edge facts). GroundingSpace cannot run this: its\n\
         trie panics (#1076) at ~1,500 same-head atoms.\n"
    );
    for &n in SIZES {
        let t = Instant::now();
        let mut s = MorkSpace::new();
        for i in 0..n {
            s.add(edge(i));
        }
        let load = t.elapsed().as_secs_f64();
        let count = s.atom_count().unwrap_or(0);

        // Point-query latency: median-ish over reps spread across the space.
        let reps = 20_000usize;
        let t = Instant::now();
        let mut hits = 0usize;
        for j in 0..reps {
            hits += s.query(&q(j.wrapping_mul(2_654_435_761) % n)).len();
        }
        let point_us = t.elapsed().as_secs_f64() * 1e6 / reps as f64;

        // Parallel throughput over one shared snapshot.
        let snap = Arc::new(s.snapshot());
        let per = PAR_QUERIES / THREADS;
        let t = Instant::now();
        let handles: Vec<_> = (0..THREADS)
            .map(|tid| {
                let snap = Arc::clone(&snap);
                thread::spawn(move || {
                    (0..per)
                        .map(|j| snap.query(&q((tid * per + j).wrapping_mul(2_654_435_761) % n)).len())
                        .sum::<usize>()
                })
            })
            .collect();
        let phits: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
        let par_qps = PAR_QUERIES as f64 / t.elapsed().as_secs_f64();

        println!(
            "N={:>9}  load {:>6.1}s ({:>4.2}M atoms/s)  count {} ({})  point {:>5.1}us (hit={})  parallel {THREADS}t {:>9.0} q/s (hit={})",
            n,
            load,
            n as f64 / load / 1e6,
            count,
            if count == n { "exact" } else { "WRONG" },
            point_us,
            hits / reps,
            par_qps,
            phits / PAR_QUERIES,
        );
        let _ = stdout().flush();
    }
}
