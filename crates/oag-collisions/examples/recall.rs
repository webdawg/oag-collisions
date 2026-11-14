//! EXP-025: can the prefix filter's recall be widened without a full scan?
//!
//! ```text
//! cargo run -p oag-collisions --example recall
//! ```
//!
//! EXP-024 fixed the ranking signal but not the candidate set: the prefix filter excludes 61% of
//! variants, and the exclusion is steepest for the far near-duplicates. An item whose first
//! differing bit is above [`MIN_COLLISION_BITS`] is invisible at every level, however few bits
//! moved.
//!
//! The standard LSH answer is **multi-probe**: also query the prefixes that differ from the query's
//! in a few high-order bits. `collide_multi_probe` does this. The question is what it buys and
//! what it costs, and both sides are measured here:
//!
//! - **recall** -- the fraction of near-duplicate variants the index finds;
//! - **false positives** -- the fraction of unrelated documents that appear; and
//! - **probe count** -- the number of bucket lookups, which is the cost.
//!
//! An 800-token base, window sampling at width 3 with a budget of 200, 20 variants at each edit
//! distance from 2 to 256, and 160 unrelated documents. The source is an LCG, so a run is exactly
//! reproducible.

use oag_collisions::{CollisionIndex, Tokenizer, HASH_BITS, MIN_COLLISION_BITS};

/// A deterministic generator, so the corpus is identical on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

fn main() {
    let tokens = 800_usize;
    let budget = 200_usize;
    let variants_per_distance = 20_usize;
    let unrelated_count = 160_usize;
    let flat: Vec<String> = (0..tokens).map(|i| format!("w{i}")).collect();
    let tokenizer = Tokenizer::WordsWindowBudget { window: 3, target_units: budget };

    let distances = [2_usize, 4, 8, 16, 32, 64, 128, 256];
    let mut rng = Lcg(0x5EED);

    // The corpus: near-duplicates of the base, plus unrelated documents. Each unrelated document
    // draws from its own token range, so its hash is uniform against the base's rather than
    // sharing a prefix by construction.
    let mut near_ids: Vec<String> = Vec::new();
    let mut unrelated_ids: Vec<String> = Vec::new();
    let mut index = CollisionIndex::with_tokenizer(10_000, HASH_BITS, tokenizer);

    for &distance in &distances {
        for _ in 0..variants_per_distance {
            let position = (rng.next() % (tokens - distance) as u64) as usize;
            let mut changed = flat.clone();
            for k in 0..distance {
                changed[position + k] = format!("x{}", rng.next() % 1_000_000);
            }
            let text = changed.join(" ");
            let units = tokenizer.tokenize(&text);
            let refs: Vec<&str> = units.iter().map(|(u, _)| u.as_str()).collect();
            let id = format!("near-d{distance}-p{position}");
            index.insert(&id, &refs);
            near_ids.push(id);
        }
    }
    for i in 0..unrelated_count {
        let doc: Vec<String> = (0..tokens).map(|k| format!("u{i}k{k}")).collect();
        let text = doc.join(" ");
        let units = tokenizer.tokenize(&text);
        let refs: Vec<&str> = units.iter().map(|(u, _)| u.as_str()).collect();
        let id = format!("unrelated-{i}");
        index.insert(&id, &refs);
        unrelated_ids.push(id);
    }

    // The three schemes. `probe_bits` is clamped to MIN_COLLISION_BITS inside the index, and the
    // probe count is the ladder (MIN_COLLISION_BITS..=HASH_BITS) plus the perturbed prefixes.
    let ladder = HASH_BITS - MIN_COLLISION_BITS + 1;
    let schemes = [
        ("standard (no extra probes)", MIN_COLLISION_BITS, 0),
        ("multi-probe, 1 flip", MIN_COLLISION_BITS, 1),
        ("multi-probe, 2 flips", MIN_COLLISION_BITS, 2),
    ];

    println!("EXP-025: widening the prefix filter's recall");
    println!("==============================================");
    println!(
        "document {tokens} tokens, window 3, budget {budget}, {} near-duplicates, {unrelated_count} unrelated",
        near_ids.len()
    );
    println!();
    println!(
        "{:>30} {:>10} {:>12} {:>14} {:>12}",
        "scheme", "probes", "near found", "unrelated found", "total found"
    );

    for (label, probe_bits, max_flips) in schemes {
        let found = index.collide_multi_probe_text(&flat.join(" "), probe_bits, max_flips);
        let found_ids: std::collections::HashSet<&str> =
            found.iter().map(|c| c.id).collect();
        let near_found = near_ids.iter().filter(|id| found_ids.contains(id.as_str())).count();
        let unrelated_found = unrelated_ids
            .iter()
            .filter(|id| found_ids.contains(id.as_str()))
            .count();
        // The probe count: the ladder plus the perturbed prefixes actually queried.
        let extra = (1..(1_u64 << probe_bits.min(MIN_COLLISION_BITS)))
            .filter(|mask| mask.count_ones() <= max_flips)
            .count();
        println!(
            "{:>30} {:>10} {:>12} {:>14} {:>12}",
            label,
            ladder as usize + extra,
            format!("{}/{}", near_found, near_ids.len()),
            format!("{}/{}", unrelated_found, unrelated_ids.len()),
            found_ids.len()
        );
    }

    // The recall gain is only worth what it costs. Express both as rates over their own sets, and
    // state the trade in the same row: extra near-duplicates found per extra unrelated document.
    println!();
    println!("the trade, stated as rates");
    println!("==========================");
    let baseline = index.collide_multi_probe_text(&flat.join(" "), MIN_COLLISION_BITS, 0);
    let baseline_ids: std::collections::HashSet<&str> = baseline.iter().map(|c| c.id).collect();
    let baseline_near = near_ids.iter().filter(|id| baseline_ids.contains(id.as_str())).count();
    let baseline_unrelated = unrelated_ids
        .iter()
        .filter(|id| baseline_ids.contains(id.as_str()))
        .count();
    println!(
        "baseline: {}/{} near ({:.1}%), {}/{} unrelated ({:.1}%)",
        baseline_near,
        near_ids.len(),
        100.0 * baseline_near as f64 / near_ids.len() as f64,
        baseline_unrelated,
        unrelated_ids.len(),
        100.0 * baseline_unrelated as f64 / unrelated_ids.len() as f64,
    );
    for (label, probe_bits, max_flips) in schemes.iter().skip(1) {
        let found = index.collide_multi_probe_text(&flat.join(" "), *probe_bits, *max_flips);
        let found_ids: std::collections::HashSet<&str> = found.iter().map(|c| c.id).collect();
        let near = near_ids.iter().filter(|id| found_ids.contains(id.as_str())).count();
        let unrelated = unrelated_ids
            .iter()
            .filter(|id| found_ids.contains(id.as_str()))
            .count();
        let extra_near = near.saturating_sub(baseline_near);
        let extra_unrelated = unrelated.saturating_sub(baseline_unrelated);
        println!(
            "{label:>30}: +{extra_near} near, +{extra_unrelated} unrelated \
             ({:.1} near per unrelated)",
            extra_near as f64 / extra_unrelated.max(1) as f64
        );
    }

    // And does the extra recall reach the *closest* documents, or only the far ones? The recall
    // gain that matters is at the small edit distances, where a miss is a near-duplicate lost.
    println!();
    println!("recall per edit distance");
    println!("========================");
    println!(
        "{:>8} {:>12} {:>12} {:>12}",
        "distance", "standard", "1 flip", "2 flips"
    );
    for &distance in &distances {
        let ids: Vec<&String> = near_ids.iter().filter(|id| id.contains(&format!("d{distance}-"))).collect();
        let count = |flips: u32| -> usize {
            let found = index.collide_multi_probe_text(&flat.join(" "), MIN_COLLISION_BITS, flips);
            let found_ids: std::collections::HashSet<&str> = found.iter().map(|c| c.id).collect();
            ids.iter().filter(|id| found_ids.contains(id.as_str())).count()
        };
        println!(
            "{:>8} {:>12} {:>12} {:>12}",
            distance,
            format!("{}/{}", count(0), ids.len()),
            format!("{}/{}", count(1), ids.len()),
            format!("{}/{}", count(2), ids.len()),
        );
    }
    println!();
    println!("read this as: the extra probes buy recall at a cost in false positives, and the");
    println!("recall that matters is at the small edit distances. If the 1-flip scheme recovers the");
    println!("near end without a proportional rise in unrelated hits, it is the one to use; if the");
    println!("unrelated column climbs as fast as the near one, the extra recall is not worth it.");
    println!();
}
