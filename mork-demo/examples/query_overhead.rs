//! Isolate per-query overhead in the MORK query path, single-threaded, so the cost can be
//! attributed before optimizing. Three phases over a loaded MorkSpace:
//!   A. query a *reused* pre-built pattern atom   -> pure codec + kernel cost
//!   B. query a freshly `format!`-built atom each  -> A + caller-side atom construction
//!   C. reused pattern against a grounded-heavy space -> exposes any per-query work that scales
//!      with the space's grounded-atom registry (e.g. cloning it on every query)
//! The A/B gap is caller atom-build; the A/C gap is registry-scaling cost.
//!
//! Run:  cargo run --release -p mork-demo --example query_overhead

use std::time::Instant;

use hyperon_atom::Atom;
use hyperon_space::{Space, SpaceMut};
use metta_on_mork::MorkSpace;

const N: usize = 100_000;
const Q: usize = 1_000_000;

fn edge(i: usize) -> Atom {
    Atom::expr([
        Atom::sym("edge"),
        Atom::sym(format!("n{i}")),
        Atom::sym(format!("n{}", i + 1)),
    ])
}

/// `(val n{i} <Number i>)` -- a grounded atom per fact, so the space's grounded registry holds N
/// entries.
fn valued(i: usize) -> Atom {
    Atom::expr([
        Atom::sym("val"),
        Atom::sym(format!("n{i}")),
        Atom::gnd(hyperon_atom::gnd::number::Number::Integer(i as i64)),
    ])
}

fn edge_query(k: usize) -> Atom {
    Atom::expr([Atom::sym("edge"), Atom::sym(format!("n{k}")), Atom::var("d")])
}

fn val_query(k: usize) -> Atom {
    Atom::expr([Atom::sym("val"), Atom::sym(format!("n{k}")), Atom::var("x")])
}

fn key(j: usize) -> usize {
    j.wrapping_mul(2_654_435_761) % N
}

fn qps(ms: f64) -> f64 {
    Q as f64 / (ms / 1000.0)
}

fn main() {
    let mut edges = MorkSpace::new();
    for i in 0..N {
        edges.add(edge(i));
    }
    let mut vals = MorkSpace::new();
    for i in 0..N {
        vals.add(valued(i));
    }
    println!("{N} atoms loaded per space; {Q} queries per phase.\n");

    // A. reused pattern atom (build the atom once, query it Q times) -- pure codec + kernel.
    let pat = edge_query(key(0));
    let t = Instant::now();
    let mut hits = 0;
    for _ in 0..Q {
        hits += edges.query(&pat).len();
    }
    let a = t.elapsed().as_secs_f64() * 1000.0;
    println!("A reused atom, edge space        {a:>8.0}ms  {:>10.0} q/s  hits/q={}", qps(a), hits / Q);

    // B. fresh atom per query (format! + Atom::expr) -- adds caller-side construction.
    let t = Instant::now();
    let mut hits = 0;
    for j in 0..Q {
        hits += edges.query(&edge_query(key(j))).len();
    }
    let b = t.elapsed().as_secs_f64() * 1000.0;
    println!("B fresh atom,  edge space        {b:>8.0}ms  {:>10.0} q/s  hits/q={}", qps(b), hits / Q);

    // C. reused pattern, grounded-heavy space -- exposes registry-scaling per-query work.
    let vpat = val_query(key(0));
    let t = Instant::now();
    let mut hits = 0;
    for _ in 0..Q {
        hits += vals.query(&vpat).len();
    }
    let c = t.elapsed().as_secs_f64() * 1000.0;
    println!("C reused atom, grounded space    {c:>8.0}ms  {:>10.0} q/s  hits/q={}", qps(c), hits / Q);

    println!("\nA->B gap = caller atom build ({:.0}ms / {Q} = {:.0}ns each).", b - a, (b - a) * 1e6 / Q as f64);
    println!("A->C gap = grounded-registry per-query cost ({:.0}ms / {Q} = {:.0}ns each).", c - a, (c - a) * 1e6 / Q as f64);
}
