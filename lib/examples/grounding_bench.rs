//! Baseline: the stock Hyperon `GroundingSpace` on the same load-then-query
//! workload as mork-hyperon-space's `scale_showcase`, for a head-to-head. Each
//! scale runs under `catch_unwind` so a #1076-style trie panic is reported rather
//! than aborting the run.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

use hyperon::space::grounding::GroundingSpace;
use hyperon_atom::*;

fn run(n: u32) -> (std::time::Duration, std::time::Duration, usize) {
    let mut space = GroundingSpace::new();
    let t = Instant::now();
    for i in 0..n {
        space.add(Atom::expr([
            Atom::sym("edge"),
            Atom::sym(format!("n{}", i)),
            Atom::sym(format!("n{}", i + 1)),
        ]));
    }
    let load = t.elapsed();

    let q = Atom::expr([
        Atom::sym("edge"),
        Atom::sym(format!("n{}", n / 2)),
        Atom::var("dst"),
    ]);
    let t = Instant::now();
    let results = space.query(&q);
    let query = t.elapsed();
    (load, query, results.len())
}

fn main() {
    println!("stock Hyperon GroundingSpace:");
    for n in [1_000u32, 2_000, 5_000, 10_000, 50_000, 100_000, 500_000] {
        match catch_unwind(AssertUnwindSafe(|| run(n))) {
            Ok((load, query, count)) => println!(
                "{:>7} atoms | load {:>10.2?} | first query {:>12.2?} | {} result(s)",
                n, load, query, count
            ),
            Err(_) => {
                println!("{:>7} atoms | PANIC (e.g. #1076 trie crash) -- stopping", n);
                break;
            }
        }
    }
}
