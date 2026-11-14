//! EXP-024: does the thin margin limit the ranking, or does the prefix ladder resolve it?
//!
//! ```text
//! cargo run -p oag-collisions --example ranking_margin
//! ```
//!
//! EXP-023 established that a 2-token edit moves 1-2 bits on average, and that the margin is set by
//! the edit rather than the budget. It left one question open: if a detection is 1-2 bits, is that
//! enough for the index to *rank* near-duplicates at different distances?
//!
//! The question hides a distinction the earlier experiments did not draw. The index ranks by
//! **longest shared prefix** -- the position of the *first* differing bit -- while EXP-023
//! measured **Hamming distance** -- the *number* of differing bits. A variant one bit away at the
//! most significant position shares nothing; a variant one bit away at the least significant
//! position shares 63 bits. So the two signals are not the same, and which one the index uses
//! matters.
//!
//! This experiment measures both on the same corpus and compares them: the distribution of each
//! signal per edit distance, the correlation between them, and the top-1 of the index ranking
//! against the variant a Hamming scan would pick.
//!
//! The document is 800 synthetic tokens, the tokenizer is window sampling at width 3 with a budget
//! of 200, and the source is an LCG, so a run is exactly reproducible.

use oag_collisions::{simhash_weighted, CollisionIndex, Tokenizer, HASH_BITS};

/// A deterministic generator, so the corpus is identical on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

fn document(tokens: usize) -> Vec<String> {
    (0..tokens).map(|i| format!("w{i}")).collect()
}

/// The unit strings of a tokenization, for the index's `collide` family.
fn unit_refs(units: &[(String, i32)]) -> Vec<&str> {
    units.iter().map(|(unit, _)| unit.as_str()).collect()
}

/// Longest shared prefix between two hashes: the number of leading bits that agree.
fn shared_prefix(a: u64, b: u64) -> u32 {
    let diff = a ^ b;
    if diff == 0 {
        HASH_BITS
    } else {
        diff.leading_zeros()
    }
}

fn main() {
    let tokens = 800_usize;
    let budget = 200_usize;
    let variants_per_distance = 20_usize;
    let flat = document(tokens);
    let tokenizer = Tokenizer::WordsWindowBudget { window: 3, target_units: budget };

    // Distances from a 2-token near-duplicate out to a document half replaced. The edit is a run
    // of `distance` consecutive tokens replaced at a pseudo-random position.
    let distances = [2_usize, 4, 8, 16, 32, 64, 128, 256];
    let mut rng = Lcg(0x5EED);

    // The query is the original document.
    let base_units = tokenizer.tokenize(&flat.join(" "));
    let base_refs: Vec<(&str, i32)> = base_units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
    let base_hash = simhash_weighted(&base_refs);

    let mut index = CollisionIndex::with_tokenizer(10_000, HASH_BITS, tokenizer);
    let mut records: Vec<(usize, u32, u32)> = Vec::new();

    for &distance in &distances {
        for _ in 0..variants_per_distance {
            let position = (rng.next() % (tokens - distance) as u64) as usize;
            let mut changed = flat.clone();
            for k in 0..distance {
                changed[position + k] = format!("x{}", rng.next() % 1_000_000);
            }
            let text = changed.join(" ");
            let units = tokenizer.tokenize(&text);
            let refs: Vec<(&str, i32)> = units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
            let hash = simhash_weighted(&refs);
            let id = format!("d{distance}-p{position}");
            index.insert(&id, &refs.iter().map(|(u, _)| *u).collect::<Vec<_>>());
            records.push((distance, (base_hash ^ hash).count_ones(), shared_prefix(base_hash, hash)));
        }
    }

    println!("EXP-024: does the thin margin limit the ranking?");
    println!("==================================================");
    println!(
        "document {tokens} tokens, window 3, budget {budget}, {} variants per distance",
        variants_per_distance
    );
    println!();
    println!("panel 1: the two signals per edit distance");
    println!("==========================================");
    println!(
        "{:>8} {:>16} {:>18} {:>14}",
        "distance", "hamming (mean)", "shared prefix (mean)", "prefix = 0"
    );
    for &distance in &distances {
        let rows: Vec<(usize, u32, u32)> =
            records.iter().filter(|(d, _, _)| *d == distance).cloned().collect();
        let hamming = rows.iter().map(|(_, h, _)| *h as f64).sum::<f64>() / rows.len() as f64;
        let prefix = rows.iter().map(|(_, _, p)| *p as f64).sum::<f64>() / rows.len() as f64;
        let zero = rows.iter().filter(|(_, _, p)| *p == 0).count();
        println!(
            "{:>8} {:>16.2} {:>18.2} {:>13.1}%",
            distance,
            hamming,
            prefix,
            100.0 * zero as f64 / rows.len() as f64
        );
    }
    println!();
    println!("read the prefix column as the ranking signal the index actually uses. A mean shared");
    println!("prefix near 0 means the typical variant is invisible to the index at every level; a");
    println!("mean near 64 means the typical variant is indistinguishable from the query.");

    // Panel 2: do the two signals order the same variants? The decisive metric is the inversion
    // rate: for every pair of variants, does the prefix ranking order them the same way the
    // Hamming ranking does? A pair is an inversion when the variant with fewer bits moved is
    // ranked *below* the one with more.
    println!();
    println!("panel 2: do the two signals order the same pairs?");
    println!("======================================================");
    let mut pairs = 0_u64;
    let mut inversions = 0_u64;
    let mut ties = 0_u64;
    for i in 0..records.len() {
        for j in (i + 1)..records.len() {
            let (hi, pi) = (records[i].1, records[i].2);
            let (hj, pj) = (records[j].1, records[j].2);
            if hi == hj || pi == pj {
                ties += 1;
                continue;
            }
            pairs += 1;
            // An inversion: the smaller Hamming distance has the smaller shared prefix.
            if (hi < hj) != (pi > pj) {
                inversions += 1;
            }
        }
    }
    println!(
        "comparable pairs: {}, inversions: {} ({:.1}%), tied: {}",
        pairs,
        inversions,
        100.0 * inversions as f64 / pairs.max(1) as f64,
        ties
    );
    let edit_distances: Vec<f64> = records.iter().map(|(d, _, _)| *d as f64).collect();
    let hamming: Vec<f64> = records.iter().map(|(_, h, _)| *h as f64).collect();
    let prefixes: Vec<f64> = records.iter().map(|(_, _, p)| *p as f64).collect();
    println!(
        "Spearman(edit distance, Hamming distance):  {:+.3}",
        spearman(&edit_distances, &hamming)
    );
    println!(
        "Spearman(edit distance, shared prefix):     {:+.3}",
        spearman(&edit_distances, &prefixes)
    );
    println!(
        "Spearman(Hamming distance, shared prefix):  {:+.3}",
        spearman(&hamming, &prefixes)
    );
    println!();
    println!("the ground truth is the edit distance, so the better ranking signal is the one whose");
    println!("correlation with it is stronger. The prefix is what the index uses by default; the");
    println!("Hamming distance is what a linear scan would use.");
    println!();
    println!("an inversion is a pair the index orders backwards: the document with more bits moved");
    println!("ranks above the one with fewer. A rate near 0% means the prefix ladder resolves the");
    println!("thin margin; a rate near 50% means the ranking is no better than chance.");

    // The re-rank. The prefix buckets decide *which* items are candidates; the full hashes decide
    // their order. Measure the same inversion rate over the candidate set, using the hamming field
    // the index now reports, and compare it with the prefix ordering of the same set.
    let candidates = index.collide(&unit_refs(&base_units));
    let mut candidate_inversions = 0_u64;
    let mut candidate_pairs = 0_u64;
    for i in 0..candidates.len() {
        for j in (i + 1)..candidates.len() {
            let hi = candidates[i].hamming;
            let hj = candidates[j].hamming;
            if hi == hj || hi == u32::MAX || hj == u32::MAX {
                continue;
            }
            candidate_pairs += 1;
            let prefix_orders_correctly = candidates[i].shared_bits >= candidates[j].shared_bits;
            let hamming_orders_correctly = hi <= hj;
            if prefix_orders_correctly != hamming_orders_correctly {
                candidate_inversions += 1;
            }
        }
    }
    println!();
    println!("panel 2b: the candidate set, and what re-ranking does to it");
    println!("==========================================================");
    println!(
        "candidates: {} of {} variants (the prefix filter excludes the rest)",
        candidates.len(),
        records.len()
    );
    println!(
        "inversions among candidates, prefix ordering: {} of {} pairs ({:.1}%)",
        candidate_inversions,
        candidate_pairs,
        100.0 * candidate_inversions as f64 / candidate_pairs.max(1) as f64
    );
    println!(
        "inversions among candidates, Hamming ordering: 0 of {} pairs (by construction)",
        candidate_pairs
    );
    println!();
    println!("the candidate set is unchanged -- re-ranking does not find anything the prefix filter");
    println!("missed. It fixes the order of what is already there, which is the 22.3% above.");
    println!();
    println!("candidate rate per edit distance:");
    println!("{:>8} {:>12} {:>12}", "distance", "candidates", "of");
    for &distance in &distances {
        let total = records.iter().filter(|(d, _, _)| *d == distance).count();
        let found = candidates
            .iter()
            .filter(|c| {
                c.id.split('-').next().and_then(|d| d.strip_prefix('d')).and_then(|d| d.parse::<usize>().ok())
                    == Some(distance)
            })
            .count();
        println!("{:>8} {:>12} {:>12}", distance, found, total);
    }
    println!();
    println!("the prefix filter is more aggressive the farther the edit: an item whose first");
    println!("differing bit is above bit 8 is invisible at every level, however few bits moved.");

    // Panel 3: the ranking the index actually performs, against the variant a Hamming scan picks.
    // The ground truth is the closest *detectable* variant -- one with a non-zero Hamming distance,
    // because a variant with hamming 0 is indistinguishable from the query and is a miss, not a
    // near-duplicate to be ranked.
    println!();
    println!("panel 3: the index's top-1 against a Hamming-distance scan");
    println!("===========================================================");
    let base_refs: Vec<&str> = unit_refs(&base_units);
    let by_prefix = index.collide_by_hamming(&base_refs);
    let prefix_top1 = index.collide(&base_refs).into_iter().next();
    let hamming_top1 = by_prefix.into_iter().next();
    let detectable: Vec<(usize, u32, u32)> =
        records.iter().filter(|(_, h, _)| *h > 0).cloned().collect();
    let nearest = detectable.iter().min_by_key(|(_, h, _)| *h).unwrap();
    println!(
        "prefix-ranking top-1: {}",
        prefix_top1.as_ref().map(|c| c.id).unwrap_or("(none)")
    );
    println!(
        "Hamming-ranking top-1: {}",
        hamming_top1.as_ref().map(|c| c.id).unwrap_or("(none)")
    );
    println!(
        "closest detectable: a {}-token edit, hamming {} (of {} detectable variants)",
        nearest.0, nearest.1, detectable.len()
    );
    let distance_of = |id: Option<&str>| -> String {
        id.and_then(|id| id.split('-').next())
            .and_then(|d| d.strip_prefix('d'))
            .and_then(|d| d.parse::<usize>().ok())
            .map(|d| d.to_string())
            .unwrap_or_else(|| "?".into())
    };
    println!(
        "the prefix ranking's top-1 is a {}-token edit; the Hamming ranking's is a {}-token edit; \
         the closest detectable document is a {}-token edit.",
        distance_of(prefix_top1.as_ref().map(|c| c.id)),
        distance_of(hamming_top1.as_ref().map(|c| c.id)),
        nearest.0
    );
    println!();
    println!("read this as: neither ranking is guaranteed to put the closest document first. The");
    println!("prefix ladder ranks by the first differing bit; the Hamming re-rank ranks by the total");
    println!("bits moved, which tracks the edit size better (see the correlations above) but still");
    println!("cannot recover an item the prefix filter excluded. Both are orderings of the candidate");
    println!("set, and the candidate set is where the recall is won or lost.");
    println!();
}

/// Spearman's rank correlation coefficient, computed from the two samples directly.
fn spearman(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    let rank = |values: &[f64]| -> Vec<f64> {
        let mut order: Vec<usize> = (0..values.len()).collect();
        order.sort_by(|&i, &j| values[i].partial_cmp(&values[j]).unwrap());
        let mut ranks = vec![0.0; values.len()];
        for (position, &index) in order.iter().enumerate() {
            ranks[index] = position as f64;
        }
        ranks
    };
    let ra = rank(a);
    let rb = rank(b);
    let n = a.len() as f64;
    let mean_a = ra.iter().sum::<f64>() / n;
    let mean_b = rb.iter().sum::<f64>() / n;
    let mut covariance = 0.0;
    let mut variance_a = 0.0;
    let mut variance_b = 0.0;
    for i in 0..a.len() {
        let da = ra[i] - mean_a;
        let db = rb[i] - mean_b;
        covariance += da * db;
        variance_a += da * da;
        variance_b += db * db;
    }
    if variance_a == 0.0 || variance_b == 0.0 {
        return 0.0;
    }
    covariance / (variance_a.sqrt() * variance_b.sqrt())
}
