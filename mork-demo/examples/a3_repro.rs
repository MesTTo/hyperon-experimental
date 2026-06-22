//! Localize the a3 conjunction 4x over-count: compare raw Space::query bindings on
//! MorkSpace vs GroundingSpace for the conjunction and each factor.
use hyperon::space::grounding::GroundingSpace;
use hyperon_atom::Atom;
use hyperon_space::{Space, SpaceMut};
use metta_on_mork::MorkSpace;

fn implies(head: &str) -> Atom {
    // (implies (Frog $x) (<head> $x))
    Atom::expr([
        Atom::sym("implies"),
        Atom::expr([Atom::sym("Frog"), Atom::var("x")]),
        Atom::expr([Atom::sym(head), Atom::var("x")]),
    ])
}

fn kb() -> Vec<Atom> {
    vec![
        implies("Green"),
        implies("Eats-flies"),
        Atom::expr([Atom::sym("Frog"), Atom::sym("Sam")]),
        Atom::expr([Atom::sym("Robot"), Atom::sym("Sophia")]),
    ]
}

fn factor(p: &str, head: &str) -> Atom {
    // (implies ($P $x) (<head> $x))
    Atom::expr([
        Atom::sym("implies"),
        Atom::expr([Atom::var(p), Atom::var("x")]),
        Atom::expr([Atom::sym(head), Atom::var("x")]),
    ])
}

fn conjunction() -> Atom {
    Atom::expr([Atom::sym(","), factor("P", "Green"), factor("P", "Eats-flies")])
}

fn show(label: &str, q: &Atom) {
    let mut gs = GroundingSpace::new();
    let mut ms = MorkSpace::new();
    for a in kb() {
        gs.add(a.clone());
        ms.add(a);
    }
    let gr = gs.query(q);
    let mr = ms.query(q);
    println!("--- {label} ---");
    println!("  GroundingSpace: {} result(s)", gr.len());
    for b in gr.iter() {
        println!("    {b}");
    }
    println!("  MorkSpace:      {} result(s)", mr.len());
    for b in mr.iter() {
        println!("    {b}");
    }
}

fn main() {
    show("factor1 (implies ($P $x) (Green $x))", &factor("P", "Green"));
    show("conjunction (, f1 f2)", &conjunction());
}
