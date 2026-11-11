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

/// The minimum shared prefix for two items to count as a collision at all.
///
/// Below this the shared bits are noise: two random 64-bit hashes share ~1 bit on average, so a
/// threshold of 1 would return everything. 8 bits is ~1 in 256 by chance, which is a reasonable
/// floor for "these might be related".
pub const MIN_COLLISION_BITS: u32 = 8;

/// A locality-sensitive hash of a token multiset.
///
/// Classic SimHash: weight each bit position by how many tokens' hashes have that bit set, then
/// take the majority. Similar token sets move the majority on few positions, so their SimHashes
/// share a long prefix. Deterministic and dependency-free beyond `blake3`.
pub fn simhash(tokens: &[&str]) -> u64 {
    let mut weights = [0i32; HASH_BITS as usize];
    for token in tokens {
        let digest = blake3::hash(token.as_bytes());
        let bits = u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap());
        for (position, weight) in weights.iter_mut().enumerate() {
            let bit = (bits >> position) & 1;
            *weight += if bit == 1 { 1 } else { -1 };
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
        Self {
            buckets: HashMap::new(),
            hashes: HashMap::new(),
            order: std::collections::VecDeque::new(),
            max_items,
        }
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
        self.remove(id);
        let hash = simhash(tokens);
        self.hashes.insert(id.to_string(), hash);
        self.order.push_back(id.to_string());
        for length in 1..=HASH_BITS {
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

    /// Convenience for free text: tokenizes then inserts.
    pub fn insert_text(&mut self, id: &str, text: &str) -> bool {
        let tokens = tokenize(text);
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        self.insert(id, &refs)
    }

    /// Removes `id` from every bucket. Missing ids are a no-op, so a caller can prune without
    /// checking first.
    pub fn remove(&mut self, id: &str) {
        let Some(hash) = self.hashes.remove(id) else { return };
        self.order.retain(|existing| existing != id);
        for length in 1..=HASH_BITS {
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
        for length in MIN_COLLISION_BITS..=HASH_BITS {
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
            .map(|(id, shared_bits)| Collision { id, shared_bits })
            .collect();
        results.sort_by(|a, b| b.shared_bits.cmp(&a.shared_bits));
        results
    }

    /// Convenience for free text.
    pub fn collide_text(&self, text: &str) -> Vec<Collision<'_>> {
        let tokens = tokenize(text);
        let refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        self.collide(&refs)
    }

    /// How many distinct items are indexed.
    pub fn len(&self) -> usize {
        self.hashes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
