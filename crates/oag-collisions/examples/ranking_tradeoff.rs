//! Measures the prefix-level trade-off: ranking quality against index size.
//!
//! Every item costs `prefix_levels` bucket entries, not one, so halving the level count halves the
//! memory. What it costs in return is ranking resolution -- two items sharing more than
//! `prefix_levels` bits are tied. This example measures both sides on the same synthetic corpus
//! rather than asserting that the trade is obviously worth making.
//!
//! Run with `cargo run -p oag-collisions --example ranking_tradeoff`.

use oag_collisions::{tokenize, CollisionIndex, HASH_BITS};

/// A deterministic generator, so the corpus is identical on every run. A tiny LCG is enough; this
/// is test data, not a beacon.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

/// Builds a document of `words` tokens, then returns variants with the first `replaced` tokens
/// swapped for unrelated ones. Variant 0 is the original.
fn corpus(words: usize, variants: usize) -> Vec<Vec<String>> {
    let mut rng = Lcg(0x5EED);
    let mut base: Vec<String> = (0..words).map(|i| format!("w{i}")).collect();
    let mut out = Vec::new();
    for v in 0..variants {
        let replaced = v * 2; // 0, 2, 4, ... tokens changed
        let mut variant = base.clone();
        for i in 0..replaced {
            variant[i] = format!("x{}", rng.next() % 1_000_000);
        }
        out.push(variant);
        // Keep `base` fixed: the query is the unmodified document.
        let _ = &mut base;
    }
    out
}

fn main() {
    let words = 200;
    let variants = 60; // 0..=59, i.e. 0,2,...,118 tokens replaced
    let documents = corpus(words, variants);
    let query = &documents[0]; // the original

    // The correct nearest neighbour is the variant with the fewest replacements -- variant 1.
    // Everything else is progressively less similar, so a correct index must rank variant 1 first.
    let correct = 1usize;

    println!("ranking trade-off: prefix levels vs quality vs size");
    println!("===================================================");
    println!("corpus: {variants} variants of a {words}-token document, 0..={} tokens replaced", (variants - 1) * 2);
    println!("query:  the original document");
    println!("correct answer: variant {correct} (2 tokens replaced; variant 0 is the query itself and is not indexed)");
    println!();
    println!(
        "{:>6} {:>10} {:>12} {:>14} {:>12}",
        "levels", "top-1 ok", "mean rank", "bucket entries", "entry ratio"
    );

    let mut baseline_entries = 0usize;
    for levels in [HASH_BITS, 32, 16, 8] {
        let mut index = CollisionIndex::with_capacity_and_levels(10_000, levels);
        // Skip variant 0: it *is* the query, and an index that returns the identical document
        // ahead of the near-duplicate is correct, not confused.
        for (position, document) in documents.iter().enumerate().skip(1) {
            let refs: Vec<&str> = document.iter().map(String::as_str).collect();
            index.insert(&format!("variant-{position}"), &refs);
        }

        // Ask the query against every variant. Rank is the position of the correct variant.
        let refs: Vec<&str> = query.iter().map(String::as_str).collect();
        let results = index.collide(&refs);
        let rank = results
            .iter()
            .position(|collision| collision.id == format!("variant-{correct}"))
            .map(|position| position + 1);

        let entries = index.bucket_entries();
        if baseline_entries == 0 {
            baseline_entries = entries;
        }
        println!(
            "{:>6} {:>10} {:>12} {:>14} {:>11.2}x",
            levels,
            if rank == Some(1) { "yes" } else { "NO" },
            rank.map(|r| r.to_string()).unwrap_or_else(|| "absent".into()),
            entries,
            entries as f64 / baseline_entries as f64,
        );
    }

    println!();
    println!("read this as: the fewest levels that still gets top-1 right is the one to ship.");
    println!("below that, the index is saving memory by getting the answer wrong.");
    let _ = tokenize("tokenization is exercised elsewhere");
}
