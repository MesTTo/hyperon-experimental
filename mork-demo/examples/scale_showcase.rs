//! Scale head-to-head: stock Hyperon `GroundingSpace` vs the MORK-backed `MorkSpace`, driven
//! through the bare `Space`/`SpaceMut` trait (no interpreter), on a same-head atom workload
//! (`(item-shape-signal pattern-i target-i i)`), the shape behind hyperon-experimental #1079.
//!
//! For each N it reports, per backend: load time, `atom_count`, `visit` count, the count of a
//! full wildcard query, and a point-query latency. The true count is N, so a backend whose
//! enumeration or wildcard count is < N is returning WRONG answers (GroundingSpace's trie
//! undercounts same-head atoms; MORK walks every leaf, so it is exact). Every backend call is
//! caught, so a #1076-style trie panic is reported rather than aborting the run.
//!
//! Run:  cargo run --release -p mork-demo --example scale_showcase

use std::borrow::Cow;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

use hyperon::space::grounding::GroundingSpace;
use hyperon_atom::{Atom, VariableAtom};
use hyperon_space::{Space, SpaceMut, SpaceVisitor};
use metta_on_mork::MorkSpace;

const SIZES: &[usize] = &[1_500, 15_000, 150_000, 1_500_000];

struct Counter(usize);
impl SpaceVisitor for Counter {
    fn accept(&mut self, _: Cow<'_, Atom>) {
        self.0 += 1;
    }
}

/// A same-head atom: every atom shares the head symbol `item-shape-signal`, the case the
/// GroundingSpace trie mis-indexes (#1079).
fn atom(i: usize) -> Atom {
    Atom::expr([
        Atom::sym("item-shape-signal"),
        Atom::sym(format!("pattern-{i}")),
        Atom::sym(format!("target-{i}")),
        Atom::sym(format!("{i}")),
    ])
}

fn point_query(i: usize) -> Atom {
    // Atoms are 4-element (head + 3 args); pin the first arg, leave the other two free.
    Atom::expr([
        Atom::sym("item-shape-signal"),
        Atom::sym(format!("pattern-{i}")),
        Atom::var("b"),
        Atom::var("c"),
    ])
}

fn wildcard_query() -> Atom {
    Atom::expr([
        Atom::sym("item-shape-signal"),
        Atom::var("p"),
        Atom::var("b"),
        Atom::var("c"),
    ])
}

fn ok<T>(r: std::thread::Result<T>, f: impl FnOnce(T) -> String) -> String {
    r.map(f).unwrap_or_else(|_| "PANIC".to_string())
}

/// Loads N same-head atoms into `space` and measures it. Read ops take `&self`, so a panic
/// inside `catch_unwind` leaves `space` usable for the next phase.
fn measure<S: Space + SpaceMut>(label: &str, mut space: S, n: usize) {
    let t = Instant::now();
    let load = catch_unwind(AssertUnwindSafe(|| {
        for i in 0..n {
            space.add(atom(i));
        }
    }));
    let load_ms = t.elapsed().as_secs_f64() * 1000.0;
    if load.is_err() {
        println!("  {label:<14} load PANIC at scale {n}");
        return;
    }

    let count = ok(catch_unwind(AssertUnwindSafe(|| space.atom_count())), |c| {
        c.map_or("?".into(), |c| flag(c, n))
    });
    let visit = ok(
        catch_unwind(AssertUnwindSafe(|| {
            let mut c = Counter(0);
            let _ = space.visit(&mut c);
            c.0
        })),
        |c| flag(c, n),
    );

    let t = Instant::now();
    let wild = ok(
        catch_unwind(AssertUnwindSafe(|| space.query(&wildcard_query()).len())),
        |c| flag(c, n),
    );
    let wild_ms = t.elapsed().as_secs_f64() * 1000.0;

    let t = Instant::now();
    let point = ok(
        catch_unwind(AssertUnwindSafe(|| space.query(&point_query(n / 2)).len())),
        |c| format!("{c} match"),
    );
    let point_us = t.elapsed().as_secs_f64() * 1e6;

    println!(
        "  {label:<14} load {load_ms:>9.1}ms  count {count:<12} visit {visit:<12} wildcard {wild:<12} ({wild_ms:>7.1}ms)  point {point} {point_us:>8.1}us"
    );
}

/// Tags a measured count: bare number if it equals the true N, else `got/N WRONG`.
fn flag(got: usize, n: usize) -> String {
    if got == n {
        format!("{got}")
    } else {
        format!("{got}/{n} WRONG")
    }
}

fn main() {
    println!("Same-head workload (item-shape-signal ...). True atom count = N.");
    println!("A count < N means the backend returns WRONG answers (lost atoms).\n");
    for &n in SIZES {
        println!("N = {n}");
        measure("GroundingSpace", GroundingSpace::new(), n);
        measure("MORK", MorkSpace::new(), n);
        println!();
    }
}
