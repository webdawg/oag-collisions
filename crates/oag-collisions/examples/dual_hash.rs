//! EXP-026: does a second independent hash recover what the prefix filter misses?
//!
//! ```text
//! cargo run -p oag-collisions --example dual_hash
//! ```
//!
//! EXP-025 widened the near end of the recall curve with a 1-flip multi-probe, but the far end is
//! still limited: a variant is found only if its first differing bit is at or below
//! [`MIN_COLLISION_BITS`], and the multi-probe can only perturb a bounded number of high-order bits.
//!
//! A near-duplicate that the prefix ladder misses is not missed because it is far -- it is missed
//! because the bits it happened to flip are high-order. A second hash built from a different
//! domain flips *different* bits, so the two miss independently. This measures whether that
//! independence is worth anything: the recall of one hash, of a second hash, and of their union,
//! over the same corpus EXP-025 used.
//!
//! The document is 800 synthetic tokens, window sampling at width 3 with a budget of 200, 20
//! variants at each edit distance from 2 to 256, and 160 unrelated documents. The source is an LCG,
//! so a run is exactly reproducible.

use oag_collisions::{simhash_with_domain, Tokenizer, MIN_COLLISION_BITS};

/// A deterministic generator, so the corpus is identical on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

/// Whether the prefix ladder would find an item with this hash: its first differing bit from the
/// query must be at or below the collision threshold.
fn found_by_ladder(query: u64, item: u64) -> bool {
    let diff = query ^ item;
    if diff == 0 {
        return true;
    }
    diff.leading_zeros() >= MIN_COLLISION_BITS
}

fn main() {
    let tokens = 800_usize;
    let budget = 200_usize;
    let variants_per_distance = 20_usize;
    let unrelated_count = 160_usize;
    let flat: Vec<String> = (0..tokens).map(|i| format!("w{i}")).collect();
    let tokenizer = Tokenizer::WordsWindowBudget { window: 3, target_units: budget };

    let distances = [2_usize, 4, 8, 16, 32, 64, 128, 256];
    let domain_a: &[u8] = b"oag-collisions/index-a";
    let domain_b: &[u8] = b"oag-collisions/index-b";
    let mut rng = Lcg(0x5EED);

    let base_units = tokenizer.tokenize(&flat.join(" "));
    let base_refs: Vec<(&str, i32)> =
        base_units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
    let base_a = simhash_with_domain(domain_a, &base_refs);
    let base_b = simhash_with_domain(domain_b, &base_refs);

    // For every variant, both hashes and whether each domain's ladder would find it.
    let mut near: Vec<(usize, bool, bool)> = Vec::new();
    for &distance in &distances {
        for _ in 0..variants_per_distance {
            let position = (rng.next() % (tokens - distance) as u64) as usize;
            let mut changed = flat.clone();
            for k in 0..distance {
                changed[position + k] = format!("x{}", rng.next() % 1_000_000);
            }
            let units = tokenizer.tokenize(&changed.join(" "));
            let refs: Vec<(&str, i32)> = units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
            let a = simhash_with_domain(domain_a, &refs);
            let b = simhash_with_domain(domain_b, &refs);
            near.push((
                distance,
                found_by_ladder(base_a, a),
                found_by_ladder(base_b, b),
            ));
        }
    }

    // The same for unrelated documents, which measure the false-positive cost of any extra recall.
    let mut unrelated: Vec<(bool, bool)> = Vec::new();
    for i in 0..unrelated_count {
        let doc: Vec<String> = (0..tokens).map(|k| format!("u{i}k{k}")).collect();
        let units = tokenizer.tokenize(&doc.join(" "));
        let refs: Vec<(&str, i32)> = units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
        let a = simhash_with_domain(domain_a, &refs);
        let b = simhash_with_domain(domain_b, &refs);
        unrelated.push((found_by_ladder(base_a, a), found_by_ladder(base_b, b)));
    }

    let rate = |found: usize, total: usize| 100.0 * found as f64 / total as f64;
    let (a_only, b_only, union, both) = near.iter().fold((0, 0, 0, 0), |(a, b, u, both), ( _, fa, fb)| {
        (a + usize::from(*fa), b + usize::from(*fb), u + usize::from(*fa || *fb), both + usize::from(*fa && *fb))
    });

    println!("EXP-026: is a second independent hash worth its memory?");
    println!("==========================================================");
    println!(
        "document {tokens} tokens, window 3, budget {budget}, {} near-duplicates, {unrelated_count} unrelated",
        near.len()
    );
    println!();
    println!("panel 1: the recall of one hash and of two, over the near-duplicates");
    println!("=====================================================================");
    println!(
        "{:>32} {:>12} {:>12} {:>12}",
        "scheme", "found", "of", "rate"
    );
    println!(
        "{:>32} {:>12} {:>12} {:>11.1}%",
        "hash A alone", a_only, near.len(), rate(a_only, near.len())
    );
    println!(
        "{:>32} {:>12} {:>12} {:>11.1}%",
        "hash B alone", b_only, near.len(), rate(b_only, near.len())
    );
    println!(
        "{:>32} {:>12} {:>12} {:>11.1}%",
        "union of A and B", union, near.len(), rate(union, near.len())
    );
    println!(
        "{:>32} {:>12} {:>12} {:>11.1}%",
        "found by both (agreement)", both, near.len(), rate(both, near.len())
    );
    println!();
    println!("the union is the recall a dual-hash index would deliver; the agreement is how often the");
    println!("two hashes make the same decision, which is what leaves room for the union to help.");

    println!();
    println!("panel 2: the false-positive cost, over the unrelated documents");
    println!("=================================================================");
    let (ua, ub, uunion) = unrelated.iter().fold((0, 0, 0), |(a, b, u), (fa, fb)| {
        (a + usize::from(*fa), b + usize::from(*fb), u + usize::from(*fa || *fb))
    });
    println!(
        "{:>32} {:>12} {:>12} {:>11.1}%",
        "hash A alone", ua, unrelated.len(), rate(ua, unrelated.len())
    );
    println!(
        "{:>32} {:>12} {:>12} {:>11.1}%",
        "hash B alone", ub, unrelated.len(), rate(ub, unrelated.len())
    );
    println!(
        "{:>32} {:>12} {:>12} {:>11.1}%",
        "union of A and B", uunion, unrelated.len(), rate(uunion, unrelated.len())
    );
    println!();
    println!("every extra recall is bought with false positives. The union's unrelated rate is what");
    println!("a dual-hash index would admit; compare it with the recall gain in panel 1.");

    println!();
    println!("panel 3: recall per edit distance, one hash versus two");
    println!("=======================================================");
    println!(
        "{:>8} {:>12} {:>12} {:>12}",
        "distance", "hash A", "hash B", "union"
    );
    for &distance in &distances {
        let rows: Vec<(usize, bool, bool)> =
            near.iter().filter(|(d, _, _)| *d == distance).cloned().collect();
        let a = rows.iter().filter(|(_, fa, _)| *fa).count();
        let b = rows.iter().filter(|(_, _, fb)| *fb).count();
        let u = rows.iter().filter(|(_, fa, fb)| *fa || *fb).count();
        println!(
            "{:>8} {:>12} {:>12} {:>12}",
            distance,
            format!("{}/{}", a, rows.len()),
            format!("{}/{}", b, rows.len()),
            format!("{}/{}", u, rows.len()),
        );
    }
    println!();
    println!("read this as: the union's gain over the better single hash is the recall a second index");
    println!("would add, and the union's unrelated rate in panel 2 is what it costs. If the gain is");
    println!("large where the miss matters and the cost stays near the single-hash rate, a dual-hash");
    println!("index is worth its memory; if the cost climbs with the gain, it is not.");
    println!();
}
