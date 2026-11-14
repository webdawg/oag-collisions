//! `oag-collisions` -- inference by collision.
//!
//! A layer above the Open Agent Graph that treats a **collision** as an inference result rather
//! than an error. Two independently produced values that land in the same slot are not a
//! coincidence to be discarded: the collision *is* the answer to a question. Given a query, the
//! result of a collision is "these things belong together", and the strength of that answer is how
//! much of the slot they share.
//!
//! The mechanism here is **locality-sensitive hashing**: hash inputs so that *similar* inputs are
//! more likely to collide, then index every prefix of every hash. A query collides partially with
//! everything already indexed, and the longest shared prefix ranks the result. No vector
//! database, no embedding model, no training -- one deterministic function and a `HashMap`.
//!
//! The index is bounded, not least because each item costs 64 map entries: see
//! [`DEFAULT_MAX_ITEMS`] for why the item count is not the number to reason about.
//!
//! This is deliberately the smallest honest instance of the idea. It is a new inference mechanism
//! for the project to build on, not a finished product.
//!
//! See `SPEC.md` at the repository root for the operator's own statement of the idea, recorded
//! verbatim.

#![forbid(unsafe_code)]

pub mod aberration;

use std::collections::HashMap;

/// Hash width in bits. 64 keeps the prefix ladder manageable (64 index entries per item) while
/// giving a useful similarity gradient.
pub const HASH_BITS: u32 = 64;

/// Default cap on how many items a `CollisionIndex` holds.
///
/// The bound that matters is not the item count but the bucket count: every item is stored under
/// all 64 prefixes of its hash, so memory is `O(items * 64)` map entries. At the default of
/// 100,000 items that is 6.4 million entries -- already the difference between "a similarity
/// index" and "most of the RAM on a small box". Without a cap, indexing a graph's worth of nodes
/// would grow without limit, which is the same defect the imagination layer's dream log had before
/// it was bounded.
pub const DEFAULT_MAX_ITEMS: usize = 100_000;

/// Expands a token list into unigrams plus contiguous n-grams up to `max_n`.
///
/// Why: unigram SimHash is a majority vote over bit positions, and with few tokens each bit's vote
/// is decided by noise -- measured in `examples/ranking_tradeoff.rs`: a 60-token document produces
/// chance-level rankings and a 20-token document produces nothing above the collision threshold at
/// all. Shingling attacks that directly: a 20-token text yields 20 unigrams plus 19 bigrams, nearly
/// doubling the vote, and it captures word order, which unigram SimHash deliberately discards.
///
/// `max_n` is clamped to at least 1; asking for 1 returns the tokens unchanged.
pub fn shingles(tokens: &[String], max_n: usize) -> Vec<String> {
    let max_n = max_n.max(1);
    if max_n == 1 {
        return tokens.to_vec();
    }
    let mut out = tokens.to_vec();
    for n in 2..=max_n {
        if tokens.len() < n {
            break;
        }
        for window in tokens.windows(n) {
            out.push(window.join(" "));
        }
    }
    out
}

/// How an item's text is turned into the units the hash sees.
///
/// Two schemes, because the short-document failure turned out to depend on tokenization: word
/// n-grams are only as good as the tokenizer, and a synthetic or malformed token stream is a real
/// case this layer has to survive. Character n-grams remove that dependency entirely -- there is
/// no tokenization step to get wrong, and a changed token still changes every character shingle
/// that overlaps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tokenizer {
    /// Lowercased alphanumeric word tokens, optionally expanded with word n-grams (see
    /// [`shingles`]). This is the default.
    ///
    /// `bigram_weight` scales the vote each bigram contributes relative to a unigram's
    /// `unigram_weight`, so `0` is plain unigrams, `1` with `unigram_weight` 1 is unigrams and
    /// bigrams voting equally, and a fraction lets order register without letting it dominate.
    /// See [`simhash_weighted`].
    ///
    /// Both weights are integers, so a *fractional* bigram vote is expressed by scaling the
    /// unigrams up: unigrams 10 and bigrams 1 gives bigrams a tenth of a vote. The default is
    /// unigrams 1, bigrams 0 -- bigrams off entirely.
    Words { shingle: usize, unigram_weight: i32, bigram_weight: i32 },
    /// Overlapping character n-grams of the raw text, case-preserving. `n` is clamped to at least 1.
    Characters { n: usize },
    /// Word units, but only a deterministic *sample* of them, so the unit count is bounded no
    /// matter how long the document is.
    ///
    /// Every measurement in `EXPERIMENTS.md` points the same way: the unit count is the binding
    /// constraint, not the document length or the unit composition. A 40-token document and a
    /// 200-token one hash 40 and 200 units, and EXP-007 showed that doubling the units collapses
    /// sensitivity. This variant holds the budget fixed instead: it takes every k-th unit with
    /// k = ceil(len / target), which spreads the sample across the whole document rather than
    /// truncating it.
    ///
    /// The trade is coverage for resolution. A stride sample of a long document sees a fraction of
    /// it, so two documents that differ only in the unsampled region hash identically. Whether
    /// that trade is worth making is exactly what EXP-014 measures.
    WordsBudget {
        shingle: usize,
        unigram_weight: i32,
        bigram_weight: i32,
        target_units: usize,
    },
    /// As [`Tokenizer::WordsBudget`], but the sample is a deterministic *shuffle* rather than a
    /// stride.
    ///
    /// The stride variant spreads coverage but is periodic: with stride k it sees only positions
    /// congruent to 0 mod k, so an edit landing entirely between two strides is invisible. A
    /// shuffle has no such blind spot -- every position is equally likely to be sampled -- at the
    /// cost of clustering, so a small edit can be missed by chance instead of by structure.
    ///
    /// Which failure mode is more acceptable is measured, not assumed: see EXP-015.
    WordsShuffledBudget {
        shingle: usize,
        unigram_weight: i32,
        bigram_weight: i32,
        target_units: usize,
        seed: u64,
    },
    /// Hash a fixed budget of *windows* -- runs of `window` consecutive tokens -- rather than
    /// individual tokens.
    ///
    /// This is the third sampling scheme, and the one neither stride nor shuffle addresses. A
    /// 2-token edit in an 800-token document touches 2 of 800 units under token sampling. Under
    /// window sampling it touches every window that overlaps it: about `window + 1` of them. With
    /// `window = 3` that is 4 units instead of 2, and more importantly the affected units are
    /// *spread* rather than adjacent, so a stride's periodic blind spot is far less likely to miss
    /// all of them. The cost is that each unit is coarser: two documents differing only inside one
    /// window share that window's replacement, so the hash reflects a blend rather than a position.
    WordsWindowBudget {
        window: usize,
        target_units: usize,
    },
}

impl Default for Tokenizer {
    fn default() -> Self {
        Tokenizer::Words { shingle: 1, unigram_weight: 1, bigram_weight: 0 }
    }
}

impl Tokenizer {
    /// Produces the units to hash for `text`, each with its vote weight.
    ///
    /// A unigram weighs 1. A bigram weighs `bigram_weight`, so `0` switches bigrams off and a
    /// fraction gives them a fraction of a vote -- the knob that lets order register without
    /// letting it dominate. See [`simhash_weighted`].
    pub fn tokenize(&self, text: &str) -> Vec<(String, i32)> {
        match self {
            Tokenizer::Words { shingle, unigram_weight, bigram_weight } => {
                let tokens = tokenize(text);
                let unigram_weight = (*unigram_weight).max(1);
                let mut units: Vec<(String, i32)> =
                    tokens.iter().cloned().map(|t| (t, unigram_weight)).collect();
                if *shingle >= 2 && *bigram_weight != 0 {
                    for n in 2..=*shingle {
                        if tokens.len() < n {
                            break;
                        }
                        for window in tokens.windows(n) {
                            units.push((window.join(" "), *bigram_weight));
                        }
                    }
                }
                units
            }
            Tokenizer::Characters { n } => {
                char_shingles(text, *n).into_iter().map(|unit| (unit, 1)).collect()
            }
            Tokenizer::WordsBudget { shingle, unigram_weight, bigram_weight, target_units } => {
                let mut units = Tokenizer::Words {
                    shingle: *shingle,
                    unigram_weight: *unigram_weight,
                    bigram_weight: *bigram_weight,
                }
                .tokenize(text);
                let target = (*target_units).max(1);
                if units.len() > target {
                    // Stride sample: keeps the sample spread across the document rather than
                    // truncating it, and is fully deterministic.
                    let stride = (units.len() + target - 1) / target;
                    units = units.into_iter().step_by(stride).take(target).collect();
                }
                units
            }
            Tokenizer::WordsWindowBudget { window, target_units } => {
                let tokens = tokenize(text);
                let window = (*window).max(1);
                let mut units: Vec<(String, i32)> = Vec::new();
                if tokens.len() >= window {
                    for slice in tokens.windows(window) {
                        units.push((slice.join(" "), 1));
                    }
                } else {
                    units.push((tokens.join(" "), 1));
                }
                let target = (*target_units).max(1);
                if units.len() > target {
                    let stride = (units.len() + target - 1) / target;
                    units = units.into_iter().step_by(stride).take(target).collect();
                }
                units
            }
            Tokenizer::WordsShuffledBudget {
                shingle,
                unigram_weight,
                bigram_weight,
                target_units,
                seed,
            } => {
                let mut units = Tokenizer::Words {
                    shingle: *shingle,
                    unigram_weight: *unigram_weight,
                    bigram_weight: *bigram_weight,
                }
                .tokenize(text);
                let target = (*target_units).max(1);
                if units.len() > target {
                    // Fisher-Yates from a deterministic LCG, then restore document order so the
                    // sample is comparable with the stride variant.
                    let mut state = *seed;
                    let mut order: Vec<usize> = (0..units.len()).collect();
                    for i in (1..order.len()).rev() {
                        state = state
                            .wrapping_mul(6_364_136_223_846_793_005)
                            .wrapping_add(1_442_695_040_888_963_407);
                        let j = ((state >> 33) as usize) % (i + 1);
                        order.swap(i, j);
                    }
                    order.truncate(target);
                    order.sort_unstable();
                    units = order.into_iter().map(|i| units[i].clone()).collect();
                }
                units
            }
        }
    }

    /// A short name for reporting.
    pub fn describe(&self) -> String {
        match self {
            Tokenizer::Words { shingle, unigram_weight, bigram_weight }
                if *shingle == 1 || *bigram_weight == 0 =>
            {
                format!("word unigrams (bigrams off)")
            }
            Tokenizer::Words { shingle, unigram_weight, bigram_weight } => {
                format!(
                    "word unigrams at weight {unigram_weight} + {shingle}-grams at weight {bigram_weight}"
                )
            }
            Tokenizer::Characters { n } => format!("character {n}-grams"),
            Tokenizer::WordsBudget { target_units, .. } => {
                format!("word units, budget {target_units}")
            }
            Tokenizer::WordsShuffledBudget { target_units, .. } => {
                format!("word units, shuffled budget {target_units}")
            }
            Tokenizer::WordsWindowBudget { window, target_units } => {
                format!("word windows of {window}, budget {target_units}")
            }
        }
    }
}

/// Overlapping character n-grams of `text`, exactly as they appear (case preserved).
///
/// Deliberately not lowercased: a character shingle is a byte-level fingerprint, and folding case
/// would throw away information the scheme is meant to keep. For a comparison that should ignore
/// case, lowercase the text before calling.
pub fn char_shingles(text: &str, n: usize) -> Vec<String> {
    let n = n.max(1);
    let characters: Vec<char> = text.chars().collect();
    if characters.len() < n {
        return characters.iter().map(|c| c.to_string()).collect();
    }
    characters.windows(n).map(|window| window.iter().collect()).collect()
}

/// The minimum shared prefix for two items to count as a collision at all.
///
/// Below this the shared bits are noise: two random 64-bit hashes share ~1 bit on average, so a
/// threshold of 1 would return everything. 8 bits is ~1 in 256 by chance, which is a reasonable
/// floor for "these might be related".
pub const MIN_COLLISION_BITS: u32 = 8;

/// Longest shared prefix between two hashes: the number of leading bits that agree. Identical
/// hashes share all 64; hashes differing first at the most significant bit share none.
fn shared_prefix(a: u64, b: u64) -> u32 {
    let diff = a ^ b;
    if diff == 0 {
        HASH_BITS
    } else {
        diff.leading_zeros()
    }
}

/// A locality-sensitive hash of a token multiset.
///
/// Classic SimHash: weight each bit position by how many tokens' hashes have that bit set, then
/// take the majority. Similar token sets move the majority on few positions, so their SimHashes
/// share a long prefix. Deterministic and dependency-free beyond `blake3`.
pub fn simhash(tokens: &[&str]) -> u64 {
    let weighted: Vec<(&str, i32)> = tokens.iter().map(|token| (*token, 1)).collect();
    simhash_weighted(&weighted)
}

/// SimHash with a per-unit vote weight.
///
/// The unweighted scheme gives every unit one vote, which forces a choice between two failures
/// (measured in `EXPERIMENTS.md`): unigrams are exactly blind to word order, and bigrams -- which
/// double the unit count -- freeze the hash against substitution. Weighting is the standard
/// escape from that either/or: unigrams keep weight 1 and bigrams are given a fraction of a vote,
/// so order contributes to the hash without dominating it.
///
/// A weight of 0 removes a unit from the vote entirely; negative weights are accepted and simply
/// vote the other way, which is useful for down-weighting a known-noise unit.
pub fn simhash_weighted(units: &[(&str, i32)]) -> u64 {
    let mut weights = [0i32; HASH_BITS as usize];
    for (token, weight) in units {
        if *weight == 0 {
            continue;
        }
        let digest = blake3::hash(token.as_bytes());
        let bits = u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap());
        for (position, slot) in weights.iter_mut().enumerate() {
            let bit = (bits >> position) & 1;
            *slot += if bit == 1 { *weight } else { -*weight };
        }
    }
    let mut hash = 0u64;
    for (position, weight) in weights.iter().enumerate() {
        if *weight > 0 {
            hash |= 1 << position;
        }
    }
    hash
}

/// Tokenize free text into the unit the hash sees.
///
/// Deliberately crude: lowercase alphanumeric runs. This is a similarity signal, not a parser --
/// the point is that "rust async runtime" and "Rust Async Runtime!" land close together.
pub fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        if character.is_alphanumeric() {
            // Lowercased so that "Rust" and "rust" are the same token: the signal is the bag of
            // words, and capitalisation at a sentence boundary is not content.
            current.extend(character.to_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// The result of one collision: which item collided, and how strongly.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Collision<'a> {
    pub id: &'a str,
    /// Shared prefix length in bits. Longer means a stronger inference.
    pub shared_bits: u32,
    /// Hamming distance between the query's full hash and the item's full hash. Smaller means a
    /// stronger inference.
    ///
    /// This is the signal a linear scan would use; `shared_bits` is the one the prefix buckets
    /// produce. The two are not the same: a variant one bit away at the most significant position
    /// shares nothing, while a variant one bit away at the least significant position shares 63
    /// bits. `examples/ranking_margin.rs` measures how often the two orderings disagree.
    pub hamming: u32,
}

/// An index where every item is stored under every prefix of its hash.
///
/// That is what makes partial collisions findable: instead of asking "does this exact hash
/// exist", a query asks "what shares my first N bits", for every N. Storage is `O(items * 64)`
/// small integers -- cheap enough that bounding it is not worth the complexity today.
#[derive(Debug)]
pub struct CollisionIndex {
    /// `(prefix_length, prefix)` -> item ids stored under it.
    buckets: HashMap<(u32, u64), Vec<String>>,
    /// Item id -> its full hash, so re-indexing or removal stays possible.
    hashes: HashMap<String, u64>,
    /// Insertion order, oldest first, so a full index can evict deterministically.
    order: std::collections::VecDeque<String>,
    /// How many items the index will hold before evicting the oldest.
    max_items: usize,
    /// How many prefix levels to index, 1..=this. Defaults to [`HASH_BITS`].
    ///
    /// Fewer levels means proportionally less memory -- each item costs `levels` entries, not 64 --
    /// and a coarser ranking, since two items sharing more than `levels` bits are tied. Whether
    /// that trade is worth making is measured in `examples/ranking_tradeoff.rs`, not assumed.
    prefix_levels: u32,
    /// How text is turned into the units the hash sees. One index, one tokenizer, for the same
    /// reason it has one prefix-level count: hashes built from different unit schemes are not
    /// comparable.
    tokenizer: Tokenizer,
}

impl Default for CollisionIndex {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_MAX_ITEMS)
    }
}

impl CollisionIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// An index that holds at most `max_items` items, evicting the oldest when full.
    ///
    /// `0` means unbounded and is deliberately not offered as a default: an unbounded collision
    /// index is a memory-exhaustion vector, and the caller should have to ask for it.
    pub fn with_capacity(max_items: usize) -> Self {
        Self::with_capacity_and_levels(max_items, HASH_BITS)
    }

    /// As [`Self::with_capacity`], with an explicit prefix-level count.
    ///
    /// `prefix_levels` is clamped to `[1, HASH_BITS]`. A level count of 1 indexes only exact-hash
    /// matches; [`HASH_BITS`] resolves every possible shared prefix.
    pub fn with_capacity_and_levels(max_items: usize, prefix_levels: u32) -> Self {
        Self::with_capacity_levels_and_shingle(max_items, prefix_levels, 1)
    }

    /// As [`Self::with_capacity_and_levels`], with an explicit shingle size.
    ///
    /// `shingle = 2` hashes unigrams and word bigrams together, which is the measured fix to try
    /// for short documents -- see [`shingles`].
    pub fn with_capacity_levels_and_shingle(max_items: usize, prefix_levels: u32, shingle: usize) -> Self {
        Self::with_tokenizer(
            max_items,
            prefix_levels,
            Tokenizer::Words { shingle, unigram_weight: 1, bigram_weight: 1 },
        )
    }

    /// As [`Self::with_capacity_and_levels`], with an explicit tokenizer.
    pub fn with_tokenizer(max_items: usize, prefix_levels: u32, tokenizer: Tokenizer) -> Self {
        Self {
            buckets: HashMap::new(),
            hashes: HashMap::new(),
            order: std::collections::VecDeque::new(),
            max_items,
            prefix_levels: prefix_levels.clamp(1, HASH_BITS),
            tokenizer,
        }
    }

    /// The tokenizer this index hashes with.
    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// How many prefix levels this index resolves.
    pub fn prefix_levels(&self) -> u32 {
        self.prefix_levels
    }

    /// How many bucket entries one item costs at this level count -- the number to reason about
    /// when sizing an index, rather than the item count.
    pub fn entries_per_item(&self) -> u32 {
        self.prefix_levels
    }

    /// How many items this index will hold before evicting.
    pub fn capacity(&self) -> usize {
        self.max_items
    }

    /// Indexes `id` under every prefix of the SimHash of `tokens`.
    ///
    /// Re-indexing an existing id replaces it rather than duplicating: a collision index that
    /// counts the same item twice would inflate every result it appears in.
    ///
    /// Returns `true` if an older item was evicted to make room. Eviction is oldest-first, which is
    /// the honest default for an index with no usage statistics: recency is not the same as
    /// relevance, and pretending otherwise would need a hit counter this layer does not have yet.
    pub fn insert(&mut self, id: &str, tokens: &[&str]) -> bool {
        let units: Vec<(String, i32)> = tokens.iter().map(|token| (token.to_string(), 1)).collect();
        self.insert_units(id, &units)
    }

    /// Indexes `id` from units carrying explicit vote weights -- the path a tokenizer with a
    /// non-unit `bigram_weight` takes. See [`simhash_weighted`].
    ///
    /// This is a separate method from [`Self::insert`] rather than a flag on it because the two
    /// have different callers: `insert` is the raw-token escape hatch, `insert_units` is what text
    /// goes through.
    pub fn insert_units(&mut self, id: &str, units: &[(String, i32)]) -> bool {
        self.remove(id);
        let refs: Vec<&str> = units.iter().map(|(unit, _)| unit.as_str()).collect();
        let weights: Vec<i32> = units.iter().map(|(_, weight)| *weight).collect();
        let weighted: Vec<(&str, i32)> = refs.into_iter().zip(weights).collect();
        let hash = simhash_weighted(&weighted);
        self.hashes.insert(id.to_string(), hash);
        self.order.push_back(id.to_string());
        for length in 1..=self.prefix_levels {
            let prefix = hash >> (HASH_BITS - length);
            self.buckets.entry((length, prefix)).or_default().push(id.to_string());
        }
        self.enforce_capacity()
    }

    /// Evicts the oldest items until the index is within its capacity.
    fn enforce_capacity(&mut self) -> bool {
        if self.max_items == 0 {
            return false;
        }
        let mut evicted = false;
        while self.hashes.len() > self.max_items {
            let Some(oldest) = self.order.pop_front() else { break };
            // `remove` takes `&str`; the id is owned here, so borrow it for the call.
            self.remove(&oldest);
            evicted = true;
        }
        evicted
    }

    /// Convenience for free text: applies **this index's tokenizer**, weights and all, then
    /// inserts.
    ///
    /// This used to call the free `tokenize` function and so ignored the tokenizer entirely --
    /// meaning an index configured for bigrams or character n-grams indexed unigrams while
    /// `collide_text` queried with the configured scheme, and every cross-tokenizer comparison was
    /// between two differently-hashed documents. Fixed and covered by
    /// `the_index_honours_its_tokenizer_on_both_sides`.
    pub fn insert_text(&mut self, id: &str, text: &str) -> bool {
        let units = self.tokenizer.tokenize(text);
        self.insert_units(id, &units)
    }

    /// Removes `id` from every bucket. Missing ids are a no-op, so a caller can prune without
    /// checking first.
    pub fn remove(&mut self, id: &str) {
        let Some(hash) = self.hashes.remove(id) else { return };
        self.order.retain(|existing| existing != id);
        for length in 1..=self.prefix_levels {
            let prefix = hash >> (HASH_BITS - length);
            if let Some(bucket) = self.buckets.get_mut(&(length, prefix)) {
                bucket.retain(|existing| existing != id);
                if bucket.is_empty() {
                    self.buckets.remove(&(length, prefix));
                }
            }
        }
    }

    /// The inference: everything that collides with `tokens`, strongest first.
    ///
    /// Returns each colliding id once, with its longest shared prefix. Items sharing fewer than
    /// [`MIN_COLLISION_BITS`] are omitted -- below that the match is indistinguishable from
    /// chance.
    pub fn collide(&self, tokens: &[&str]) -> Vec<Collision<'_>> {
        let hash = simhash(tokens);
        let mut best: HashMap<&str, u32> = HashMap::new();
        // Never look past the levels that were actually indexed: a bucket that does not exist
        // would be silently treated as "no collision", which is the correct answer anyway but
        // hides the configuration from anyone reading the code.
        let deepest = self.prefix_levels.min(HASH_BITS);
        for length in MIN_COLLISION_BITS..=deepest {
            let prefix = hash >> (HASH_BITS - length);
            if let Some(bucket) = self.buckets.get(&(length, prefix)) {
                for id in bucket {
                    let entry = best.entry(id.as_str()).or_insert(0);
                    *entry = (*entry).max(length);
                }
            }
        }
        let mut results: Vec<Collision<'_>> = best
            .into_iter()
            .map(|(id, shared_bits)| Collision {
                id,
                shared_bits,
                hamming: self
                    .hashes
                    .get(id)
                    .map(|item| (hash ^ item).count_ones())
                    .unwrap_or(u32::MAX),
            })
            .collect();
        results.sort_by(|a, b| b.shared_bits.cmp(&a.shared_bits));
        results
    }

    /// Like [`CollisionIndex::collide`], but ranks the candidates by Hamming distance between the
    /// full hashes rather than by the longest shared prefix.
    ///
    /// The prefix buckets find the candidates; the full hashes order them. This is the standard
    /// LSH refinement, and it is cheap here because the candidate set is already in memory and the
    /// index already stores every item's full hash -- it just was not using it for ranking.
    ///
    /// The two signals genuinely differ: a variant one bit away at the most significant position
    /// shares no prefix at all, while one bit away at the least significant position shares 63. So
    /// prefix ranking orders some pairs backwards relative to the edit size -- measured in
    /// `examples/ranking_margin.rs` at 22.3% of comparable pairs. Re-ranking by Hamming removes
    /// those inversions within the candidate set.
    ///
    /// The candidate set itself is still prefix-filtered: an item whose first differing bit is
    /// above [`MIN_COLLISION_BITS`] is never a candidate, however small its Hamming distance. So
    /// this improves the ordering of what the index finds; it does not make the index a distance
    /// oracle over the whole corpus.
    pub fn collide_by_hamming(&self, tokens: &[&str]) -> Vec<Collision<'_>> {
        let mut results = self.collide(tokens);
        results.sort_by(|a, b| a.hamming.cmp(&b.hamming));
        results
    }

    /// Like [`CollisionIndex::collide_text`], but ranks by Hamming distance between the full
    /// hashes. See [`CollisionIndex::collide_by_hamming`] for the difference between the two
    /// signals.
    pub fn collide_by_hamming_text(&self, text: &str) -> Vec<Collision<'_>> {
        let units = self.tokenizer.tokenize(text);
        let refs: Vec<&str> = units.iter().map(|(unit, _)| unit.as_str()).collect();
        self.collide_by_hamming(&refs)
    }

    /// Like [`CollisionIndex::collide_by_hamming`], but also probes prefixes that differ from the
    /// query's in the first `probe_bits` positions, up to `max_flips` flips.
    ///
    /// The prefix filter is what limits recall: an item whose first differing bit is above
    /// [`MIN_COLLISION_BITS`] is invisible at every level, however few bits moved -- measured in
    /// `examples/ranking_margin.rs` as 61% of variants excluded. Probing the
    /// length-[`MIN_COLLISION_BITS`] prefixes that differ from the query's in a few high-order bits
    /// recovers those items. This is the standard multi-probe LSH refinement.
    ///
    /// `probe_bits` is clamped to [`MIN_COLLISION_BITS`] and `max_flips` to `probe_bits`. The extra
    /// probe count is `sum_{k=1..=max_flips} C(probe_bits, k)` on top of the standard ladder, so
    /// `max_flips = 2` with the default `probe_bits = 8` is 36 extra lookups. Every extra probe is
    /// a bucket that can contain unrelated items, so the recall gain is bought with false
    /// positives -- `examples/recall.rs` measures both sides.
    pub fn collide_multi_probe(
        &self,
        tokens: &[&str],
        probe_bits: u32,
        max_flips: u32,
    ) -> Vec<Collision<'_>> {
        let hash = simhash(tokens);
        let deepest = self.prefix_levels.min(HASH_BITS);
        let mut candidates: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for length in MIN_COLLISION_BITS..=deepest {
            let prefix = hash >> (HASH_BITS - length);
            if let Some(bucket) = self.buckets.get(&(length, prefix)) {
                candidates.extend(bucket.iter().map(String::as_str));
            }
        }
        let probe_bits = probe_bits.min(MIN_COLLISION_BITS);
        let max_flips = max_flips.min(probe_bits);
        let base = hash >> (HASH_BITS - MIN_COLLISION_BITS);
        for mask in 1..(1_u64 << probe_bits) {
            if mask.count_ones() > max_flips {
                continue;
            }
            if let Some(bucket) = self.buckets.get(&(MIN_COLLISION_BITS, base ^ mask)) {
                candidates.extend(bucket.iter().map(String::as_str));
            }
        }
        let mut results: Vec<Collision<'_>> = candidates
            .iter()
            .filter_map(|id| {
                self.hashes.get(*id).map(|item| Collision {
                    id,
                    shared_bits: shared_prefix(hash, *item),
                    hamming: (hash ^ item).count_ones(),
                })
            })
            .collect();
        results.sort_by(|a, b| a.hamming.cmp(&b.hamming));
        results
    }
}

impl CollisionIndex {
    /// Text form of [`CollisionIndex::collide_multi_probe`].
    pub fn collide_multi_probe_text(
        &self,
        text: &str,
        probe_bits: u32,
        max_flips: u32,
    ) -> Vec<Collision<'_>> {
        let units = self.tokenizer.tokenize(text);
        let refs: Vec<&str> = units.iter().map(|(unit, _)| unit.as_str()).collect();
        self.collide_multi_probe(&refs, probe_bits, max_flips)
    }

    /// Convenience for free text, expanded with this index's shingle size.
    pub fn collide_text(&self, text: &str) -> Vec<Collision<'_>> {
        let units = self.tokenizer.tokenize(text);
        let refs: Vec<&str> = units.iter().map(|(unit, _)| unit.as_str()).collect();
        self.collide(&refs)
    }

    /// How many distinct items are indexed.
    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    /// How many bucket entries the index currently holds -- the number that actually costs memory,
    /// as opposed to [`Self::len`], which is the number of items.
    ///
    /// Exposed because the item count is the wrong quantity to reason about: at the default 64
    /// levels each item costs 64 entries, so an index reporting 100,000 items is holding 6.4
    /// million entries.
    pub fn bucket_entries(&self) -> usize {
        self.buckets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shuffled budget is the stride budget's counterpart: same unit budget, no periodic
    /// blind spot. Both properties are asserted -- the budget is honoured and the sample is
    /// deterministic for a given seed (and differs between seeds).
    #[test]
    fn the_shuffled_budget_honours_its_target_and_is_seed_deterministic() {
        let text = (0..1000).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        let at = |seed: u64| {
            Tokenizer::WordsShuffledBudget {
                shingle: 1,
                unigram_weight: 1,
                bigram_weight: 0,
                target_units: 50,
                seed,
            }
            .tokenize(&text)
        };
        assert_eq!(at(1).len(), 50, "a 1000-token document must hash exactly 50 units");
        assert_eq!(at(1), at(1), "the same seed must give the same sample");
        assert_ne!(at(1), at(2), "different seeds should give different samples");

        // Under budget the document is passed through unsampled, as with the stride variant.
        let short = Tokenizer::WordsShuffledBudget {
            shingle: 1,
            unigram_weight: 1,
            bigram_weight: 0,
            target_units: 50,
            seed: 1,
        }
        .tokenize("alpha beta gamma");
        assert_eq!(short.len(), 3);
    }

    /// The window scheme is the third sampling design: same budget, overlapping coverage, so a
    /// small edit changes every window that overlaps it. Both properties are asserted.
    #[test]
    fn the_window_budget_honours_its_target_and_is_deterministic() {
        let text = (0..1000).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        let at = |window: usize| {
            Tokenizer::WordsWindowBudget { window, target_units: 50 }.tokenize(&text)
        };
        let units = at(3);
        assert_eq!(units.len(), 50, "a 1000-token document must hash exactly 50 windows");
        assert_eq!(units, at(3), "window sampling must be deterministic");

        // Every unit is a 3-token window, not a single token -- that width is what lets a small
        // edit change a unit without the edit having to land on a sampled position. The sampled
        // units themselves are stride-spaced, so they do not overlap each other.
        assert!(
            units.iter().all(|(unit, w)| unit.split(' ').count() == 3 && *w == 1),
            "every unit must be a 3-token window at weight 1"
        );

        // A document shorter than one window is passed through as a single unit.
        let short = Tokenizer::WordsWindowBudget { window: 3, target_units: 50 }
            .tokenize("alpha beta");
        assert_eq!(short.len(), 1);
    }

    /// The budget tokenizer exists because every measurement points at the unit count as the
    /// binding constraint. It holds that count fixed regardless of document length, so sensitivity
    /// should stop depending on length. Two properties matter and are asserted: the budget is
    /// honoured exactly, and the sample is deterministic (a stride sample, not a random one).
    #[test]
    fn the_budget_tokenizer_honours_its_target_and_is_deterministic() {
        let text = (0..1000).map(|i| format!("w{i}")).collect::<Vec<_>>().join(" ");
        let tokenizer = Tokenizer::WordsBudget {
            shingle: 1,
            unigram_weight: 1,
            bigram_weight: 0,
            target_units: 50,
        };
        let units = tokenizer.tokenize(&text);
        assert_eq!(units.len(), 50, "a 1000-token document must hash exactly 50 units");
        assert_eq!(units, tokenizer.tokenize(&text), "the sample must be deterministic");

        // A document already under budget is passed through unsampled.
        let short = tokenizer.tokenize("alpha beta gamma");
        assert_eq!(short.len(), 3);
    }

    /// `unigram_weight` exists so a *fractional* bigram vote is expressible: the vote weights are
    /// integers, so unigrams 10 with bigrams 1 gives bigrams a tenth of a vote. Before it was
    /// exposed, no ratio between 0 and 1 could be reached at all.
    #[test]
    fn unigram_weight_scales_the_unigram_vote_relative_to_bigrams() {
        let text = "alpha beta gamma delta";
        let units = |unigram: i32, bigram: i32| {
            Tokenizer::Words { shingle: 2, unigram_weight: unigram, bigram_weight: bigram }
                .tokenize(text)
        };
        let plain = units(1, 0);
        assert!(plain.iter().all(|(_, w)| *w == 1), "bigrams off means all weights 1");
        let scaled = units(10, 1);
        let unigram_votes: Vec<i32> = scaled
            .iter()
            .filter(|(unit, _)| !unit.contains(' '))
            .map(|(_, w)| *w)
            .collect();
        let bigram_votes: Vec<i32> = scaled
            .iter()
            .filter(|(unit, _)| unit.contains(' '))
            .map(|(_, w)| *w)
            .collect();
        assert!(unigram_votes.iter().all(|w| *w == 10));
        assert!(bigram_votes.iter().all(|w| *w == 1));
        // The ratio is what matters: a bigram carries a tenth of a unigram's vote.
        assert_eq!(unigram_votes.len(), 4);
        assert_eq!(bigram_votes.len(), 3);
        assert_eq!(plain.len(), 4, "bigrams off means only unigrams");
    }

    /// Regression test for a real defect: `insert_text` used the free `tokenize` function and so
    /// ignored the index's tokenizer, while `collide_text` honoured it. An index configured for
    /// bigrams therefore indexed unigrams and queried with bigrams, and every cross-tokenizer
    /// comparison was between two differently-hashed documents.
    ///
    /// The self-collision assertion is the one that catches it: a document must collide with
    /// itself at full strength under whatever tokenizer the index was configured with.
    #[test]
    fn the_index_honours_its_tokenizer_on_both_sides() {
        let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
        let reordered: String = text.split_whitespace().rev().collect::<Vec<_>>().join(" ");

        let mut unigram = CollisionIndex::with_tokenizer(
            100,
            HASH_BITS,
            Tokenizer::Words { shingle: 1, unigram_weight: 1, bigram_weight: 0 },
        );
        unigram.insert_text("doc", text);
        assert_eq!(
            unigram.collide_text(text).first().map(|c| c.shared_bits),
            Some(HASH_BITS),
            "a document must collide with itself at full strength"
        );
        // Unigrams are order-invariant, so the reordering also collides at full strength.
        assert_eq!(
            unigram.collide_text(&reordered).first().map(|c| c.shared_bits),
            Some(HASH_BITS)
        );

        let mut bigram = CollisionIndex::with_tokenizer(
            100,
            HASH_BITS,
            Tokenizer::Words { shingle: 2, unigram_weight: 1, bigram_weight: 1 },
        );
        bigram.insert_text("doc", text);
        assert_eq!(
            bigram.collide_text(text).first().map(|c| c.shared_bits),
            Some(HASH_BITS),
            "a bigram index must also collide with itself at full strength"
        );
        // But bigrams *detect* the reordering, which is the whole reason to configure them.
        assert!(
            bigram.collide_text(&reordered).first().map(|c| c.shared_bits).unwrap_or(0) < HASH_BITS,
            "bigrams must detect a reordering"
        );
    }

    /// EXP-008: unigram SimHash is *exactly* invariant to token order, because the majority vote
    /// sums each token once and a permutation does not change a sum. This is a structural property
    /// of the scheme, not a tuning artifact -- no level count or capacity removes it -- so it is
    /// asserted as an equality rather than a threshold.
    #[test]
    fn unigram_hash_is_exactly_invariant_to_token_order() {
        let forward: Vec<String> = (0..200).map(|i| format!("w{i}")).collect();
        let mut reversed = forward.clone();
        reversed.reverse();
        let mut swapped = forward.clone();
        swapped.swap(10, 11);

        let f_refs: Vec<&str> = forward.iter().map(String::as_str).collect();
        let r_refs: Vec<&str> = reversed.iter().map(String::as_str).collect();
        let s_refs: Vec<&str> = swapped.iter().map(String::as_str).collect();

        assert_eq!(simhash(&f_refs), simhash(&r_refs), "a full reversal must not change the hash");
        assert_eq!(simhash(&f_refs), simhash(&s_refs), "an adjacent swap must not change the hash");
    }

    /// Character trigrams are what the character tokenizer hashes, so the hash's sensitivity must
    /// be checked on them directly -- the word-token check above does not transfer.
    #[test]
    fn character_trigrams_respond_to_text_changes() {
        let query = "w0 w1 w2 w3 w4 w5 w6 w7 w8 w9";
        let near = "w0 w1 w2 w3 w4 w5 w6 w7 w8 x9";
        let far = "y0 y1 y2 y3 y4 y5 y6 y7 y8 y9";

        let units = |text: &str| {
            let units = char_shingles(text, 3);
            let refs: Vec<&str> = units.iter().map(String::as_str).collect();
            simhash(&refs)
        };

        let (q, n, f) = (units(query), units(near), units(far));
        let near_differs = (q ^ n).count_ones();
        let far_differs = (q ^ f).count_ones();
        println!("near differs in {near_differs} bits, far differs in {far_differs} bits");
        assert!(
            near_differs < far_differs,
            "a one-token change must move fewer bits than a full rewrite: near {near_differs}, far {far_differs}"
        );
    }

    /// A sanity check on the hash itself: two token sets differing in one element must share most
    /// of their hash. If this fails, the majority vote is broken and every collision measurement
    /// above is measuring the breakage.
    #[test]
    fn a_one_token_change_moves_only_a_few_bits() {
        let base: Vec<String> = (0..200).map(|i| format!("w{i}")).collect();
        let refs: Vec<&str> = base.iter().map(String::as_str).collect();
        let mut changed = base.clone();
        changed[0] = "x123456".to_string();
        let changed_refs: Vec<&str> = changed.iter().map(String::as_str).collect();

        let a = simhash(&refs);
        let b = simhash(&changed_refs);
        let shared = u64::BITS - (a ^ b).leading_zeros();
        assert!(
            shared >= 40,
            "changing 1 of 200 tokens should leave most of the hash intact, shared prefix was {shared} bits"
        );
    }

    #[test]
    fn identical_input_collides_fully() {
        let mut index = CollisionIndex::new();
        index.insert_text("a", "rust async runtime");
        // Identical tokens share all 64 bits -- the strongest possible collision.
        let results = index.collide_text("rust async runtime");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a");
        assert_eq!(results[0].shared_bits, HASH_BITS);
    }

    #[test]
    fn near_duplicate_collides_strongly_and_ranks_first() {
        let mut index = CollisionIndex::new();
        // Realistic-length texts: SimHash is a majority vote over bit positions, so with a
        // handful of tokens the vote is decided by noise. These differ by one sentence.
        index.insert_text(
            "near",
            "the rust async runtime provides scheduled task execution across worker threads and \
             is the foundation most oag services are built on",
        );
        index.insert_text(
            "far",
            "a fixed groundwater borehole probe reports water column pressure and temperature at \
             a known location every few seconds",
        );
        let results = index.collide_text(
            "The Rust Async Runtime provides scheduled task execution across worker threads, and \
             is the foundation most services are built on.",
        );
        assert_eq!(results[0].id, "near", "the near duplicate must rank first: {results:?}");
        assert!(results[0].shared_bits >= MIN_COLLISION_BITS);
        assert!(
            results.iter().all(|c| c.id != "far" || c.shared_bits <= results[0].shared_bits),
            "the unrelated item must not outrank the near duplicate"
        );
    }

    /// The Hamming re-rank: the prefix buckets find the candidates, the full hashes order them.
    ///
    /// The two signals are not the same -- a variant one bit away at the most significant position
    /// shares no prefix, while one bit away at the least significant position shares 63. So the
    /// prefix ordering can put a farther document above a nearer one (22.3% of comparable pairs,
    /// measured in `examples/ranking_margin.rs`). `collide_by_hamming` re-ranks the same candidates
    /// by the full-hash distance, which is what a linear scan would give.
    #[test]
    fn collide_by_hamming_orders_the_candidates_by_full_hash_distance() {
        let mut index = CollisionIndex::new();
        // Realistic-length synthetic documents, so the majority vote is decided by content rather
        // than noise. `near` changes one token; `far` changes twenty of the eighty. Both are
        // near-duplicates of the base and so both survive the prefix filter -- an unrelated
        // document would not, and would make the test assert nothing about ordering.
        let base: Vec<String> = (0..80).map(|i| format!("w{i}")).collect();
        let near = {
            let mut v = base.clone();
            v[79] = "x79".to_string();
            v
        };
        let far = {
            let mut v = base.clone();
            for (i, slot) in v.iter_mut().enumerate().skip(60) {
                *slot = format!("x{i}");
            }
            v
        };
        index.insert_text("base", &base.join(" "));
        index.insert_text("near", &near.join(" "));
        index.insert_text("far", &far.join(" "));

        let by_prefix = index.collide_text(&base.join(" "));
        let by_hamming = index.collide_by_hamming_text(&base.join(" "));
        assert!(!by_hamming.is_empty());

        // The re-rank changes the order, not the candidate set: the prefix buckets decide which
        // items are found, and the full hashes only order them.
        let mut prefix_ids: Vec<&str> = by_prefix.iter().map(|c| c.id).collect();
        let mut hamming_ids: Vec<&str> = by_hamming.iter().map(|c| c.id).collect();
        prefix_ids.sort_unstable();
        hamming_ids.sort_unstable();
        assert_eq!(
            prefix_ids, hamming_ids,
            "re-ranking must not change which items are candidates"
        );

        // The Hamming ordering is non-decreasing, and the field is populated from the stored full
        // hashes rather than left at the sentinel.
        for pair in by_hamming.windows(2) {
            assert!(
                pair[0].hamming <= pair[1].hamming,
                "Hamming ranking must be non-decreasing: {} ({}) before {} ({})",
                pair[0].id,
                pair[0].hamming,
                pair[1].id,
                pair[1].hamming
            );
        }
        assert!(by_hamming.iter().all(|c| c.hamming != u32::MAX));

        // The identical document is at distance 0 and ranks first -- the ordering the edit size
        // implies, which the prefix ordering does not guarantee.
        assert_eq!(by_hamming[0].id, "base", "the identical document must rank first");
        assert_eq!(by_hamming[0].hamming, 0, "the identical document is at distance 0");
        // And the near duplicate, when it survives the prefix filter, is closer than the farther
        // one.
        if let (Some(near), Some(far)) = (
            by_hamming.iter().find(|c| c.id == "near"),
            by_hamming.iter().find(|c| c.id == "far"),
        ) {
            assert!(
                near.hamming < far.hamming,
                "near {} must be closer than far {}",
                near.hamming,
                far.hamming
            );
        }
    }

    /// The multi-probe contract: zero flips is exactly the standard ladder, extra flips never
    /// narrow the candidate set, and every scheme is ordered by Hamming distance.
    #[test]
    fn multi_probe_zero_flips_matches_the_ladder_and_extra_flips_never_narrow_it() {
        let mut index = CollisionIndex::new();
        let base: Vec<String> = (0..80).map(|i| format!("w{i}")).collect();
        index.insert_text("base", &base.join(" "));
        for distance in [2_usize, 4, 8, 16] {
            for k in 0..5 {
                let mut changed = base.clone();
                for (slot_index, slot) in changed.iter_mut().enumerate().skip(10 * k).take(distance) {
                    *slot = format!("x{slot_index}");
                }
                index.insert_text(&format!("d{distance}-{k}"), &changed.join(" "));
            }
        }
        let query = base.join(" ");

        let standard = index.collide_text(&query);
        let zero_flip = index.collide_multi_probe_text(&query, MIN_COLLISION_BITS, 0);
        let one_flip = index.collide_multi_probe_text(&query, MIN_COLLISION_BITS, 1);

        let standard_ids: std::collections::HashSet<&str> =
            standard.iter().map(|c| c.id).collect();
        let zero_flip_ids: std::collections::HashSet<&str> =
            zero_flip.iter().map(|c| c.id).collect();
        let one_flip_ids: std::collections::HashSet<&str> =
            one_flip.iter().map(|c| c.id).collect();
        assert_eq!(
            standard_ids, zero_flip_ids,
            "zero flips must be exactly the standard ladder"
        );
        assert!(
            standard_ids.is_subset(&one_flip_ids),
            "extra probes must never narrow the candidate set"
        );
        for list in [&zero_flip, &one_flip] {
            for pair in list.windows(2) {
                assert!(
                    pair[0].hamming <= pair[1].hamming,
                    "every multi-probe scheme must be ordered by Hamming distance"
                );
            }
        }
    }

    #[test]
    fn unrelated_input_collides_with_nothing_above_the_threshold() {
        let mut index = CollisionIndex::new();
        index.insert_text("needle", "supercalifragilisticexpialidocious");
        // A single unrelated item can occasionally share >= 8 bits by chance; with one item the
        // expectation is well below the threshold, and the assertion is that nothing reaches the
        // *strong* end of the scale.
        let results = index.collide_text("completely disjoint vocabulary");
        assert!(
            results.iter().all(|c| c.shared_bits < 32),
            "a chance collision must not look like a strong inference: {results:?}"
        );
    }

    #[test]
    fn re_indexing_replaces_rather_than_duplicates() {
        let mut index = CollisionIndex::new();
        index.insert_text(
            "a",
            "the first version of this document described a routing engine over tensor pads and \
             nothing about collision based inference at all",
        );
        index.insert_text(
            "a",
            "the second version of this document describes a collision index where similar inputs \
             share a long hash prefix and that prefix is the inference",
        );
        assert_eq!(index.len(), 1, "one item, one entry");
        let query = "the second version of this document describes a collision index where \
                     similar inputs share a long hash prefix and that prefix is the inference";
        let results = index.collide_text(query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].shared_bits, HASH_BITS);
        // And the stale index entries are gone: the old text no longer matches at full strength.
        let stale = index.collide_text(
            "the first version of this document described a routing engine over tensor pads and \
             nothing about collision based inference at all",
        );
        assert!(
            stale.is_empty() || stale[0].shared_bits < HASH_BITS,
            "a replaced entry must not still match at full strength: {stale:?}"
        );
    }

    #[test]
    fn removal_clears_every_bucket() {
        let mut index = CollisionIndex::new();
        index.insert_text("a", "rust async runtime");
        index.remove("a");
        assert!(index.is_empty());
        assert!(index.collide_text("rust async runtime").is_empty());
    }

    /// The defect this guards: an unbounded index is a memory-exhaustion vector, and each item
    /// costs 64 bucket entries, so the item count alone is not the number to reason about.
    #[test]
    fn a_full_index_evicts_the_oldest_item() {
        let mut index = CollisionIndex::with_capacity(2);
        assert_eq!(index.capacity(), 2);
        index.insert_text("oldest", "alpha beta gamma delta epsilon zeta eta theta");
        index.insert_text("middle", "iota kappa lambda mu nu xi omicron pi rho");
        assert_eq!(index.len(), 2);

        // Inserting a third evicts the oldest, not the newest.
        let evicted = index.insert_text("newest", "sigma tau upsilon phi chi psi omega sigma");
        assert!(evicted, "inserting past capacity must report the eviction");
        assert_eq!(index.len(), 2);
        assert!(
            index.collide_text("alpha beta gamma delta epsilon zeta eta theta").is_empty(),
            "the evicted item must no longer collide"
        );
        assert_eq!(index.collide_text("sigma tau upsilon phi chi psi omega sigma")[0].id, "newest");
        assert_eq!(index.collide_text("iota kappa lambda mu nu xi omicron pi rho")[0].id, "middle");
    }

    /// Re-indexing an item at capacity must not evict a *different* item: replacing an existing
    /// entry is not an insertion.
    #[test]
    fn re_indexing_at_capacity_does_not_evict_anything() {
        let mut index = CollisionIndex::with_capacity(2);
        index.insert_text("a", "alpha beta gamma delta epsilon zeta eta theta iota");
        index.insert_text("b", "kappa lambda mu nu xi omicron pi rho sigma tau");
        assert!(!index.insert_text("a", "upsilon phi chi psi omega sigma tau upsilon phi"));
        assert_eq!(index.len(), 2, "replacing an entry must not evict its neighbour");
        assert_eq!(index.collide_text("kappa lambda mu nu xi omicron pi rho sigma tau")[0].id, "b");
    }

    /// Eviction must clear every one of the item's 64 bucket entries, not just the hash record --
    /// otherwise the index would leak memory one stale entry at a time and still return ghosts.
    #[test]
    fn eviction_clears_every_bucket_entry() {
        let mut index = CollisionIndex::with_capacity(1);
        index.insert_text("a", "alpha beta gamma delta epsilon zeta eta theta iota kappa");
        index.insert_text("b", "lambda mu nu xi omicron pi rho sigma tau upsilon phi");
        assert_eq!(index.len(), 1);
        assert_eq!(index.collide_text("lambda mu nu xi omicron pi rho sigma tau upsilon phi")[0].id, "b");
        // Nothing from "a" survives at any prefix length.
        assert!(index.collide_text("alpha beta gamma delta epsilon zeta eta theta iota kappa").is_empty());
    }

    /// Fewer prefix levels means proportionally fewer bucket entries -- the memory half of the
    /// trade-off. 16 levels costs a quarter of 64.
    #[test]
    fn prefix_levels_control_the_memory_cost() {
        let full = CollisionIndex::with_capacity_and_levels(1_000, HASH_BITS);
        let mut quarter = CollisionIndex::with_capacity_and_levels(1_000, 16);
        assert_eq!(full.entries_per_item(), 64);
        assert_eq!(quarter.entries_per_item(), 16);
        assert_eq!(quarter.prefix_levels(), 16);

        quarter.insert_text("a", "alpha beta gamma delta epsilon zeta eta theta iota kappa");
        // One item, 16 entries -- not 64.
        assert!(quarter.buckets.len() <= 16, "got {} entries", quarter.buckets.len());
        assert!(full.buckets.len() <= 64);
    }

    /// The ranking is still correct at a coarse level count: identical input still collides at
    /// the deepest level available, and the near duplicate still outranks the unrelated item.
    #[test]
    fn a_coarse_index_still_ranks_correctly() {
        let mut index = CollisionIndex::with_capacity_and_levels(1_000, 16);
        index.insert_text("near", "the rust async runtime provides scheduled task execution across worker threads");
        index.insert_text("far", "a groundwater borehole probe reports water column pressure and temperature");

        let results = index.collide_text(
            "the rust async runtime provides scheduled task execution across worker threads, and is fast",
        );
        assert_eq!(results[0].id, "near", "a coarse index must still rank the near duplicate first");
        // With 16 levels the strongest possible shared prefix is 16 bits, not 64.
        assert!(results[0].shared_bits <= 16);
    }

    #[test]
    fn empty_index_returns_nothing_not_everything() {
        let index = CollisionIndex::new();
        assert!(index.collide_text("anything at all").is_empty());
    }

    #[test]
    fn hashing_is_deterministic_and_order_insensitive() {
        // A set-similarity hash should not care about token order: "a b" and "b a" describe the
        // same bag of words.
        assert_eq!(simhash(&["alpha", "beta"]), simhash(&["beta", "alpha"]));
        assert_eq!(simhash(&["alpha"]), simhash(&["alpha"]));
        assert_ne!(simhash(&["alpha"]), simhash(&["omega"]));
    }

    #[test]
    fn tokenization_ignores_case_and_punctuation() {
        assert_eq!(tokenize("Hello, World!"), vec!["hello", "world"]);
        assert_eq!(tokenize("  spaced   out  "), vec!["spaced", "out"]);
        assert!(tokenize("!!!").is_empty());
    }
}
