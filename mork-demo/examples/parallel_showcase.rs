//! Parallel-querying head-to-head. A MORK `MorkSnapshot` is a `Send + Sync` copy-on-write clone
//! of the trie, so one shared snapshot answers queries from many threads at once. Hyperon's
//! `GroundingSpace` is `Rc<RefCell>`-based (`!Send`/`!Sync`, hyperon-experimental #410), so it
//! cannot be shared across threads at all and must serve queries serially.
//!
//! Loads N edges, then runs Q point queries: GroundingSpace on one thread (its only option),
//! MORK on 1/2/4/8 threads over one `Arc<MorkSnapshot>`, reporting throughput and speedup.
//!
//! Run:  cargo run --release -p mork-demo --example parallel_showcase

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use hyperon::space::grounding::GroundingSpace;
use hyperon_atom::Atom;
use hyperon_space::{Space, SpaceMut};
use metta_on_mork::{MorkSnapshot, MorkSpace};

const N: usize = 500_000; // edges loaded
const Q: usize = 400_000; // total point queries

fn edge(i: usize) -> Atom {
    Atom::expr([
        Atom::sym("edge"),
        Atom::sym(format!("n{i}")),
        Atom::sym(format!("n{}", i + 1)),
    ])
}

fn query(k: usize) -> Atom {
    Atom::expr([Atom::sym("edge"), Atom::sym(format!("n{k}")), Atom::var("d")])
}

/// Spread query keys across the whole space (Knuth multiplicative hash mod N), so threads do not
/// all hit the same trie path.
fn key(j: usize) -> usize {
    j.wrapping_mul(2_654_435_761) % N
}

/// Compiles only for `Send + Sync` types. `MorkSnapshot` passes; `GroundingSpace` would NOT
/// (it is `!Sync`), which is exactly why it cannot be the argument to `thread::spawn` below.
fn assert_send_sync<T: Send + Sync>() {}

fn qps(ms: f64) -> f64 {
    Q as f64 / (ms / 1000.0)
}

fn main() {
    assert_send_sync::<MorkSnapshot>();
    // assert_send_sync::<GroundingSpace>(); // <-- does not compile: GroundingSpace is !Sync (#410)

    println!("Loading {N} edges into each backend...");
    let mut mork = MorkSpace::new();
    for i in 0..N {
        mork.add(edge(i));
    }
    let snap = Arc::new(mork.snapshot());

    // Share only the query *keys* (usize is Send+Sync); each thread builds its own query atoms.
    // `Atom` is itself !Send + !Sync (it can hold a `Box<dyn GroundedAtom>`, and GroundedAtom has
    // no Send/Sync bound), so a `Vec<Atom>` cannot cross or be shared between threads at all -- the
    // whole Hyperon atom model is single-threaded at the type level. MORK shares *bytes* (the
    // snapshot trie) and lets each thread build its transient query atom locally, which is exactly
    // why it parallelizes where GroundingSpace cannot.
    let keys: Arc<Vec<usize>> = Arc::new((0..Q).map(key).collect());

    // GroundingSpace: serial only (cannot be shared across threads). The whole load+query is caught
    // because its trie can panic (#1076) on add or query of a large same-head space.
    let keys_gs = Arc::clone(&keys);
    let gs_res = catch_unwind(AssertUnwindSafe(|| {
        let mut gs = GroundingSpace::new();
        for i in 0..N {
            gs.add(edge(i));
        }
        let t = Instant::now();
        let h: usize = keys_gs.iter().map(|&k| gs.query(&query(k)).len()).sum();
        (t.elapsed().as_secs_f64() * 1000.0, h)
    }));
    match gs_res {
        Ok((gs_ms, h)) => println!(
            "\nGroundingSpace  (1 thread, !Sync so cannot parallelize)  {gs_ms:>8.0}ms  {:>10.0} q/s  hits={h}",
            qps(gs_ms)
        ),
        Err(_) => println!(
            "\nGroundingSpace  PANIC (#1076 trie.rs:179) on {N} same-head edges -- cannot serve the query set at all"
        ),
    }

    println!("\nMORK  (one Arc<MorkSnapshot> shared across threads):");
    let mut base_ms = 0.0;
    for (idx, &threads) in [1usize, 2, 4, 8, 16].iter().enumerate() {
        let per = Q / threads;
        let t = Instant::now();
        let handles: Vec<_> = (0..threads)
            .map(|tid| {
                let snap = Arc::clone(&snap);
                let keys = Arc::clone(&keys);
                thread::spawn(move || {
                    keys[tid * per..(tid + 1) * per]
                        .iter()
                        .map(|&k| snap.query(&query(k)).len())
                        .sum::<usize>()
                })
            })
            .collect();
        let hits: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        if idx == 0 {
            base_ms = ms;
        }
        println!(
            "  {threads:>2} thread(s)  {ms:>8.0}ms  {:>10.0} q/s  {:>4.1}x vs 1-thread  hits={hits}",
            qps(ms),
            base_ms / ms
        );
    }
}
