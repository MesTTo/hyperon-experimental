//! Benchmark the official MeTTa scripts on Hyperon GroundingSpace vs a MORK-backed
//! &self. End-to-end per script (build a runner -> load stdlib into &self -> run the
//! script). Reports min and median over N runs. Only scripts where both backends pass
//! and agree are timed apples-to-apples; others are listed but excluded from the total.
//!
//! Run:  cargo run --release -p mork-demo --example bench

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Instant;

use hyperon::metta::runner::Metta;
use hyperon::metta::text::SExprParser;
use hyperon_atom::Atom;
use hyperon_space::DynSpace;
use metta_on_mork::MorkSpace;

const SCRIPTS: &str = "/home/user/Dev/hyperon-build-src/python/tests/scripts";
const RUNS: usize = 30;

fn run_once(self_mork: bool, src: &str) -> Option<Vec<Vec<Atom>>> {
    catch_unwind(AssertUnwindSafe(|| {
        let metta = if self_mork {
            Metta::new_with_stdlib_loader(None, Some(DynSpace::new(MorkSpace::new())), None)
        } else {
            Metta::new(None)
        };
        metta.run(SExprParser::new(src)).ok()
    }))
    .ok()
    .flatten()
}

fn passes(r: &Option<Vec<Vec<Atom>>>) -> bool {
    matches!(r, Some(rs) if !rs.iter().flatten().any(|a| a.to_string().contains("Error")))
}

/// (min_ms, median_ms) over RUNS timed builds+runs.
fn time_backend(self_mork: bool, src: &str) -> (f64, f64) {
    let mut t = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let start = Instant::now();
        let _ = run_once(self_mork, src);
        t.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    t.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (t[0], t[t.len() / 2])
}

fn main() {
    let mut paths: Vec<_> = std::fs::read_dir(SCRIPTS)
        .expect("scripts dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map_or(false, |x| x == "metta"))
        .collect();
    paths.sort();

    println!(
        "{:<26} {:>10} {:>10} {:>10} {:>10}  {}",
        "script", "GS min", "GS med", "MORK min", "MORK med", "med x"
    );
    println!("{}", "-".repeat(82));

    let (mut gs_tot, mut mk_tot) = (0.0f64, 0.0f64);
    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(path).unwrap_or_default();
        // Warm up once (first build pays one-time init); then time.
        let base = run_once(false, &src);
        let mork = run_once(true, &src);
        let comparable = passes(&base) && passes(&mork) && base == mork;
        if !comparable {
            println!("{:<26} {:>10} (excluded: not both-pass-identical)", name, "");
            continue;
        }
        let (gmin, gmed) = time_backend(false, &src);
        let (mmin, mmed) = time_backend(true, &src);
        gs_tot += gmed;
        mk_tot += mmed;
        println!(
            "{:<26} {:>10.3} {:>10.3} {:>10.3} {:>10.3}  {:>5.2}x",
            name, gmin, gmed, mmin, mmed, mmed / gmed
        );
    }
    println!("{}", "-".repeat(82));
    println!(
        "TOTAL (median, comparable)   GroundingSpace {:.2} ms   MORK {:.2} ms   ({:.2}x)",
        gs_tot,
        mk_tot,
        mk_tot / gs_tot
    );
    println!("\nNote: these are small correctness scripts (few atoms); the dominant cost is");
    println!("building the runner + loading stdlib into &self each run, not atomspace scale.");
    println!("MORK's advantage is at scale (1M+ atoms) and parallelism, not tiny scripts.");
}
