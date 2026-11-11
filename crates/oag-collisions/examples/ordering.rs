//! Does the collision index notice when a document's words are reordered?
//!
//! EXP-007's surface models similarity as *token substitution*, which is a synthetic proxy for how
//! real near-duplicates differ. Real duplicates differ by paraphrase and by **reordering** -- the
//! same words in a different order. That case is worth isolating because unigram SimHash depends
//! only on the token *multiset*: the majority vote sums each token's bits once, so permuting the
//! tokens cannot change the hash at all.
//!
//! If that holds, the layer is structurally blind to word order, and no amount of tuning fixes it.
//! Run with `cargo run -p oag-collisions --example ordering`.

use oag_collisions::{simhash, CollisionIndex, Tokenizer, HASH_BITS};

fn document(units: usize) -> Vec<String> {
    (0..units).map(|i| format!("w{i}")).collect()
}

fn hash_of(tokenizer: Tokenizer, text: &str) -> u64 {
    let units = tokenizer.tokenize(text);
    let refs: Vec<&str> = units.iter().map(String::as_str).collect();
    simhash(&refs)
}

fn main() {
    let units = 200;
    let base = document(units);
    let base_text = base.join(" ");

    // A substitution: replace two tokens. This is the case the surface models.
    let mut substituted = base.clone();
    substituted[0] = "x111111".into();
    substituted[1] = "x222222".into();

    // A reordering: reverse the whole document. The token multiset is identical.
    let mut reversed = base.clone();
    reversed.reverse();

    // A milder reordering: swap two adjacent tokens. Also multiset-identical.
    let mut swapped = base.clone();
    swapped.swap(10, 11);

    println!("does the index notice reordering?");
    println!("=================================");
    println!("document: {units} tokens");
    println!();
    println!(
        "{:>22} {:>18} {:>18}",
        "change", "word unigrams", "word bigrams"
    );
    for (label, variant) in [
        ("replace 2 tokens", &substituted),
        ("reverse all tokens", &reversed),
        ("swap 2 adjacent", &swapped),
    ] {
        let text = variant.join(" ");
        let unigram = (hash_of(Tokenizer::Words { shingle: 1 }, &base_text)
            ^ hash_of(Tokenizer::Words { shingle: 1 }, &text))
        .count_ones();
        let bigram = (hash_of(Tokenizer::Words { shingle: 2 }, &base_text)
            ^ hash_of(Tokenizer::Words { shingle: 2 }, &text))
        .count_ones();
        println!("{label:>22} {unigram:>18} {bigram:>18}");
    }

    // And through the index itself, so the result is not an artifact of calling simhash directly.
    println!();
    println!("through the collision index (word unigrams, {HASH_BITS} levels)");
    println!("==============================================================");
    let mut index = CollisionIndex::with_capacity_and_levels(1_000, HASH_BITS);
    index.insert_text("base", &base_text);
    index.insert_text("reversed", &reversed.join(" "));
    let results = index.collide_text(&base_text);
    let reversed_hit = results.iter().find(|c| c.id == "reversed");
    match reversed_hit {
        Some(hit) => println!(
            "  reversed document collides with the original at {} shared bits",
            hit.shared_bits
        ),
        None => println!(
            "  reversed document does not collide with the original at all (below the threshold)"
        ),
    }

    println!();
    println!("reading this");
    println!("============");
    println!("- A replacement moves bits under both tokenizations, because the token multiset changed.");
    println!("- A reordering moves bits ONLY under bigrams. Under unigrams the hash is *identical*,");
    println!("  because SimHash sums each token once and a permutation does not change the sum.");
    println!("- So a unigram index is structurally blind to word order: two documents that share every");
    println!("  word but say something entirely different are the same document to it.");
    println!("- This is a stronger statement than 'unigrams are less sensitive'. It is an invariance,");
    println!("  and no level count or capacity setting can remove it.");
}
