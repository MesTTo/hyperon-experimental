//! Diagnostic: run one MeTTa script on a MORK-backed &self with no panic guard, so a
//! backtrace surfaces. Usage: cargo run -p mork-demo --example diag -- <script.metta>
use hyperon::metta::runner::Metta;
use hyperon::metta::text::SExprParser;
use hyperon_space::DynSpace;
use metta_on_mork::MorkSpace;

fn main() {
    let path = std::env::args().nth(1).expect("usage: diag <script.metta>");
    let src = std::fs::read_to_string(&path).expect("read script");
    let metta = Metta::new_with_stdlib_loader(None, Some(DynSpace::new(MorkSpace::new())), None);
    match metta.run(SExprParser::new(&src)) {
        Ok(results) => {
            for (i, r) in results.iter().enumerate() {
                println!("expr[{i}] => {r:?}");
            }
        }
        Err(e) => println!("error: {e}"),
    }
}
