//! Hyperon conformance sweep: run the official MeTTa test scripts on a MORK-backed
//! `&self` and on stock GroundingSpace, and report where MORK matches.
//!
//! Each script is self-contained MeTTa with its own `assertEqual*` checks. We run the
//! identical source under both backends; a script "passes" a backend when it produces
//! no `Error` atom, and MORK is "conformant" on it when it passes and yields the same
//! result vector as the GroundingSpace baseline. Panics (e.g. the GroundingSpace trie
//! crash, #1076) are caught per run so one bad script doesn't abort the sweep.
//!
//! Run:  cargo run --release -p mork-demo --example conformance

use std::panic::{catch_unwind, AssertUnwindSafe};

use hyperon::metta::runner::Metta;
use hyperon::metta::text::SExprParser;
use hyperon_atom::Atom;
use hyperon_space::DynSpace;
use metta_on_mork::MorkSpace;

const SCRIPTS: &str = "/home/user/Dev/hyperon-build-src/python/tests/scripts";

/// Outcome of running one script on one backend.
enum Outcome {
    Pass(Vec<Vec<Atom>>),
    Failed(Vec<Vec<Atom>>),
    Panicked,
}

fn run_script(self_mork: bool, src: &str) -> Outcome {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let metta = if self_mork {
            Metta::new_with_stdlib_loader(None, Some(DynSpace::new(MorkSpace::new())), None)
        } else {
            Metta::new(None)
        };
        metta.run(SExprParser::new(src))
    }));
    match result {
        Ok(Ok(results)) => {
            let err = results.iter().flatten().any(|a| a.to_string().contains("Error"));
            if err { Outcome::Failed(results) } else { Outcome::Pass(results) }
        }
        Ok(Err(_)) | Err(_) => Outcome::Panicked,
    }
}

fn label(o: &Outcome) -> &'static str {
    match o {
        Outcome::Pass(_) => "pass",
        Outcome::Failed(_) => "FAIL",
        Outcome::Panicked => "PANIC",
    }
}

fn main() {
    let mut paths: Vec<_> = std::fs::read_dir(SCRIPTS)
        .expect("scripts dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map_or(false, |x| x == "metta"))
        .collect();
    paths.sort();

    println!("{:<26} {:<7} {:<7} {}", "script", "ground", "mork", "match");
    println!("{}", "-".repeat(56));

    let (mut conformant, mut total, mut mork_only_fail) = (0, 0, 0);
    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(path).unwrap_or_default();
        let base = run_script(false, &src);
        let mork = run_script(true, &src);

        // "match" is meaningful only when both produced results.
        let same = match (&base, &mork) {
            (Outcome::Pass(b), Outcome::Pass(m)) | (Outcome::Failed(b), Outcome::Failed(m)) => b == m,
            _ => false,
        };
        let base_ok = matches!(base, Outcome::Pass(_));
        let mork_ok = matches!(mork, Outcome::Pass(_));
        total += 1;
        if base_ok && mork_ok && same {
            conformant += 1;
        }
        if base_ok && !mork_ok {
            mork_only_fail += 1;
        }
        println!(
            "{:<26} {:<7} {:<7} {}",
            name,
            label(&base),
            label(&mork),
            if same { "yes" } else { "no" }
        );
    }

    println!("{}", "-".repeat(56));
    println!("conformant (both pass, identical): {conformant}/{total}");
    println!("regressions (baseline passes, MORK does not): {mork_only_fail}");
}
