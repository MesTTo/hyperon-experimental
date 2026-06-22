//! Bisect e3_match_states: MeTTa State atoms (Rc<RefCell> mutable cells) stored inside
//! equalities on the MORK trie. Run each assertion-bearing fragment on GroundingSpace vs
//! a MORK-backed self, print results, to see exactly which State behaviour diverges.
use hyperon::metta::runner::Metta;
use hyperon::metta::text::SExprParser;
use hyperon_space::DynSpace;
use metta_on_mork::MorkSpace;

const SETUP: &str = "\
(= (new-goal-status! $goal $status)
    (let $new-state (new-state $status)
         (add-atom &self (= (status (Goal $goal)) $new-state))))
! (new-goal-status! lunch-order inactive)
! (new-goal-status! meditation inactive)
";

const STAGES: &[(&str, &str)] = &[
    ("get-initial(=inactive)", "!(get-state (status (Goal lunch-order)))"),
    ("get-meditation(=inactive)", "!(get-state (status (Goal meditation)))"),
    (
        "change-then-get(=active)",
        "!(nop (change-state! (status (Goal lunch-order)) active))
!(get-state (status (Goal lunch-order)))",
    ),
    (
        "meditation-still-inactive",
        "!(nop (change-state! (status (Goal lunch-order)) active))
!(get-state (status (Goal meditation)))",
    ),
    (
        "match-state-active(=meditation)",
        "!(nop (change-state! (status (Goal lunch-order)) active))
!(bind! &state-active (new-state active))
!(nop (change-state! &state-active inactive))
!(match &self (= (status (Goal $goal)) &state-active) $goal)",
    ),
];

fn go(label: &str, self_mork: bool, prog: &str) {
    let metta = if self_mork {
        Metta::new_with_stdlib_loader(None, Some(DynSpace::new(MorkSpace::new())), None)
    } else {
        Metta::new(None)
    };
    let full = format!("{SETUP}{prog}");
    match metta.run(SExprParser::new(&full)) {
        Ok(results) => {
            // Only the trailing bang(s) of `prog` matter; show the last result row.
            if let Some(res) = results.last() {
                let shown: Vec<String> = res.iter().map(|a| format!("{a}")).collect();
                println!("    [{}/{label}] ({}) {:?}", if self_mork {"MORK"} else {"GS"}, res.len(), shown);
            } else {
                println!("    [{}/{label}] (no results)", if self_mork {"MORK"} else {"GS"});
            }
        }
        Err(e) => println!("    [{}/{label}] err: {e}", if self_mork { "MORK" } else { "GS" }),
    }
}

fn main() {
    for (name, prog) in STAGES {
        println!("== {name} ==");
        go(name, false, prog);
        go(name, true, prog);
    }
}
