# oag-collisions

**License: GNU Affero General Public License v3.0 or later.**

A layer above the Open Agent Graph that treats a **collision** as an inference result rather than an
error. Two independently produced values that land in the same slot are not a coincidence to be
discarded: the collision *is* the answer to a question, and the strength of that answer is how much
of the slot they share.

## The mechanism

Locality-sensitive hashing. Inputs are hashed so that *similar* inputs are more likely to collide,
and every prefix of every hash is indexed. A query collides partially with everything already
indexed, and the longest shared prefix ranks the result.

The prefix is the *filter*; it decides which items are found. It is a weak *ranking* signal -- it
is the position of the first differing bit, not how many bits differ. `collide_by_hamming` re-ranks
the same candidates by the full-hash Hamming distance, which tracks the edit size far better
(Spearman +0.916 against the edit distance, versus -0.443 for the shared prefix) and eliminates the
22.3% of pairs the prefix ordering gets backwards. The index already stores every item's full
hash, so the re-rank costs nothing asymptotically. See `EXPERIMENTS.md` EXP-024.

The filter is also a *recall* limit: an item whose first differing bit is above bit 8 is
invisible at every level. `collide_multi_probe` widens it by also querying the prefixes that
differ from the query's in a few high-order bits. With one flip it recovers every variant at the
2-, 4- and 8-token edit distances (from 62/160 to 118/160 overall) for 8 extra probes, at 14
near-duplicates per unrelated document admitted. See `EXPERIMENTS.md` EXP-025.

```
"the rust async runtime provides scheduled task execution"
"the rust async runtime provides scheduled task execution, and is fast"
                    ↓ simhash, then index every prefix
        shared prefix length  →  the inference result, strongest first
```

No vector database, no embedding model, no training. One deterministic function and a `HashMap`.

## What it is structurally blind to

Unigram SimHash sums each token's bits once, so **permuting a document does not change its hash at
all**. Measured (`examples/ordering.rs`): a 200-token document and its full reversal collide at
**64 shared bits** -- a perfect, full-strength collision -- and an adjacent swap is likewise
invisible. This is an invariance, not a sensitivity limit: no level count, capacity, or threshold
setting removes it. Two documents that share every word but assert something entirely different are
the same document to a unigram index.

Word bigrams do detect order (24 bits moved by a full reversal), but EXP-005 in `EXPERIMENTS.md`
measured them degrading the substitution case. The layer therefore faces a real either/or:
**unigrams are blind to order; bigrams cost substitution sensitivity.** That trade is measured, not
chosen.

## The bound

Every item is stored under all 64 prefixes of its hash, so memory is `O(items * 64)` map entries --
not `O(items)`. At the default capacity of 100,000 items that is 6.4 million entries, which is
already the difference between "a similarity index" and "most of the RAM on a small box".

`CollisionIndex` is therefore bounded: `with_capacity(n)` caps the item count, and a full index
evicts the oldest item first. `capacity() == 0` means unbounded and is not offered as a default --
an unbounded collision index is a memory-exhaustion vector, and the caller should have to ask for
it. `insert`/`insert_text` return whether an eviction happened.

**Unit budget.** `Tokenizer::WordsBudget` samples a deterministic stride of the document's word
units so the unit count is bounded regardless of length -- the trade is coverage for resolution,
and it is the mechanism every measurement points at. With a budget of 50, document sizes from 100
to 800 tokens all land in the same 2.4-4.9 bit sensitivity band, against 0.5-0.9 for an unbudgeted
800-token document. The budget, not the document length, becomes the tuning knob. See EXP-014 in
`EXPERIMENTS.md`.

**Prefix levels.** Each item is stored under every prefix of its hash, so the level count is also
the per-item memory cost. `with_capacity_and_levels(n, levels)` trades that against ranking
resolution: two items sharing more than `levels` bits tie. Measured on a synthetic corpus
(`cargo run -p oag-collisions --example ranking_tradeoff`): 32 levels keeps top-1 correctness and
drops memory to 40%, while 16 levels saves 88% and starts returning the wrong answer. The default
stays at 64 -- full fidelity -- and the knob is there for a deployment that has measured its own
corpus.

Eviction is oldest-first rather than least-recently-used because the index has no usage statistics:
recency is not relevance, and pretending otherwise would need a hit counter this layer does not
have. Re-indexing an existing id replaces it and never evicts a neighbour.

## The experiment

`SPEC.md` records the operator's statement that the sun is travelling at light speed, and
`EXPERIMENTS.md` records what happens when that is taken literally. The short version: it is
testable, because an accelerated observer sees the sky slide, and the slide rate reads the orbital
speed directly. Three independent measurements -- stellar kinematics (230 km/s), the CMB dipole
(369.8 km/s), and Earth's own orbit as a control (29.78 km/s) -- all land three orders of magnitude
below the claim.

```
cargo run -p oag-collisions --example aberration
```

## Layout

| path | what it is |
|---|---|
| `crates/oag-collisions/src/lib.rs` | SimHash, tokenization, and the bounded `CollisionIndex` |
| `crates/oag-collisions/src/aberration.rs` | the solar-aberration experiment and its three measurements |
| `crates/oag-collisions/examples/aberration.rs` | prints the experiment's numbers |
| `crates/oag-collisions/examples/ranking_tradeoff.rs` | measures prefix levels against ranking quality and index size |
| `SPEC.md` | the operator's statement, recorded verbatim |
| `EXPERIMENTS.md` | hypotheses, procedures, results, and conclusions |
