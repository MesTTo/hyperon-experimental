//! Hyperon-compatibility, proven on real code: run the unmodified trueagi-io/chaining
//! `dtl` forward and backward chainers on the MORK backend and on stock GroundingSpace,
//! and check the runs are identical.
//!
//! Three backend configurations per program:
//!   - baseline : &self GroundingSpace, &kb GroundingSpace  (the reference)
//!   - kb-mork  : &self GroundingSpace, &kb MORK            (data path on MORK)
//!   - full-mork: &self MORK,          &kb MORK            (drop-in: the whole runner on MORK)
//!
//! The chainers pull rules and facts from `&kb` with `get-atoms` / `match`, so their data
//! path runs on whichever backend `&kb` uses. `full-mork` additionally puts the program and
//! the entire stdlib into a MORK `&self`, exercising the grounded-atom round-trip. The
//! chainer source is byte-for-byte the upstream file; only the space backend changes.
//!
//! Run:  cargo run --release -p mork-demo --example chaining_compat

use hyperon::metta::runner::Metta;
use hyperon::metta::text::SExprParser;
use hyperon::space::grounding::GroundingSpace;
use hyperon_atom::Atom;
use hyperon_space::DynSpace;
use metta_on_mork::MorkSpace;
use regex::Regex;

const CHAINING: &str = "/home/user/Dev/chaining";

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{CHAINING}/{rel}"))
        .unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// Drops the module ceremony (`register-module!` / `import!`) and the `bind! &kb` line;
/// `&kb` is provided externally so the backend is the only variable between runs.
fn strip_module_lines(src: &str) -> String {
    src.lines()
        .filter(|l| {
            let t = l.trim_start();
            !(t.contains("register-module!") || t.contains("import!") || t.contains("bind! &kb"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn forward_program() -> String {
    let utils = read("dtl/utils.metta");
    let fc = read("dtl/forward/no-curry.metta");
    let test = strip_module_lines(&read("dtl/tests/test-forward-chaining-no-curry.metta"));
    format!("{utils}\n{fc}\n{test}")
}

fn backward_program() -> String {
    let utils = read("dtl/utils.metta");
    let bc = read("dtl/backward/no-curry.metta");
    let test = strip_module_lines(&read("dtl/tests/test-backward-chaining-no-curry.metta"));
    format!("{utils}\n{bc}\n{test}")
}

#[derive(Clone, Copy)]
struct Config {
    label: &'static str,
    self_mork: bool,
    kb_mork: bool,
}

/// Runs `program` under one backend configuration.
fn run(cfg: Config, program: &str) -> Result<Vec<Vec<Atom>>, String> {
    let metta = if cfg.self_mork {
        Metta::new_with_stdlib_loader(None, Some(DynSpace::new(MorkSpace::new())), None)
    } else {
        Metta::new(None)
    };
    let kb: DynSpace = if cfg.kb_mork {
        DynSpace::new(MorkSpace::new())
    } else {
        DynSpace::new(GroundingSpace::new())
    };
    let kb_atom = Atom::gnd(kb);
    metta
        .tokenizer()
        .borrow_mut()
        .register_token(Regex::new(r"&kb").unwrap(), move |_| kb_atom.clone());
    metta.run(SExprParser::new(program))
}

/// True when some result atom is an `Error` (a failed assertion).
fn has_error(results: &[Vec<Atom>]) -> bool {
    results
        .iter()
        .flatten()
        .any(|a| a.to_string().contains("Error"))
}

const BASELINE: Config = Config { label: "baseline ", self_mork: false, kb_mork: false };
const KB_MORK: Config = Config { label: "kb-mork  ", self_mork: false, kb_mork: true };
const FULL_MORK: Config = Config { label: "full-mork", self_mork: true, kb_mork: true };

/// Runs one program across all configs and reports per-config pass + match-to-baseline.
fn report(name: &str, program: &str) -> bool {
    println!("\n===== {name} chaining =====");
    let base = run(BASELINE, program);
    let Ok(base_results) = &base else {
        println!("  baseline errored: {:?}", base);
        return false;
    };
    let base_pass = !has_error(base_results);
    println!("  {} : pass={base_pass} (reference)", BASELINE.label);
    let mut all_ok = base_pass;
    for cfg in [KB_MORK, FULL_MORK] {
        match run(cfg, program) {
            Ok(r) => {
                let pass = !has_error(&r);
                let same = &r == base_results;
                println!("  {} : pass={pass}  identical-to-baseline={same}", cfg.label);
                all_ok &= pass && same;
            }
            Err(e) => {
                println!("  {} : ERROR {e}", cfg.label);
                all_ok = false;
            }
        }
    }
    all_ok
}

fn main() {
    let f = report("forward", &forward_program());
    let b = report("backward", &backward_program());
    println!("\n== verdict ==");
    if f && b {
        println!("  COMPATIBLE: both real chainers run identically on MORK, including the full");
        println!("  drop-in (program + stdlib + kb all on the MORK trie).");
    } else {
        println!("  DIVERGENCE in at least one config; see above.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_compatible(name: &str, program: &str, cfg: Config) {
        let base = run(BASELINE, program).expect("baseline run");
        assert!(!has_error(&base), "{name}: baseline asserts failed: {base:?}");
        let got = run(cfg, program).expect("config run");
        assert!(!has_error(&got), "{name}: {} asserts failed: {got:?}", cfg.label);
        assert_eq!(base, got, "{name}: {} differs from baseline", cfg.label);
    }

    #[test]
    fn forward_kb_on_mork() {
        assert_compatible("forward", &forward_program(), KB_MORK);
    }

    #[test]
    fn forward_full_mork() {
        assert_compatible("forward", &forward_program(), FULL_MORK);
    }

    #[test]
    fn backward_kb_on_mork() {
        assert_compatible("backward", &backward_program(), KB_MORK);
    }

    #[test]
    fn backward_full_mork() {
        assert_compatible("backward", &backward_program(), FULL_MORK);
    }
}
