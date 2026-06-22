//! Trace where the codec drops higher-order results (d2). The kernel matcher finds the
//! match (verified by a kernel test), so an empty result on MORK is a codec/query_btm
//! drop. Run the lambda application on MORK vs GroundingSpace and print result counts.
use hyperon::metta::runner::Metta;
use hyperon::metta::text::SExprParser;
use hyperon_space::DynSpace;
use metta_on_mork::MorkSpace;

const PROG: &str = "\
(= ((lambda $var $body) $arg) (let $var $arg $body))
(= (part-appl $f $x) (lambda $y ($f $x $y)))
(= (inc) (part-appl + 1))
!(part-appl + 1)
!(inc)
!((inc) 5)";

fn go(label: &str, self_mork: bool) {
    let metta = if self_mork {
        Metta::new_with_stdlib_loader(None, Some(DynSpace::new(MorkSpace::new())), None)
    } else {
        Metta::new(None)
    };
    println!("== {label} ==");
    match metta.run(SExprParser::new(PROG)) {
        Ok(results) => {
            for (i, res) in results.iter().enumerate() {
                let shown: Vec<String> = res.iter().map(|a| format!("{a:?}")).collect();
                println!("  expr[{i}] ({} results) = {:?}", res.len(), shown);
            }
        }
        Err(e) => println!("  err: {e}"),
    }
}

fn main() {
    go("GroundingSpace", false);
    go("MORK", true);
}
