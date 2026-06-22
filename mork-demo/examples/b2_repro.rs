//! Trace b2_backchain: `match` used inside equalities, with recursion over compound
//! bindings captured from the space. Run staged fragments on MORK vs GroundingSpace
//! to find the exact step that diverges.
use hyperon::metta::runner::Metta;
use hyperon::metta::text::SExprParser;
use hyperon_space::DynSpace;
use metta_on_mork::MorkSpace;

const STAGES: &[(&str, &str)] = &[
    // 1. match inside an equality, success and failure.
    (
        "match-in-equality",
        "\
(Frog Sam)
(= (frog $x) (match &self (Frog $x) T))
!(frog Sam)
!(frog Fritz)",
    ),
    // 2. single implication step: bind $a to a compound (Evaluation (human Plato)).
    (
        "one-implication",
        "\
(Evaluation (philosopher Plato))
(Evaluation (likes-to-wrestle Plato))
(Implication (Evaluation (human $x)) (Evaluation (mortal $x)))
(= (deduce (Evaluation ($P $x))) (match &self (Evaluation ($P $x)) T))
(= (deduce (Evaluation ($P $x)))
   (match &self (Implication $a (Evaluation ($P $x))) (deduce $a)))
!(deduce (Evaluation (human Plato)))",
    ),
    // 3a. Just the raw match that binds $a to a compound with an internal var that
    // must coreference with $x=Plato from the conclusion. Returns the instantiated $a.
    (
        "match-binds-compound",
        "\
(Implication
   (And (Evaluation (philosopher $x)) (Evaluation (likes-to-wrestle $x)))
   (Evaluation (human $x)))
!(match &self (Implication $a (Evaluation (human Plato))) $a)",
    ),
    // 3b. deduce a compound And directly (And-rule recursion + base-case match).
    (
        "deduce-and-compound",
        "\
(Evaluation (philosopher Plato))
(Evaluation (likes-to-wrestle Plato))
(= (deduce (Evaluation ($P $x))) (match &self (Evaluation ($P $x)) T))
(= (deduce (And $a $b)) (And (deduce $a) (deduce $b)))
(= (And T T) T)
!(deduce (And (Evaluation (philosopher Plato)) (Evaluation (likes-to-wrestle Plato))))",
    ),
    // 3c. derive human via the And-implication (the recursive step that builds the And).
    (
        "deduce-human",
        "\
(Evaluation (philosopher Plato))
(Evaluation (likes-to-wrestle Plato))
(Implication
   (And (Evaluation (philosopher $x)) (Evaluation (likes-to-wrestle $x)))
   (Evaluation (human $x)))
(= (deduce (Evaluation ($P $x))) (match &self (Evaluation ($P $x)) T))
(= (deduce (Evaluation ($P $x)))
   (match &self (Implication $a (Evaluation ($P $x))) (deduce $a)))
(= (deduce (And $a $b)) (And (deduce $a) (deduce $b)))
(= (And T T) T)
!(deduce (Evaluation (human Plato)))",
    ),
    // 3d. two implications coexist, but query human (1 level). Isolates "2 impls in space".
    (
        "two-impls-query-human",
        "\
(Evaluation (philosopher Plato))
(Evaluation (likes-to-wrestle Plato))
(Implication
   (And (Evaluation (philosopher $x)) (Evaluation (likes-to-wrestle $x)))
   (Evaluation (human $x)))
(Implication (Evaluation (human $x)) (Evaluation (mortal $x)))
(= (deduce (Evaluation ($P $x))) (match &self (Evaluation ($P $x)) T))
(= (deduce (Evaluation ($P $x)))
   (match &self (Implication $a (Evaluation ($P $x))) (deduce $a)))
(= (deduce (And $a $b)) (And (deduce $a) (deduce $b)))
(= (And T T) T)
!(deduce (Evaluation (human Plato)))",
    ),
    // 3e. direct human fact + human->mortal impl, query mortal (2 levels, no And).
    (
        "mortal-from-direct-human",
        "\
(Evaluation (human Plato))
(Implication (Evaluation (human $x)) (Evaluation (mortal $x)))
(= (deduce (Evaluation ($P $x))) (match &self (Evaluation ($P $x)) T))
(= (deduce (Evaluation ($P $x)))
   (match &self (Implication $a (Evaluation ($P $x))) (deduce $a)))
!(deduce (Evaluation (mortal Plato)))",
    ),
    // 3f. raw mortal match with BOTH implications present: expect exactly one $a result
    // = (Evaluation (human Plato)). A spurious extra match from the And-impl would show.
    (
        "raw-mortal-match-2impls",
        "\
(Implication
   (And (Evaluation (philosopher $x)) (Evaluation (likes-to-wrestle $x)))
   (Evaluation (human $x)))
(Implication (Evaluation (human $x)) (Evaluation (mortal $x)))
!(match &self (Implication $a (Evaluation (mortal Plato))) $a)",
    ),
    // 3. full deduction chain (And rule + recursion).
    (
        "full-deduce",
        "\
(Evaluation (philosopher Plato))
(Evaluation (likes-to-wrestle Plato))
(Implication
   (And (Evaluation (philosopher $x)) (Evaluation (likes-to-wrestle $x)))
   (Evaluation (human $x)))
(Implication (Evaluation (human $x)) (Evaluation (mortal $x)))
(= (deduce (Evaluation ($P $x))) (match &self (Evaluation ($P $x)) T))
(= (deduce (Evaluation ($P $x)))
   (match &self (Implication $a (Evaluation ($P $x))) (deduce $a)))
(= (deduce (And $a $b)) (And (deduce $a) (deduce $b)))
(= (And T T) T)
!(deduce (Evaluation (mortal Plato)))",
    ),
];

fn go(label: &str, self_mork: bool, prog: &str) {
    let metta = if self_mork {
        Metta::new_with_stdlib_loader(None, Some(DynSpace::new(MorkSpace::new())), None)
    } else {
        Metta::new(None)
    };
    match metta.run(SExprParser::new(prog)) {
        Ok(results) => {
            let last = results.len().saturating_sub(2);
            for (i, res) in results.iter().enumerate() {
                let shown: Vec<String> = res.iter().map(|a| format!("{a}")).collect();
                let mark = if i >= last { "  <-- bang" } else { "" };
                println!("    [{label}/{}] {i}: ({}) {:?}{mark}", if self_mork {"MORK"} else {"GS"}, res.len(), shown);
            }
        }
        Err(e) => println!("    [{label}/{}] err: {e}", if self_mork { "MORK" } else { "GS" }),
    }
}

fn main() {
    for (name, prog) in STAGES {
        println!("== stage: {name} ==");
        go(name, false, prog);
        go(name, true, prog);
        println!();
    }
}
