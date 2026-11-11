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

```
"the rust async runtime provides scheduled task execution"
"the rust async runtime provides scheduled task execution, and is fast"
                    ↓ simhash, then index every prefix
        shared prefix length  →  the inference result, strongest first
```

No vector database, no embedding model, no training. One deterministic function and a `HashMap`.

## The bound

Every item is stored under all 64 prefixes of its hash, so memory is `O(items * 64)` map entries --
not `O(items)`. At the default capacity of 100,000 items that is 6.4 million entries, which is
already the difference between "a similarity index" and "most of the RAM on a small box".

`CollisionIndex` is therefore bounded: `with_capacity(n)` caps the item count, and a full index
evicts the oldest item first. `capacity() == 0` means unbounded and is not offered as a default --
an unbounded collision index is a memory-exhaustion vector, and the caller should have to ask for
it. `insert`/`insert_text` return whether an eviction happened.

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
| `SPEC.md` | the operator's statement, recorded verbatim |
| `EXPERIMENTS.md` | hypotheses, procedures, results, and conclusions |
