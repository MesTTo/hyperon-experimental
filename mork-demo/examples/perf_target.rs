//! A tight single-threaded point-query loop over a large MORK trie, for `perf stat`/`perf record`
//! to profile the query path (is it memory-latency/bandwidth bound, or CPU?).
//! Run:  perf stat -d target/release/examples/perf_target
use hyperon_atom::Atom;
use hyperon_space::{Space, SpaceMut};
use metta_on_mork::MorkSpace;

const N: usize = 2_000_000;
const Q: usize = 3_000_000;

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
    let mut s = MorkSpace::new();
    for i in 0..N {
        s.add(edge(i));
    }
    let snap = s.snapshot();
    eprintln!("loaded {N}; starting {Q} queries");
    let mut hits = 0usize;
    for j in 0..Q {
        hits += snap.count_matches(&q(j.wrapping_mul(2_654_435_761) % N));
    }
    println!("hits={hits}");
}
