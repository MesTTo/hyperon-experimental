//! Isolate which backend triggers the `matcher.rs:469 Unexpected state` panic seen under the
//! benchmark, on each excluded (both-fail) script. Runs each script on GroundingSpace-only and
//! MORK-only under catch_unwind and reports panicked/ok, so a malformed-Bindings bug in the MORK
//! codec is told apart from a pre-existing baseline interpreter panic.
use std::panic::{catch_unwind, AssertUnwindSafe};

use hyperon::metta::runner::Metta;
use hyperon::metta::text::SExprParser;
use hyperon_space::DynSpace;
use metta_on_mork::MorkSpace;

const SCRIPTS: &str = "/home/user/Dev/hyperon-build-src/python/tests/scripts";

fn run(self_mork: bool, src: &str) -> Result<usize, ()> {
    catch_unwind(AssertUnwindSafe(|| {
        let metta = if self_mork {
            Metta::new_with_stdlib_loader(None, Some(DynSpace::new(MorkSpace::new())), None)
        } else {
            Metta::new(None)
        };
        metta.run(SExprParser::new(src)).map(|r| r.len()).unwrap_or(0)
    }))
    .map_err(|_| ())
}

const REPS: usize = 40;

fn main() {
    let mut paths: Vec<_> = std::fs::read_dir(SCRIPTS)
        .expect("scripts dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map_or(false, |x| x == "metta"))
        .collect();
    paths.sort();
    println!("{REPS} runs/backend. panics counted (HashMap-order-dependent merge).");
    println!("{:<26} {:>16} {:>16}", "script", "GS panics", "MORK panics");
    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(path).unwrap_or_default();
        let gp = (0..REPS).filter(|_| run(false, &src).is_err()).count();
        let mp = (0..REPS).filter(|_| run(true, &src).is_err()).count();
        if gp > 0 || mp > 0 {
            println!("{name:<26} {gp:>16} {mp:>16}");
        }
    }
    println!("(scripts with 0 panics on both backends omitted)");
}
