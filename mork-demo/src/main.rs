//! MeTTa-On-Mork: run real MeTTa programs on a MORK-backed atomspace. The Metta
//! runner's &self space is a MorkSpace (via DynSpace), so `add-atom` writes into
//! the MORK trie through MorkSpace's byte-level codec and `match` queries it.
use hyperon::metta::runner::Metta;
use hyperon::metta::text::SExprParser;
use hyperon_space::DynSpace;
use metta_on_mork::MorkSpace;

fn main() {
    let metta = Metta::new_with_stdlib_loader(None, Some(DynSpace::new(MorkSpace::new())), None);
    let program = "\
!(add-atom &self (parent Tom Bob))
!(add-atom &self (parent Bob Ann))
!(match &self (parent Tom $x) $x)
!(match &self (parent $p $c) ($p -> $c))";
    match metta.run(SExprParser::new(program)) {
        Ok(results) => {
            for (i, r) in results.iter().enumerate() {
                println!("expr[{}] => {:?}", i, r);
            }
        }
        Err(e) => println!("error: {}", e),
    }
    println!("MORK-backed &self space now holds: {}", metta.space());
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperon_atom::Atom;

    #[test]
    fn metta_match_executes_on_mork() {
        let metta =
            Metta::new_with_stdlib_loader(None, Some(DynSpace::new(MorkSpace::new())), None);
        let results = metta
            .run(SExprParser::new(
                "\
!(add-atom &self (parent Tom Bob))
!(add-atom &self (parent Bob Ann))
!(match &self (parent Tom $x) $x)",
            ))
            .unwrap();
        // expr 0,1 are add-atom (=> ()); expr 2 is the match.
        assert_eq!(results.len(), 3);
        assert_eq!(results[2], vec![Atom::sym("Bob")]);
    }
}
