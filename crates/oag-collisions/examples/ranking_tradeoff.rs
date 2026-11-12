//! Measures the prefix-level trade-off: ranking quality against index size.
//!
//! Every item costs `prefix_levels` bucket entries, not one, so halving the level count halves the
//! memory. What it costs in return is ranking resolution -- two items sharing more than
//! `prefix_levels` bits are tied. This example measures both sides on the same synthetic corpus
//! rather than asserting that the trade is obviously worth making, and then asks whether the answer
//! depends on how long the documents are.
//!
//! Run with `cargo run -p oag-collisions --example ranking_tradeoff`.

use oag_collisions::{simhash_weighted, tokenize, CollisionIndex, Tokenizer, HASH_BITS};

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
fn corpus_with_seed(words: usize, requested_variants: usize, seed: u64) -> Vec<Vec<String>> {
    // Never replace more than half the document: past that the variants stop being graded
    // similarities and become unrelated documents, which measures something else entirely. So the
    // variant count is capped to keep every variant within that bound.
    let variants = requested_variants.min(words / 4 + 1);
    let mut rng = Lcg(seed);
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
fn corpus(words: usize, variants: usize) -> Vec<Vec<String>> {
    corpus_with_seed(words, variants, 0x5EED)
}

/// The margin between the closest variant and the next one, in shared bits.
///
/// This is the metric that replaces exact top-1. EXP-011 showed top-1 flips between adjacent
/// parameter settings, so a single corpus cannot distinguish "this setting ranks correctly" from
/// "this setting got lucky". The margin is a magnitude rather than a position: a positive margin
/// means the correct variant is closest, and measuring it across several corpora shows whether
/// that is consistent or a coin flip.
fn margin(words: usize, variants: usize, levels: u32, tokenizer: Tokenizer, seed: u64) -> Option<i32> {
    let documents = corpus_with_seed(words, variants, seed);
    let query = &documents[0];
    let mut index = CollisionIndex::with_tokenizer(10_000, levels, tokenizer);
    for (position, document) in documents.iter().enumerate().skip(1) {
        index.insert_text(&format!("variant-{position}"), &document.join(" "));
    }
    let results = index.collide_text(&query.join(" "));
    let first = results.iter().find(|c| c.id == "variant-1")?.shared_bits;
    let second = results
        .iter()
        .filter(|c| c.id != "variant-1")
        .map(|c| c.shared_bits)
        .max()
        .unwrap_or(0);
    Some(first as i32 - second as i32)
}

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

/// Builds an index with the given tokenizer and asks the query. Returns the same tuple as
/// [`measure`], for a caller that wants to compare tokenizers rather than level counts.
fn measure_with_tokenizer(
    words: usize,
    variants: usize,
    levels: u32,
    tokenizer: Tokenizer,
) -> (bool, bool, Option<usize>, usize) {
    let documents = corpus(words, variants);
    let mut index = CollisionIndex::with_tokenizer(10_000, levels, tokenizer);
    for (position, document) in documents.iter().enumerate().skip(1) {
        index.insert_text(&format!("variant-{position}"), &document.join(" "));
    }
    let results = index.collide_text(&documents[0].join(" "));
    let correct = format!("variant-1");
    let rank = results.iter().position(|c| c.id == correct).map(|p| p + 1);
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

    // Third sweep: the recorded follow-up. Unigram SimHash is a majority vote over bit
    // positions, and short documents do not give the vote enough tokens to be decided by content.
    // Shingling (unigrams + word bigrams) nearly doubles the vote for a short text and captures
    // word order. Does it move the short-document failure?
    println!();
    println!("does bigram shingling fix the short-document failure?");
    println!("===================================================");
    println!("{:>8} {:>14} {:>14} {:>14}", "tokens", "unigram top-1", "bigram top-1", "bigram tie");
    for words in [200_usize, 60, 20, 8] {
        let unigram = measure_with_tokenizer(words, variants, HASH_BITS, Tokenizer::Words { shingle: 1, unigram_weight: 1, bigram_weight: 0 });
        let bigram = measure_with_tokenizer(words, variants, HASH_BITS, Tokenizer::Words { shingle: 2, unigram_weight: 1, bigram_weight: 1 });
        println!(
            "{:>8} {:>14} {:>14} {:>14}",
            words,
            if unigram.0 { "yes" } else { "NO" },
            if bigram.0 { "yes" } else { "NO" },
            if bigram.1 { "TIE" } else { "-" },
        );
    }

    println!();
    println!("diagnostic: bigram ranking at 200 tokens, top 5");
    println!("==================================================");
    let documents = corpus(200, variants);
    let mut index = CollisionIndex::with_capacity_levels_and_shingle(10_000, HASH_BITS, 2);
    for (position, document) in documents.iter().enumerate().skip(1) {
        index.insert_text(&format!("variant-{position}"), &document.join(" "));
    }
    let results = index.collide_text(&documents[0].join(" "));
    for collision in results.iter().take(5) {
        println!("  {:<14} {} shared bits", collision.id, collision.shared_bits);
    }
    println!("  (correct is variant-1; unigram top was variant-1 at 51 bits)");

    // Fourth sweep: the recorded follow-up. Character n-grams remove tokenization from the
    // measurement entirely -- there is no tokenizer to get wrong, and a changed token still
    // changes every character shingle overlapping it. Does that move the short-document failure?
    println!();
    println!("does character n-gram tokenization fix the short-document failure?");
    println!("==============================================================");
    println!(
        "{:>8} {:>16} {:>16} {:>16}",
        "tokens", "words (unigram)", "chars n=3", "chars n=5"
    );
    for words in [200_usize, 60, 20, 8] {
        let mut row = format!("{words:>8}");
        for tokenizer in [
            Tokenizer::Words { shingle: 1, unigram_weight: 1, bigram_weight: 0 },
            Tokenizer::Characters { n: 3 },
            Tokenizer::Characters { n: 5 },
        ] {
            let (ok, _, _, _) = measure_with_tokenizer(words, variants, HASH_BITS, tokenizer);
            row.push_str(&format!(" {:>16}", if ok { "yes" } else { "NO" }));
        }
        println!("{row}");
    }

    println!();
    println!("diagnostic: character n-gram ranking at 200 tokens, top 5");
    println!("==========================================================");
    let documents = corpus(200, variants);
    let mut index = CollisionIndex::with_tokenizer(
        10_000,
        HASH_BITS,
        Tokenizer::Characters { n: 3 },
    );
    for (position, document) in documents.iter().enumerate().skip(1) {
        index.insert_text(&format!("variant-{position}"), &document.join(" "));
    }
    let results = index.collide_text(&documents[0].join(" "));
    println!("  units per document: {}", Tokenizer::Characters { n: 3 }.tokenize(&documents[0].join(" ")).len());
    println!("  index reports {} collisions", results.len());
    // Compute every variant's hash directly and compare with the query's.
    let q_units = Tokenizer::Characters { n: 3 }.tokenize(&documents[0].join(" "));
    let q_refs: Vec<&str> = q_units.iter().map(|(unit, _)| unit.as_str()).collect();
    let q_hash = oag_collisions::simhash(&q_refs);
    println!("  query hash:  {q_hash:016x}");
    for position in 1..4 {
        let units = Tokenizer::Characters { n: 3 }.tokenize(&documents[position].join(" "));
        let refs: Vec<&str> = units.iter().map(|(unit, _)| unit.as_str()).collect();
        let hash = oag_collisions::simhash(&refs);
        let text = documents[position].join(" ");
        println!(
            "  variant-{position}: first 24 chars {:?}  hash {hash:016x}  differing bits {}",
            &text[..text.len().min(24)],
            (hash ^ q_hash).count_ones()
        );
    }
    // The mechanism: SimHash sensitivity is inversely related to the number of units, because the
    // majority vote gets harder to move as it gets larger. Measure that directly.
    println!();
    println!("  sensitivity vs number of units (a 2-token change, word unigrams):");
    for units in [10_usize, 20, 50, 100, 200, 400, 800] {
        let base: Vec<String> = (0..units).map(|i| format!("w{i}")).collect();
        let mut changed = base.clone();
        changed[0] = "x111111".into();
        changed[1] = "x222222".into();
        let a_refs: Vec<&str> = base.iter().map(String::as_str).collect();
        let b_refs: Vec<&str> = changed.iter().map(String::as_str).collect();
        let differing = (oag_collisions::simhash(&a_refs) ^ oag_collisions::simhash(&b_refs)).count_ones();
        println!("    {units:>4} units -> {differing:>2} bits differ");
    }
    // Direct check, bypassing the index: how many leading bits do the query's and variant-1's
    // trigram hashes actually share? If this is low, the index is not the problem.
    let query_units = Tokenizer::Characters { n: 3 }.tokenize(&documents[0].join(" "));
    let variant_units = Tokenizer::Characters { n: 3 }.tokenize(&documents[1].join(" "));
    let q_refs: Vec<&str> = query_units.iter().map(|(unit, _)| unit.as_str()).collect();
    let v_refs: Vec<&str> = variant_units.iter().map(|(unit, _)| unit.as_str()).collect();
    let q_hash = oag_collisions::simhash(&q_refs);
    let v_hash = oag_collisions::simhash(&v_refs);
    let differing = (q_hash ^ v_hash).count_ones();
    println!("  direct simhash differing bits: {differing} of 64 (query vs variant-1)");
    let same = query_units.iter().filter(|u| variant_units.contains(u)).count();
    println!("  units shared between them: {same} of {}", query_units.len());
    if results.is_empty() {
        println!("  no collisions above the {}-bit threshold at all", oag_collisions::MIN_COLLISION_BITS);
    }
    for collision in results.iter().take(5) {
        println!("  {:<14} {} shared bits", collision.id, collision.shared_bits);
    }

    // EXP-011: top-1 flips between adjacent settings, so it cannot be the tuning metric. The
    // margin between the correct variant and the next one, averaged over several corpora, is a
    // magnitude rather than a coin flip.
    println!();
    println!("margin between the correct variant and the next one, over 8 corpora");
    println!("=============================================================");
    println!(
        "{:>8} {:>8} {:>8} {:>12} {:>12} {:>12}",
        "unigram", "bigram", "ratio", "mean margin", "std dev", "positive"
    );
    for (unigram, bigram) in [
        (1_i32, 0_i32),
        (1, 1),
        (1, 2),
        (1, 4),
        (2, 1),
        (1, 3),
    ] {
        let tokenizer =
            Tokenizer::Words { shingle: 2, unigram_weight: unigram, bigram_weight: bigram };
        let corpora = 8_u64;
        let mut margins: Vec<i32> = Vec::new();
        let mut invisible = 0_usize;
        for seed in 0..corpora {
            match margin(200, variants, HASH_BITS, tokenizer, 0x5EED + seed * 7919) {
                Some(m) => margins.push(m),
                // No margin means variant-1 never reached the collision threshold at all -- it is
                // not outranked, it is invisible. That is a different and worse failure than
                // losing a ranking, and the old top-1 metric reported it as a plain "no".
                None => invisible += 1,
            }
        }
        if margins.is_empty() {
            println!(
                "{:>8} {:>8} {:>8} {:>12} {:>12} {:>12}",
                unigram, bigram, format!("{:.2}", bigram as f64 / unigram as f64),
                "--", "--", format!("0/{} invisible {invisible}", corpora)
            );
            continue;
        }
        let mean = margins.iter().sum::<i32>() as f64 / margins.len() as f64;
        let variance =
            margins.iter().map(|m| (*m as f64 - mean).powi(2)).sum::<f64>() / margins.len() as f64;
        let positive = margins.iter().filter(|m| **m > 0).count();
        println!(
            "{:>8} {:>8} {:>8} {:>12.1} {:>12.1} {:>12}",
            unigram,
            bigram,
            format!("{:.2}", bigram as f64 / unigram as f64),
            mean,
            variance.sqrt(),
            format!("{}/{}", positive, margins.len()),
        );
    }
    // EXP-016: the third sampling scheme. A window spans `window` tokens, so a small edit changes
    // every window overlapping it -- the stride's periodic blind spot should shrink to "at most
    // the fraction of windows that miss the edit".
    println!();
    println!("stride versus windows: coverage of a 2-token edit");
    println!("==================================================");
    println!("document 800 tokens, budget 200 -> stride 4, windows of 3 sampled the same way");
    println!();
    println!(
        "{:>10} {:>16} {:>16}",
        "edit at", "stride (token)", "windows (3-wide)"
    );
    let flat: Vec<String> = (0..800).map(|i| format!("w{i}")).collect();
    // With stride 4 the sampled positions are multiples of 4; the blind spots are the
    // positions between them (2, 6, ...). Testing both kinds is the point.
    for offset in [0_usize, 2, 4, 6] {
        let base = flat.clone();
        let mut changed = base.clone();
        changed[offset] = "x111111".into();
        changed[offset + 1] = "x222222".into();
        let measure = |tokenizer: &dyn Fn(&str) -> Vec<(String, i32)>| -> f64 {
            let base_units = tokenizer(&base.join(" "));
            let base_refs: Vec<(&str, i32)> =
                base_units.iter().map(|(unit, w)| (unit.as_str(), *w)).collect();
            let units = tokenizer(&changed.join(" "));
            let refs: Vec<(&str, i32)> =
                units.iter().map(|(unit, w)| (unit.as_str(), *w)).collect();
            (simhash_weighted(&base_refs) ^ simhash_weighted(&refs)).count_ones() as f64
        };
        let stride_fn = |text: &str| {
            Tokenizer::WordsBudget {
                shingle: 1,
                unigram_weight: 1,
                bigram_weight: 0,
                target_units: 200,
            }
            .tokenize(text)
        };
        let window_fn = |text: &str| {
            Tokenizer::WordsWindowBudget { window: 3, target_units: 200 }.tokenize(text)
        };
        println!(
            "{:>10} {:>16.1} {:>16.1}",
            format!("{}-{}", offset, offset + 1),
            measure(&stride_fn),
            measure(&window_fn)
        );
    }
    println!();
    println!("the stride's zeros are the periodic blind spot. The window scheme should show no zeros,");
    println!("because every window overlapping the edit changes; its cost is coarser units, which is");
    println!("why its per-position values are lower than the stride's peak.");

    // EXP-019: measure the absorption floor directly. Fix the edit and the position sweep,
    // vary only the budget, and find where the detection rate stops rising. The prediction from
    // EXP-018 is that the floor falls as the budget grows, because a changed unit becomes a
    // larger fraction of a smaller sample.
    println!();
    println!("absorption floor: detection rate vs unit budget (window 3, 800 tokens, 8 seeds)");
    println!("==============================================================================");
    let tokens = 800_usize;
    let flat: Vec<String> = (0..tokens).map(|i| format!("w{i}")).collect();
    let window = 3_usize;
    let seeds = 8_u64;
    println!(
        "{:>8} {:>14} {:>12} {:>10}",
        "budget", "detected", "missed", "rate"
    );
    for budget in [1_usize, 5, 10, 20, 50, 100, 200, 400, 800] {
        let mut detected = 0_usize;
        let mut trials = 0_usize;
        for offset in (0..tokens - 2).step_by(2) {
            for seed in 0..seeds {
                let mut changed = flat.clone();
                let mut rng = Lcg(0x5EED + seed * 7919);
                changed[offset] = format!("x{}", rng.next() % 1_000_000);
                changed[offset + 1] = format!("x{}", rng.next() % 1_000_000);
                let base_text = flat.join(" ");
                let base_units = Tokenizer::WordsWindowBudget { window, target_units: budget }
                    .tokenize(&base_text);
                let base_refs: Vec<(&str, i32)> =
                    base_units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
                let units = Tokenizer::WordsWindowBudget { window, target_units: budget }
                    .tokenize(&changed.join(" "));
                let refs: Vec<(&str, i32)> =
                    units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
                if (simhash_weighted(&base_refs) ^ simhash_weighted(&refs)).count_ones() > 0 {
                    detected += 1;
                }
                trials += 1;
            }
        }
        println!(
            "{:>8} {:>14} {:>12} {:>9.1}%",
            budget,
            detected,
            trials - detected,
            100.0 * detected as f64 / trials as f64
        );
    }
    println!();
    println!("read this as: the budget at which the rate stops rising is where the vote stops");
    println!("absorbing a changed unit. Below it, absorption is the binding limit; above it, coverage");
    println!("is. If the rate is flat across the sweep, absorption is not the mechanism.");

    // EXP-021: does the optimal budget scale with document length? The sqrt(N) mechanism
    // predicts the optimal *fraction* stays roughly constant, so the peak should move
    // proportionally with the document. Measure the peak for three sizes.
    println!();
    println!("optimal budget fraction across document sizes (window 3, 2-token edit, 4 seeds, 8-token position step)");
    println!("===============================================================================");
    println!(
        "{:>8} {:>10} {:>12} {:>14}",
        "tokens", "peak rate", "peak budget", "peak fraction"
    );
    // Kept small deliberately: the full sweep at 1600 tokens hashes hundreds of
    // millions of units. Four seeds and an 8-token position step give the same shape
    // for a fraction of the cost, and the absolute rates here are not the finding --
    // the location of the peak is.
    let seeds = 4_u64;
    for tokens in [400_usize, 800, 1600] {
        let flat: Vec<String> = (0..tokens).map(|i| format!("w{i}")).collect();
        let step = (tokens / 20).max(1);
        let budgets: Vec<usize> = (1..=tokens / 2).step_by(step).collect();
        let mut best_rate = 0.0_f64;
        let mut best_budget = 0_usize;
        for budget in &budgets {
            let mut detected = 0_usize;
            let mut trials = 0_usize;
            for offset in (0..tokens - 2).step_by(8) {
                for seed in 0..seeds {
                    let mut changed = flat.clone();
                    let mut rng = Lcg(0x5EED + seed * 7919);
                    changed[offset] = format!("x{}", rng.next() % 1_000_000);
                    changed[offset + 1] = format!("x{}", rng.next() % 1_000_000);
                    let base_units =
                        Tokenizer::WordsWindowBudget { window: 3, target_units: *budget }
                            .tokenize(&flat.join(" "));
                    let base_refs: Vec<(&str, i32)> =
                        base_units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
                    let units = Tokenizer::WordsWindowBudget { window: 3, target_units: *budget }
                        .tokenize(&changed.join(" "));
                    let refs: Vec<(&str, i32)> =
                        units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
                    if (simhash_weighted(&base_refs) ^ simhash_weighted(&refs)).count_ones() > 0 {
                        detected += 1;
                    }
                    trials += 1;
                }
            }
            let rate = detected as f64 / trials as f64;
            if rate > best_rate {
                best_rate = rate;
                best_budget = *budget;
            }
        }
        println!(
            "{:>8} {:>9.1}% {:>12} {:>13.1}%",
            tokens,
            100.0 * best_rate,
            best_budget,
            100.0 * best_budget as f64 / tokens as f64
        );
    }
    println!();
    println!("the sqrt(N) mechanism predicts the peak fraction stays roughly constant as the document");
    println!("grows. If the peak fraction falls, coverage is dominating at large sizes and the optimum");
    println!("is drifting toward hashing everything; if it rises, absorption is.");

    println!();
    println!("read this as: a setting whose mean margin is positive across most corpora is doing the");
    println!("right thing for a reason. One that hovers around zero with a large standard deviation is");
    println!("a coin flip, and its top-1 result on any single corpus means nothing.");

    // EXP-017: the false-negative rate. Everything above samples a handful of positions; the
    // rate over many positions is what would justify choosing a scheme in a deployment.
    println!();
    println!("false-negative rate by edit position (800 tokens, budget 200, 8 seeds per position)");
    println!("===================================================================================");
    let tokens = 800;
    let budget = 200;
    let seeds = 8_u64;
    let flat_doc = |n: usize| -> Vec<String> { (0..n).map(|i| format!("w{i}")).collect() };
    let schemes: [(&str, Tokenizer, Tokenizer, Tokenizer); 1] = [("", Tokenizer::WordsBudget { shingle: 1, unigram_weight: 1, bigram_weight: 0, target_units: budget }, Tokenizer::WordsShuffledBudget { shingle: 1, unigram_weight: 1, bigram_weight: 0, target_units: budget, seed: 0 }, Tokenizer::WordsWindowBudget { window: 3, target_units: budget })];
    let _ = schemes;
    let stride = Tokenizer::WordsBudget { shingle: 1, unigram_weight: 1, bigram_weight: 0, target_units: budget };
    let shuffle = Tokenizer::WordsShuffledBudget { shingle: 1, unigram_weight: 1, bigram_weight: 0, target_units: budget, seed: 0 };
    let window = Tokenizer::WordsWindowBudget { window: 3, target_units: budget };

    // Shuffle needs a seed per (position, seed) pair to be a fair sample, so it is rebuilt
    // inside the loop; the other two are position-independent and built once.
    let base_text = flat_doc(tokens).join(" ");
    let stride_base = stride.tokenize(&base_text);
    let stride_base_refs: Vec<(&str, i32)> =
        stride_base.iter().map(|(u, w)| (u.as_str(), *w)).collect();
    let window_base = window.tokenize(&base_text);
    let window_base_refs: Vec<(&str, i32)> =
        window_base.iter().map(|(u, w)| (u.as_str(), *w)).collect();

    let sample_positions: Vec<usize> = (0..tokens - 2).step_by(2).collect();
    let mut counts = [0_usize; 3];
    let mut trials = 0_usize;
    for offset in &sample_positions {
        for seed in 0..seeds {
            let mut changed = flat_doc(tokens);
            let mut rng = Lcg(0x5EED + seed * 7919);
            changed[*offset] = format!("x{}", rng.next() % 1_000_000);
            changed[*offset + 1] = format!("x{}", rng.next() % 1_000_000);
            let changed_text = changed.join(" ");

            let detected = |base_refs: &Vec<(&str, i32)>, units: &Vec<(String, i32)>| -> bool {
                let refs: Vec<(&str, i32)> =
                    units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
                (simhash_weighted(base_refs) ^ simhash_weighted(&refs)).count_ones() > 0
            };

            let stride_hit = detected(&stride_base_refs, &stride.tokenize(&changed_text));
            let shuffle_tokenizer = Tokenizer::WordsShuffledBudget {
                shingle: 1,
                unigram_weight: 1,
                bigram_weight: 0,
                target_units: budget,
                seed: 0x5EED + seed * 7919,
            };
            let shuffle_base = shuffle_tokenizer.tokenize(&base_text);
            let shuffle_base_refs: Vec<(&str, i32)> =
                shuffle_base.iter().map(|(u, w)| (u.as_str(), *w)).collect();
            let shuffle_hit = detected(&shuffle_base_refs, &shuffle_tokenizer.tokenize(&changed_text));
            let window_hit = detected(&window_base_refs, &window.tokenize(&changed_text));

            counts[0] += usize::from(stride_hit);
            counts[1] += usize::from(shuffle_hit);
            counts[2] += usize::from(window_hit);
            trials += 1;
        }
    }
    println!(
        "{:>12} {:>14} {:>14} {:>14}",
        "scheme", "detected", "missed", "rate"
    );
    for (name, hits) in [("stride", counts[0]), ("shuffle", counts[1]), ("windows", counts[2])] {
        println!(
            "{:>12} {:>14} {:>14} {:>13.1}%",
            name,
            hits,
            trials - hits,
            100.0 * hits as f64 / trials as f64
        );
    }
    println!();
    println!("window width at a fixed budget of {budget}");
    println!("==========================================");
    println!("the coverage argument predicts the rate tracks window*budget/tokens: width 2 covers");
    println!("50% of positions, 3 covers 75%, 5 and 8 saturate. Measure it rather than assume.");
    println!();
    println!("{:>8} {:>14} {:>12} {:>10}", "window", "detected", "missed", "rate");
    for window in [2_usize, 3, 5, 8] {
        let mut detected = 0_usize;
        let mut trials = 0_usize;
        for offset in (0..tokens - 2).step_by(2) {
            for seed in 0..seeds {
                let base_text = flat_doc(tokens).join(" ");
                let mut changed = flat_doc(tokens);
                let mut rng = Lcg(0x5EED + seed * 7919);
                changed[offset] = format!("x{}", rng.next() % 1_000_000);
                changed[offset + 1] = format!("x{}", rng.next() % 1_000_000);
                let changed_text = changed.join(" ");
                let base_units = Tokenizer::WordsWindowBudget {
                    window,
                    target_units: budget,
                }
                .tokenize(&base_text);
                let base_refs: Vec<(&str, i32)> =
                    base_units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
                let units = Tokenizer::WordsWindowBudget {
                    window,
                    target_units: budget,
                }
                .tokenize(&changed_text);
                let refs: Vec<(&str, i32)> =
                    units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
                if (simhash_weighted(&base_refs) ^ simhash_weighted(&refs)).count_ones() > 0 {
                    detected += 1;
                }
                trials += 1;
            }
        }
        println!(
            "{:>8} {:>14} {:>12} {:>9.1}%",
            window,
            detected,
            trials - detected,
            100.0 * detected as f64 / trials as f64
        );
    }
    println!();
    println!("the stride's rate should be near 50% -- it samples every fourth position, so an edit");
    println!("starting on a sampled position is caught and one starting between is not. The shuffle");
    println!("and windows should be near 100%. The gap between them is the variance-versus-granularity");
    println!("trade, now measured rather than asserted.");

    // EXP-012 located the binding constraint at the unit count, and the EXP-007 surface says a
    // 2-token edit is comfortably visible at 50-100 units (~4-6 bits) but marginal at 200. Repeat
    // the margin measurement there: do the tokenizer settings separate in the resolvable band?
    for tokens in [40_usize, 60, 100, 200] {
        println!();
        println!(
            "document size {tokens} tokens -- margin over 8 corpora, and the unit count each setting hashes"
        );
        println!("{:=>68}", "");
        println!(
            "{:>8} {:>8} {:>8} {:>12} {:>12} {:>12} {:>10}",
            "unigram", "bigram", "ratio", "mean margin", "std dev", "positive", "units"
        );
        for (unigram, bigram) in [
            (1_i32, 0_i32),
            (1, 1),
            (1, 2),
            (2, 1),
            (1, 4),
        ] {
            let tokenizer =
                Tokenizer::Words { shingle: 2, unigram_weight: unigram, bigram_weight: bigram };
            let units = tokenizer.tokenize(&(0..tokens).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ")).len();
            let corpora = 8_u64;
            let mut margins = Vec::new();
            let mut invisible = 0_usize;
            for seed in 0..corpora {
                match margin(tokens, 60.min(tokens / 4 + 1), HASH_BITS, tokenizer, 0x5EED + seed * 7919) {
                    Some(m) => margins.push(m),
                    None => invisible += 1,
                }
            }
            if margins.is_empty() {
                println!(
                    "{:>8} {:>8} {:>8} {:>12} {:>12} {:>10} {:>10}",
                    unigram, bigram,
                    format!("{:.2}", bigram as f64 / unigram as f64),
                    "--", "--", format!("0/0 invis {invisible}"), units
                );
                continue;
            }
            let mean = margins.iter().sum::<i32>() as f64 / margins.len() as f64;
            let variance = margins.iter().map(|m| (*m as f64 - mean).powi(2)).sum::<f64>()
                / margins.len() as f64;
            let positive = margins.iter().filter(|m| **m > 0).count();
            println!(
                "{:>8} {:>8} {:>8} {:>12.1} {:>12.1} {:>12} {:>10}",
                unigram,
                bigram,
                format!("{:.2}", bigram as f64 / unigram as f64),
                mean,
                variance.sqrt(),
                format!("{}/{} invis {}", positive, margins.len(), invisible),
                units
            );
        }
    }

    println!();
    println!("read the second table as the real result: for short documents no level count gets");
    println!("top-1 right, because the hash itself is decided by noise at that length -- the level");
    println!("knob trades resolution you do not have. The first table only holds for long documents.");
    let _ = tokenize("tokenization is exercised elsewhere");
}
