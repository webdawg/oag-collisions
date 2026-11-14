//! EXP-027: how much does the prefix ladder's recall vary across base documents?
//!
//! ```text
//! cargo run -p oag-collisions --example recall_variance
//! ```
//!
//! EXP-026 measured a second independent hash and found the union's gain modest -- but it also
//! found that two hashes of the *same* corpus recalled 73% and 22%. If the recall is a property of
//! the base document's near-zero bit distribution rather than of the edit, then the ladder's recall
//! is not a stable quantity and a deployment cannot quote one number for it.
//!
//! This measures that directly: many base documents, the same hash, the same edit model, and the
//! distribution of the recall. If the spread is wide, the recall is base-dependent; if it is
//! tight, EXP-026's two hashes were an anomaly.

use oag_collisions::{simhash_with_domain, Tokenizer, MIN_COLLISION_BITS};

/// A deterministic generator, so the corpus is identical on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

fn found(query: u64, item: u64) -> bool {
    let diff = query ^ item;
    diff == 0 || diff.leading_zeros() >= MIN_COLLISION_BITS
}

/// The multi-probe condition: the standard ladder, or the first `MIN_COLLISION_BITS` bits
/// differing in at most `max_flips` positions (which is what probing those perturbed prefixes
/// recovers).
fn found_multi_probe(query: u64, item: u64, max_flips: u32) -> bool {
    let diff = query ^ item;
    if diff == 0 || diff.leading_zeros() >= MIN_COLLISION_BITS {
        return true;
    }
    let first_eight = (diff >> (64 - MIN_COLLISION_BITS)) as u8;
    first_eight.count_ones() <= max_flips
}

fn main() {
    let tokens = 800_usize;
    let budget = 200usize;
    let bases = 24_usize;
    let variants = 24_usize;
    let edit = 32_usize;
    let tokenizer = Tokenizer::WordsWindowBudget { window: 3, target_units: budget };
    let mut rng = Lcg(0x5EED);

    let mut recalls = Vec::new();
    let mut recalls_one_flip = Vec::new();
    let mut recalls_two_flips = Vec::new();
    let mut near_zero_spreads = Vec::new();
    for b in 0..bases {
        // Alphanumeric-only tokens, so the tokenizer does not split them: "b0w12" is one token.
        let flat: Vec<String> = (0..tokens).map(|i| format!("b{b}w{i}")).collect();
        let base_units = tokenizer.tokenize(&flat.join(" "));
        let base_refs: Vec<(&str, i32)> =
            base_units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
        let base_hash = simhash_with_domain(b"", &base_refs);

        // The near-zero bit positions of the base: the bits a small edit can cross.
        let mut sums = [0i32; 64];
        for (unit, weight) in &base_refs {
            let digest = blake3::hash(unit.as_bytes());
            let bits = u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap());
            for (position, slot) in sums.iter_mut().enumerate() {
                *slot += if (bits >> position) & 1 == 1 { *weight } else { -*weight };
            }
        }
        let near_zero: Vec<usize> = (0..64).filter(|&p| sums[p].abs() <= 1).collect();
        let highest = near_zero.iter().copied().max().unwrap_or(0);
        near_zero_spreads.push(highest);

        let mut hits = 0;
        let mut hits_one = 0;
        let mut hits_two = 0;
        for _ in 0..variants {
            let position = (rng.next() % (tokens - edit) as u64) as usize;
            let mut changed = flat.clone();
            for k in 0..edit {
                changed[position + k] = format!("x{}", rng.next() % 1_000_000);
            }
            let units = tokenizer.tokenize(&changed.join(" "));
            let refs: Vec<(&str, i32)> = units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
            let hash = simhash_with_domain(b"", &refs);
            hits += usize::from(found(base_hash, hash));
            hits_one += usize::from(found_multi_probe(base_hash, hash, 1));
            hits_two += usize::from(found_multi_probe(base_hash, hash, 2));
        }
        recalls.push(hits);
        recalls_one_flip.push(hits_one);
        recalls_two_flips.push(hits_two);
    }

    let summarise = |values: &mut Vec<usize>, label: &str| {
        values.sort();
        let mean = values.iter().sum::<usize>() as f64 / values.len() as f64;
        let min = values[0];
        let max = values[values.len() - 1];
        let median = values[values.len() / 2];
        let variance = values
            .iter()
            .map(|r| (*r as f64 - mean).powi(2))
            .sum::<f64>()
            / values.len() as f64;
        println!(
            "{label:>22}: min {min:>2} ({:>5.1}%), median {median:>2} ({:>5.1}%), \
             max {max:>2} ({:>5.1}%), mean {mean:.1}%, std dev {:.1}",
            100.0 * min as f64 / variants as f64,
            100.0 * median as f64 / variants as f64,
            100.0 * max as f64 / variants as f64,
            variance.sqrt()
        );
    };

    println!("EXP-027: the prefix ladder's recall across base documents");
    println!("==========================================================");
    println!(
        "{bases} base documents, {tokens} tokens, window 3, budget {budget}, \
         {variants} variants at a {edit}-token edit each"
    );
    println!();
    println!("recall per base, standard ladder: {recalls:?}");
    println!("recall per base, 1-flip multi-probe: {recalls_one_flip:?}");
    println!();
    summarise(&mut recalls, "standard ladder");
    summarise(&mut recalls_one_flip, "1-flip multi-probe");
    summarise(&mut recalls_two_flips, "2-flip multi-probe");
    println!();
    println!("the highest near-zero bit position per base: {near_zero_spreads:?}");
    println!();
    println!("read the spread as the answer to whether one recall number can be quoted: a wide spread");
    println!("means the recall is a property of the base document, not of the edit model. The");
    println!("multi-probe rows show whether it also stabilises the recall, or only raises it.");
    println!();
}
