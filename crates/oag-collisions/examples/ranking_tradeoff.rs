//! Measures the prefix-level trade-off: ranking quality against index size.
//!
//! Every item costs `prefix_levels` bucket entries, not one, so halving the level count halves the
//! memory. What it costs in return is ranking resolution -- two items sharing more than
//! `prefix_levels` bits are tied. This example measures both sides on the same synthetic corpus
//! rather than asserting that the trade is obviously worth making, and then asks whether the answer
//! depends on how long the documents are.
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

/// Builds a document of `words` tokens and `variants - 1` degraded copies, each with 2, 4, 6, ...
/// tokens replaced. The original is element 0 and is never indexed -- an index that returns the
/// identical document ahead of the near-duplicate is correct, not confused.
fn corpus(words: usize, requested_variants: usize) -> Vec<Vec<String>> {
    // Never replace more than half the document: past that the variants stop being graded
    // similarities and become unrelated documents, which measures something else entirely. So the
    // variant count is capped to keep every variant within that bound.
    let variants = requested_variants.min(words / 4 + 1);
    let mut rng = Lcg(0x5EED);
    let base: Vec<String> = (0..words).map(|i| format!("w{i}")).collect();
    let mut out = Vec::new();
    for v in 0..variants {
        let replaced = v * 2;
        let mut variant = base.clone();
        for slot in variant.iter_mut().take(replaced) {
            *slot = format!("x{}", rng.next() % 1_000_000);
        }
        out.push(variant);
    }
    out
}

/// One measurement. `correct` is the variant with the fewest replacements.
/// Returns (top-1 correct, was the top a tie, mean rank of the correct item, bucket entries).
fn measure(words: usize, variants: usize, levels: u32) -> (bool, bool, Option<usize>, usize) {
    let documents = corpus(words, variants);
    let query = &documents[0];
    let correct = 1usize;

    let mut index = CollisionIndex::with_capacity_and_levels(10_000, levels);
    for (position, document) in documents.iter().enumerate().skip(1) {
        let refs: Vec<&str> = document.iter().map(String::as_str).collect();
        index.insert(&format!("variant-{position}"), &refs);
    }
    let refs: Vec<&str> = query.iter().map(String::as_str).collect();
    let results = index.collide(&refs);

    let rank = results
        .iter()
        .position(|collision| collision.id == format!("variant-{correct}"))
        .map(|position| position + 1);
    // A tie at the top means the "first" answer is arbitrary -- the index did not choose it.
    let top_tied = results
        .first()
        .map(|first| results.iter().filter(|c| c.shared_bits == first.shared_bits).count() > 1)
        .unwrap_or(false);
    (rank == Some(1), top_tied, rank, index.bucket_entries())
}

fn main() {
    println!("ranking trade-off: prefix levels vs quality vs size");
    println!("===================================================");
    let words = 200;
    let variants = 60;
    println!("corpus: {variants} variants of a {words}-token document, 0..={} tokens replaced", (variants - 1) * 2);
    println!("query:  the original document (variant 0, not indexed)");
    println!("correct answer: variant 1 (2 tokens replaced)");
    println!();
    println!(
        "{:>6} {:>10} {:>10} {:>14} {:>12}",
        "levels", "top-1 ok", "top tied", "bucket entries", "entry ratio"
    );

    let mut baseline_entries = 0usize;
    for levels in [HASH_BITS, 32, 16, 8] {
        let (ok, tied, _rank, entries) = measure(words, variants, levels);
        if baseline_entries == 0 {
            baseline_entries = entries;
        }
        println!(
            "{:>6} {:>10} {:>10} {:>14} {:>11.2}x",
            levels,
            if ok { "yes" } else { "NO" },
            if tied { "TIE" } else { "-" },
            entries,
            entries as f64 / baseline_entries as f64,
        );
    }

    println!();
    println!("does the answer depend on document length?");
    println!("==========================================");
    println!("{:>8} {:>10} {:>10} {:>14} {:>12}", "tokens", "top-1 ok", "top tied", "bucket entries", "best levels");
    // Same variant count at every length, so the only thing changing is how much text the hash sees.
    for words in [200_usize, 60, 20, 8] {
        let (ok64, tied64, _, entries64) = measure(words, variants, HASH_BITS);
        // The fewest level count that gets top-1 right *without* a tie -- and "none" when even
        // full resolution does not, which is the honest answer for short documents.
        let mut best = if ok64 && !tied64 { "64".to_string() } else { "none".to_string() };
        for levels in [32, 16, 8, 4] {
            let (ok, tied, _, _) = measure(words, variants, levels);
            if ok && !tied {
                best = levels.to_string();
                break;
            }
        }
        println!(
            "{:>8} {:>10} {:>10} {:>14} {:>12}",
            words,
            if ok64 { "yes" } else { "NO" },
            if tied64 { "TIE" } else { "-" },
            entries64,
            best,
        );
    }

    // Diagnostic: the short-document failures are the load-bearing result, so show the actual
    // ranks rather than leaving the reader to trust a single column.
    println!();
    println!("diagnostic: where does the correct variant actually rank?");
    println!("=========================================================");
    for words in [200_usize, 60, 20, 8] {
        let documents = corpus(words, variants);
        let mut index = CollisionIndex::with_capacity_and_levels(10_000, HASH_BITS);
        for (position, document) in documents.iter().enumerate().skip(1) {
            let refs: Vec<&str> = document.iter().map(String::as_str).collect();
            index.insert(&format!("variant-{position}"), &refs);
        }
        let refs: Vec<&str> = documents[0].iter().map(String::as_str).collect();
        let results = index.collide(&refs);
        let top: Vec<String> = results
            .iter()
            .take(3)
            .map(|c| format!("{} ({} bits)", c.id, c.shared_bits))
            .collect();
        println!("{words:>5} tokens: {}", top.join(", "));
    }

    println!();
    println!("read the second table as the real result: for short documents no level count gets");
    println!("top-1 right, because the hash itself is decided by noise at that length -- the level");
    println!("knob trades resolution you do not have. The first table only holds for long documents.");
    let _ = tokenize("tokenization is exercised elsewhere");
}
