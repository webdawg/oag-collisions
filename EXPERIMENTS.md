# EXPERIMENTS

A catalogue of experiments, hypotheses, procedures, results, and conclusions. Simulation results
are labelled as such and never presented as production measurements.

Status legend: **RUN** (executed, results recorded) · **PLANNED** (designed, not run) ·
**BLOCKED** (cannot run safely or lacks a dependency).

---

## EXP-001 — Is the Sun travelling at light speed?

**Repository:** `oag-collisions`
**Hypothesis under test.** "The sun is travelling at light speed" (operator's statement, recorded
verbatim in `SPEC.md`). Taken literally: the solar system's barycentre moves at `c` around the
galactic centre.

**Why this is testable at all.** Speed alone produces a constant aberration — the whole sky offset
by a fixed angle, indistinguishable from a redefinition of the coordinate origin. But an
*accelerated* observer sees the sky *slide*, and a circular orbit is continuous acceleration. The
drift rate is `a / c` radians per second, where `a = v² / r` is the centripetal acceleration. That
quantity is measurable, has been measured, and its size reads the orbital speed directly.

**Procedure.** Implemented in `crates/oag-collisions/src/aberration.rs`. Compute
`drift(v, r) = (v² / r) / c` for the two competing speeds at the solar galactic radius
(8.2 kpc), and compare against the published secular aberration drift from VLBI and Gaia
(Titov, Lambert & Gontier 2011; ~5 μas/yr). Reproduce with `cargo run -p oag-collisions --example
aberration`.

**Control.** The same computation at the *measured* solar speed (230 km/s) must land on the
published value, or the formula and constants are wrong and the comparison means nothing.

**Result (RUN, `3bb7127`+).**

| hypothesis | speed | predicted drift | in arcsec/year |
|---|---|---|---|
| measured solar motion | 230 km/s | 4.539 μas/yr | 1.26e-5 |
| operator's statement | 299,792,458 m/s | 7,712,326 μas/yr | 7.712 |
| published measurement | — | ~5 μas/yr | — |

The control passes: 4.539 μas/yr against ~5 μas/yr published.

**Conclusion.** The statement is **not consistent with observation**. The light-speed hypothesis
predicts a drift of **7.7 arcseconds per year** — about 38% of the annual aberration Earth's own
orbit produces, accumulating to roughly **0.17° over an 80-year lifetime**, about a third of the
Moon's apparent diameter. That is visible to the naked eye across a single human life and has
never been reported in any star catalogue. The predicted effect is **1.70 × 10⁶ times** the
measured one, which is exactly `(c / v)²` for v = 230 km/s — so the discrepancy is a factor of
**~1300 in velocity**.

**What would change this conclusion.** A different galactic radius does not help: at `r` small
enough to make `v = c` fit, the orbital period would be absurdly short and the centripetal
acceleration enormous. A non-circular orbit changes the drift's direction, not its order of
magnitude. The measurement itself (~5 μas/yr) is the load-bearing input, and it comes from
independent VLBI and Gaia analyses.

**Scope, stated plainly.** This is a physics consistency check, not new physics. It uses the
standard aberration formula and published measurements. It does not test any claim about
`oag-collisions` itself, and it does not modify the operator's statement in `SPEC.md` — that
statement is recorded verbatim, and this experiment is recorded next to it as the result of taking
it literally.

**Follow-up (PLANNED).** Compute the same drift for the Earth's own orbital motion and compare it
to the classical 20.5-arcsecond annual aberration — a second, independent control that uses a speed
known to three digits.


---

## EXP-004 — How many prefix levels can be dropped before ranking quality breaks?

**Repository:** `oag-collisions`
**Hypothesis.** The index stores every item under all 64 prefixes of its hash, costing `O(items * 64)`
map entries. Fewer levels means proportionally less memory and a coarser ranking, because two items
sharing more than `levels` bits tie. Somewhere between 64 and 8 there should be a level count that
keeps the answer right.

**Procedure.** `crates/oag-collisions/examples/ranking_tradeoff.rs`. A 200-token document, then 59
variants with 2, 4, ... 118 tokens replaced. The query is the original; the correct nearest
neighbour is the 2-token variant. Build an index at 64, 32, 16 and 8 levels and measure top-1
correctness and bucket-entry count. Variant 0 (the identical document) is deliberately not indexed --
an index returning the identical document ahead of the near-duplicate is correct, not confused.

**Result (RUN, `4fcd431`).** First sweep, at a fixed 200 tokens per document:

| prefix levels | top-1 correct | top tied | bucket entries | entry ratio |
|---|---|---|---|---|
| 64 | yes | - | 2570 | 1.00x |
| 32 | yes | - | 995 | **0.39x** |
| 16 | yes | **TIE** | 269 | 0.10x |
| 8 | yes | **TIE** | 42 | 0.02x |

The tie column is the important one and the first version of this experiment did not report it. At
16 and 8 levels the correct variant is still in the top set, but it is **tied** with other variants --
so the "first" answer is arbitrary. The index has not necessarily lost the right answer; it has
lost the ability to choose it. **32 levels is the fewest that gets top-1 right without a tie.**

**Second sweep: the crossover depends on document length, and for short documents there is no
crossover at all.** Same corpus shape, varying only how much text the hash sees:

| tokens | top-1 correct at 64 levels | fewest levels that works |
|---|---|---|
| 200 | yes | 32 |
| 60 | **no** | none |
| 20 | **no** | none |
| 8 | **no** | none |

The diagnostic shows why: at 200 tokens the correct variant wins on **51 shared bits**; at 60
tokens the best hit is an unrelated variant on **11 bits**, which is chance level; at 20 and 8
tokens **nothing crosses the 8-bit collision threshold at all**.

**Conclusion, corrected.** The earlier reading of this experiment was "32 levels is the sweet spot".
That holds only for long documents. The real result is twofold:

1. **For long documents**, 32 levels is the measured sweet spot -- correctness unchanged, memory at
   39%.
2. **For short documents, no level count works.** The unigram SimHash is a majority vote over bit
   positions, and with few tokens each bit's vote is decided by noise, so the hash itself carries no
   signal. The level knob trades away resolution that does not exist.

The second point is the more important one, and it is consistent with this crate's own tests, which
had to use realistic-length texts rather than asserting on 2-4 token inputs.

**Scope, stated plainly.** One synthetic corpus generator: token substitutions on synthetic
documents, with similarity graded by construction. The 200-token figure is where this generator
happens to cross into signal; a corpus with genuinely different structure could move it. What is
*not* generator-specific is the mechanism -- a majority vote over 64 bit positions needs enough
tokens for the vote to be decided by content rather than noise.

**Scope, stated plainly.** This is **one synthetic corpus**: 200-token documents with a small number
of token substitutions, where similarity is graded and known in advance. A corpus whose items
routinely share very long prefixes could shift the crossover, and a corpus with short documents
(which this layer's own tests show is where SimHash is noisy regardless) may not be served by any
level count. The default therefore stays at 64 -- full fidelity, no silent ranking loss -- and
`with_capacity_and_levels` is the knob for a deployment that has measured its own corpus. Changing a
default on one synthetic result would be exactly the kind of unmeasured trade this experiment exists
to prevent.

**Follow-up (PLANNED).** Hash **word bigrams** rather than unigrams. That both multiplies the token
count for short documents (a 20-token text yields 19 bigrams plus 20 unigrams) and captures word
order, which unigram SimHash deliberately discards. If the short-document failure is a token-count
problem, this should move it; if it is a signal problem, nothing will. Measured, not assumed.


---

## EXP-005 — Does bigram shingling fix the short-document failure?

> **CORRECTED (`a5b69bc`+): this experiment's tokenizer comparison was invalid.**
> `insert_text` called the free `tokenize` function and so ignored the index's tokenizer, while
> `collide_text` honoured it. Every cross-tokenizer measurement below therefore compared a
> **unigram-indexed** document against a **bigram-indexed** query -- two differently-hashed
> documents. The "bigrams degrade the working case" result was an artifact of that mismatch, not a
> property of bigrams. See EXP-009 for the defect and EXP-010 for the corrected, and much more
> interesting, result: at vote weight 2 bigrams improve *both* sensitivities.

**Repository:** `oag-collisions`
**Hypothesis.** The short-document failure is a token-count problem: unigram SimHash is a majority
vote over bit positions, and with few tokens the vote is decided by noise. Shingling (unigrams plus
word bigrams) nearly doubles the vote for a short text and captures word order, so it should move
the failure.

**Procedure.** Added `shingles(tokens, max_n)` and a per-index `shingle` setting
(`with_capacity_levels_and_shingle`), then re-ran the length sweep at full resolution with shingle=1
and shingle=2 on the same corpora.

**Result (RUN, `89044f9`+).**

| tokens | unigram top-1 | bigram top-1 | bigram tie |
|---|---|---|---|
| 200 | yes | **NO** | - |
| 60 | no | no | - |
| 20 | no | no | - |
| 8 | no | no | - |

**The hypothesis is falsified, and in the more informative direction: bigram shingling does not fix
the short-document failure and it *degrades* the long-document case.**** At 200 tokens the unigram
index returns variant-1 at **51 shared bits**; the bigram index returns a single result above the
collision threshold, variant-10 at **8 bits**, and the correct answer is not in the result set at
all.

**What this means.** Shingling is not a fix for short documents in this scheme. The likely mechanism
-- recorded as a hypothesis, not a result -- is that bigrams drawn from overlapping windows are
strongly correlated with each other and with the unigrams they contain, so they add vote *weight*
without adding independent information, and the extra weight reshapes the majority in a way that is
not monotone in document distance. That explanation is not verified; what is verified is the
behaviour.

**Consequence for the layer.** The `shingle` knob stays, because the measurement is one synthetic
corpus and a genuinely different corpus could go the other way -- but it ships defaulting to 1
(unigrams) and the negative result is recorded next to it. Nobody should "fix" short-document
performance with bigrams on the strength of an intuition.

**Follow-up (PLANNED).** If short-document similarity is needed, the thing to test next is a
*character* n-gram hash rather than a word n-gram: character shingles are correlated too, but they
do not depend on tokenization at all, which removes a whole class of failure from the measurement.
Measured, not assumed.


---

## EXP-006 — SimHash sensitivity is inversely proportional to the number of units

**Repository:** `oag-collisions`
**Hypothesis.** The short-document failure was explained (EXP-004) as "the majority vote is decided
by noise when there are few tokens". That explanation predicts short documents are *under*-sensitive.
This experiment tests the prediction directly.

**Procedure.** Fix a change (2 tokens replaced), vary the number of units, and count how many bits
of the hash move. Word unigrams, one corpus, only the unit count changing.

**Result (RUN, `362ca03`+).**

| units | bits moved by a 2-token change |
|---|---|
| 10 | 17 |
| 20 | 12 |
| 50 | 4 |
| 100 | 2 |
| 200 | 0 |
| 400 | 2 |
| 800 | 1 |

**Conclusion.** Sensitivity is **inversely proportional to the unit count**, and it collapses faster
than a naive `sqrt(n)` model suggests -- at 200 units a 2-token change moved *zero* bits. The
mechanism is straightforward: SimHash is a majority vote over bit positions, and adding units makes
the vote harder to move. More units means more stability, and past a point it means insensitivity.

**This contradicts the EXP-004 explanation, and the contradiction is the finding.** "Too few tokens
to decide the vote" predicts short documents are under-sensitive; the measurement shows they are
*over*-sensitive -- 10 units move 17 bits, 800 units move 1. So the 60-token failure is **not**
explained by an undecided vote, and the earlier write-up was wrong about the mechanism even though
its measurements were right.

**What is actually established:**
- Word unigrams work at 200 tokens (the correct variant wins on 51 shared bits) and fail at 60, 20
  and 8 (measured, EXP-004).
- Word bigrams and character n-grams do not fix it, and both degrade the working case (measured,
  EXP-005). The character-trigram diagnostic makes the reason visible: with 887 units per document
  the query and a variant sharing 885 of them produced hashes differing in **zero** bits -- the vote
  is frozen.
- The layer has a sensitivity *window*: too few units and the hash is noise; too many and it cannot
  move. Where that window sits depends on how much documents differ.

**Not established.** Why the 60-token case fails. Over-sensitivity alone would predict the near
duplicate still wins (it moves fewer bits than an unrelated document), yet the measurement shows it
losing. Something else is happening at that scale -- plausibly that with ~60 units the *other*
variants' chance similarities are large enough to outrank a correct match separated by only a few
bits. That is a hypothesis, not a result.

**Consequence for the layer.** The unit count is a sensitivity dial, not just a memory dial, and the
two are coupled in the opposite direction from intuition: **more units means less sensitivity**, so
"index more text to be safe" can silently freeze the index. Any deployment should measure the
sensitivity of its own unit count rather than assume more is better.

**Follow-up (PLANNED).** Repeat the sensitivity sweep while varying how much the documents differ,
to map the window: units on one axis, edit distance on the other, bits-moved as the surface. That
would turn "the window exists" into "the window is here".


---

## EXP-007 — The sensitivity window, mapped

**Repository:** `oag-collisions`
**Hypothesis.** EXP-006 found that sensitivity falls with unit count but measured it at one edit
distance. Mapping the whole surface -- units against edit distance -- should turn "a window exists"
into "the window is here".

**Procedure.** `crates/oag-collisions/examples/sensitivity.rs`. Documents of 10, 20, 50, 100, 200,
400 and 800 tokens; edits of 1, 2, 4, 8, 16, 32, 64 tokens (capped at half the document); bits of
the SimHash moved, averaged over 8 deterministic seeds because one draw of replacement tokens is a
sample rather than a measurement.

**Result (RUN, `3d70ffd`+).**

```
 units   e=1   e=2   e=4   e=8  e=16  e=32  e=64
    10  10.5  13.5  18.6  19.5  19.5  19.5  19.5
    20   7.2   9.8  16.5  19.5  20.6  20.6  20.6
    50   3.0   4.9   6.6   9.6  13.5  18.8  18.8
   100   3.6   3.5   5.4   8.5  10.9  17.6  22.4
   200   1.4   1.5   2.8   3.1   6.6  10.8  13.9
   400   1.2   1.5   3.9   4.6   5.0  10.0  12.2
   800   0.5   0.9   1.0   2.2   4.0   5.0   7.8
```

Three features are worth reading off it:

1. **The ~19.5-bit plateau** in the bottom-right is the distance between *unrelated* documents.
   Once half the document is replaced, size stops mattering -- that is the noise floor everything
   is measured against.
2. **Sensitivity falls with unit count** and does not recover: 800 units moves 0.5 bits for a
   one-token edit and 7.8 bits for a 64-token edit.
3. **The ranking gap collapses before detection does.** The gap between e=2 and e=4 is 5.1 bits at
   10 units, 1.8 at 50, 1.3 at 200, and **0.1 bits at 800**. So at 800 units two near-duplicates
   at different distances cannot be *ordered* at all, even though both are still far closer than an
   unrelated document.

**Conclusion.** The window is a **ranking** window, not a detection window. Detection degrades
gracefully -- even a frozen hash still separates near-duplicates from unrelated documents by ~19
bits. Ranking near-duplicates *against each other* is what breaks, and it breaks last: the
e=2 / e=4 gap is the number to watch, not the absolute bits moved.

**The surface also falsifies an explanation.** EXP-004 measured a failure at 60 tokens; here 60
tokens sits comfortably inside the resolvable band (e=2 moves ~4 bits, e=4 ~6). So the 60-token
failure has another cause, and EXP-006 already recorded it as unexplained. This surface narrows it
rather than closing it: whatever fails at 60 tokens is not the majority vote's sensitivity.

**Consequence for the layer.** Choose the unit count by the *ranking* requirement, not the
detection requirement. If only "is this roughly the same document" matters, a large document is
fine. If "which of these near-duplicates is closest" matters, the unit count must be small enough
that consecutive edit distances are separable -- measured at ~200 tokens for e=2 vs e=4, and
collapse by 800.

**Follow-up (PLANNED).** Repeat with a real corpus rather than synthetic token substitution, since
real near-duplicates differ by paraphrase and reorder rather than by token replacement -- the
e=2/e=4 gap is a synthetic proxy for a semantic distance the surface does not model.


---

## EXP-008 — Unigram SimHash is exactly invariant to word order

**Repository:** `oag-collisions`
**Hypothesis.** Real near-duplicates differ by paraphrase and **reordering**, not only by token
substitution (EXP-007's gap). Unigram SimHash sums each token's bits once, so a permutation cannot
change the sum -- predicting the hash is *identical* for a document and its reversal.

**Procedure.** `crates/oag-collisions/examples/ordering.rs`. A 200-token document; three variants --
two tokens replaced, all tokens reversed, two adjacent tokens swapped. Bits moved under word
unigrams and under word bigrams, plus the reversed document's collision strength through the index
itself.

**Result (RUN, `f73b6ed`+).**

| change | word unigrams | word bigrams |
|---|---|---|
| replace 2 tokens | 0 | 1 |
| reverse all tokens | **0** | **24** |
| swap 2 adjacent | 0 | 2 |

Through the index at 64 levels, the **reversed document collides with the original at 64 shared
bits** -- a full-strength, perfect collision.

**Conclusion.** Under unigram tokenization the hash is *exactly* invariant to word order. This is
stronger than "less sensitive": it is an invariance, and no level count, capacity, or threshold
setting can remove it. Two documents sharing every word but asserting something entirely different
are the same document to a unigram index.

Bigrams detect reordering (24 bits for a full reversal, 2 for an adjacent swap), which is the
expected consequence of hashing overlapping windows. EXP-005 already measured that bigrams degrade
the *substitution* case -- so the layer faces a real either/or:

- **unigrams**: insensitive to order, and (below) fragile under substitution at larger sizes;
- **bigrams**: sensitive to order, but EXP-005 measured them degrading the working case.

**This experiment also qualifies EXP-004.** EXP-004 reported that word unigrams "work at 200 tokens",
based on the correct variant winning on 51 shared bits. Here, at the same 200 tokens, a 2-token
replacement moved **0 bits**. Both are true: the sensitivity surface (EXP-007) puts a 2-token edit at
~1.5 bits at 200 units, so whether a given replacement is visible is a coin flip at that size. The
earlier "works at 200 tokens" was a single draw of replacement tokens, not a property of the size.

**Consequence for the layer.** Two separate limits, both structural:
1. **Order blindness** under unigrams -- an invariance.
2. **Sensitivity collapse** at large unit counts -- a tuning limit, measured in EXP-007.

Neither is fixable by adjusting the existing knobs. The honest position for a deployment: unigram
SimHash answers "do these share vocabulary", not "are these the same text", and it stops answering
that reliably once documents grow past the unit count its ranking window allows.

**Follow-up (PLANNED).** A hybrid tokenizer that hashes unigrams *and* a sparse sample of bigrams,
attempting to keep substitution sensitivity while recovering order sensitivity. Whether the
bigrams' vote weight can be made small enough not to freeze the hash is the open question -- and it
would be measured on the EXP-007 surface, not assumed.


---

## EXP-009 — `insert_text` ignored the index's tokenizer

**Repository:** `oag-collisions`
**Defect.** `CollisionIndex::insert_text` called the free `tokenize` function -- word unigrams,
weight 1 -- while `collide_text` called `self.tokenizer.tokenize`. An index configured for word
bigrams or character n-grams therefore **indexed unigrams and queried with the configured scheme**.
Every cross-tokenizer comparison was between two differently-hashed documents.

**How it was found.** Chasing why character trigrams produced *zero* collision signal: the query's
and a variant's trigram hashes were identical (885 of 887 units shared), yet the index reported
nothing. That is impossible for a matching pair, so the index was suspected rather than the hash.

**Fix.** `Tokenizer::tokenize` now returns `(String, i32)` units -- text and vote weight -- and a new
`insert_units` is the single insert path; `insert` wraps raw tokens at weight 1 and `insert_text`
goes through the tokenizer. Added `simhash_weighted`, the standard weighted SimHash, because the
either/or the defect exposed (unigrams blind to order, bigrams freezing substitution) is really a
*vote-weight* question: unigrams at weight 1, bigrams at a fraction.

**Test.** `the_index_honours_its_tokenizer_on_both_sides`. **Verified to catch the defect**: with
`insert_text` temporarily reverted to the free `tokenize`, the test fails on "a bigram index must
also collide with itself at full strength" and passes again once restored.

**Consequence.** Two experiments are invalidated and marked so in place rather than deleted:
- **EXP-005** (bigrams) -- the comparison was mismatched; its word-bigram conclusion does not stand
  as written.
- **EXP-008**'s character-trigram numbers -- same mismatch. EXP-008's *word* unigram/bigram
  comparison called `simhash` directly on both sides and is unaffected; the order-invariance result
  stands.

**What the corrected measurements show.**
- **Character trigrams freeze the hash completely.** With both sides consistently trigram-hashed,
  every variant of a 200-token document hashes *identically* -- all 60 collide at 64 shared bits.
  The earlier "no collisions at all" was the mismatch; the truth is worse and more informative: the
  vote is frozen, so the index cannot rank at all.
- **Word bigrams still do not fix short documents** (60 and 200 tokens both fail), but at 200 tokens
  the top hit is now `variant-2` at 18 shared bits rather than a clean `variant-1` win. Bigrams
  change *which* variant wins, not just whether one does.

**Follow-up (PLANNED).** Now that the tokenizer is honoured on both sides, re-run the EXP-007
sensitivity surface per tokenizer. The surface was measured with word unigrams only; bigram and
character surfaces are unmeasured, and the new `bigram_weight` knob is untested.


---

## EXP-010 — A bigram vote weight of 2 improves both sensitivities

**Repository:** `oag-collisions`
**Hypothesis.** EXP-009's defect invalidated EXP-005's conclusion that bigrams degrade substitution.
With the tokenizer honoured on both sides, a *fraction* of a bigram vote might register word order
without freezing the hash against substitution -- the either/or might be escapable.

**Two measurements** (`examples/sensitivity.rs`), after the EXP-009 fix.

**1. Sensitivity trade-off** (200 tokens, substitution = 2 tokens replaced, order = full reversal,
averaged over 8 seeds):

| bigram weight | reversal bits | substitution bits | units |
|---|---|---|---|
| 0 (unigrams) | 0 | 1.5 | 200 |
| 1 | 24 | **0.9** | 399 |
| 2 | 31 | **4.2** | 399 |
| 4 | 33 | 4.4 | 399 |

**Weight 1 is the worst of both worlds** -- order becomes visible (24 bits) but substitution
sensitivity drops *below* plain unigrams (0.9 vs 1.5). **Weight 2 and 4 improve both**: more order
signal *and* more substitution signal than unigrams. The reason is that a 2-token substitution
changes about three bigrams, not two unigrams, so the bigrams carry more substitution information
than they cost in vote weight.

**2. Does it fix the ranking?** Top-1 correctness for the nearest variant:

| bigram weight | 200 tokens | 60 tokens | 8 tokens |
|---|---|---|---|
| 0 (unigrams) | yes | absent | absent |
| 1 | no (#2) | no (#3) | absent |
| **2** | **yes** | **yes** | absent |
| 4 | absent | absent | absent |
| 8 | absent | absent | absent |

**Weight 2 recovers the 60-token ranking** that unigrams miss and weight 1 loses, while keeping the
200-token case. Weight 4 freezes the hash and loses everything. So the knob has a measured optimum
at 2 on this corpus, not at either extreme.

**Conclusion.** EXP-005's "bigrams degrade the working case" was wrong, and the correction is not
"bigrams are fine" but something sharper: **the default weight-1 setting is the worst available
choice**, and weight 2 dominates it on both axes. The either/or EXP-008 described -- unigrams blind
to order, bigrams freezing substitution -- is real at weight 1 but *not* at weight 2, because the
bigrams' extra substitution signal outweighs their extra vote weight there.

**Scope, stated plainly.** One synthetic corpus (token substitution, 8 seeds) and one ranking task
(top-1 among graded variants). The 8-token case fails at every weight, so very short documents
remain unsolved. The weight-2 optimum is measured, not derived: a corpus whose duplicates differ by
paraphrase rather than substitution could move it.

**Consequence.** `with_capacity_levels_and_shingle` defaults `bigram_weight` to 1 -- the measured
worst setting. That default is now known to be a bad one and should be revisited; the *tokenizer*
default (bigrams off, weight 0) is unaffected and remains the conservative choice.

**Follow-up:** superseded by EXP-011, which ran it after exposing `unigram_weight`.

---

## EXP-011 — Fine sweep: the ranking is not a smooth function of the vote ratio

**Repository:** `oag-collisions`
**Hypothesis.** EXP-010 located a bigram-weight optimum somewhere between 1 and 4. Since the vote
weights are integers, a fractional bigram vote requires scaling the unigrams up -- so
`Tokenizer::Words` gained an explicit `unigram_weight` (default 1, bigrams off), letting the sweep
run at ratios from 0 to 4.

**Result (RUN, `91534a3`+).** 200 tokens; substitution = 2 tokens replaced; ranking = top-1 among
60 graded variants.

| unigram | bigram | ratio | reversal bits | substitution bits | top-1 @200 |
|---|---|---|---|---|---|
| 1 | 0 | 0.00 | 0 | 1.0 | yes |
| 10 | 1 | 0.10 | 2 | 3.0 | NO |
| 5 | 1 | 0.20 | 4 | 3.0 | NO |
| 4 | 1 | 0.25 | 8 | 4.0 | NO |
| 2 | 1 | 0.50 | 15 | 4.0 | NO |
| 1 | 1 | 1.00 | 24 | 1.0 | NO |
| 1 | 2 | 2.00 | 31 | 4.0 | yes |
| 1 | 3 | 3.00 | 31 | 3.0 | NO |
| 1 | 4 | 4.00 | 33 | 5.0 | NO |

**What is smooth and what is not.**
- **Order sensitivity is smooth and monotone**: reversal bits rise steadily from 0 at ratio 0 to 33
  at ratio 4. More bigram vote gives more order signal, as expected.
- **Substitution sensitivity is not monotone**: 1.0, 3.0, 3.0, 4.0, 4.0, **1.0**, 4.0, 3.0, 5.0.
  It dips at ratios 0 and 1.00 and is otherwise flat around 3-5. There is no clean story here, and
  the dip at exactly 1.00 -- where unigrams and bigrams vote equally -- is recorded as an
  observation, not explained.
- **The ranking is not a function of the ratio at all, in any useful sense.** Top-1 is "yes" at
  ratios 0.00 and 2.00 and "no" at 0.10, 0.20, 0.25, 0.50, 1.00, 3.00 and 4.00. Two neighbouring
  settings nine-tenths of the way apart behave differently, and the failures are not clustered at
  either extreme.

**Conclusion: EXP-010's "weight 2 is the measured optimum" overstates the smoothness.** The
sensitivity measurements are real and the 60-token improvement at weight 2 stands, but the
*ranking* outcome flips between adjacent settings, so the top-1 task is a coin flip across the
parameter range rather than a curve with a peak. Picking a vote ratio by measuring top-1 on one
corpus is not a reliable method -- a setting that wins on this corpus can lose 0.1 away.

**What this means for the layer.** The knobs are real and the sensitivities respond to them, but
the top-1 ranking metric is too brittle to tune against. Either the ranking task needs to be
replaced with something less knife-edge (mean rank over several corpora, or a threshold on shared
bits rather than an exact top-1), or the parameters need to be chosen on a corpus the deployment
actually resembles. Both are unmeasured.

**Also delivered:** `Tokenizer::Words` now exposes `unigram_weight`, so fractional bigram votes are
reachable (unigrams 10 / bigrams 1 = a tenth of a vote). Previously the knob was integer-only with
unigrams pinned at 1, so no ratio between 0 and 1 could be expressed at all.


---

## EXP-012 — The margin metric: no setting is reliably better

**Repository:** `oag-collisions`
**Hypothesis.** EXP-011 showed top-1 flips between adjacent settings, so a single corpus cannot
distinguish a good setting from a lucky one. The margin between the correct variant and the next
one, averaged over several corpora, should be a magnitude rather than a coin flip -- and should
separate the settings.

**Procedure.** For each tokenizer setting, build the index and query the original document over 8
corpora (differing only in the replacement tokens drawn). Measure `variant-1 shared bits - best
other shared bits`. Also count corpora where variant-1 never reached the collision threshold at
all -- a different and worse failure than losing a ranking, which the top-1 metric reported as a
plain "no".

**Result (RUN, `057be25`+).** 200 tokens, 60 graded variants, 8 corpora:

| unigram | bigram | ratio | mean margin | std dev | positive | invisible |
|---|---|---|---|---|---|---|
| 1 | 0 | 0.00 | 5.7 | 15.3 | 2/6 | 2 |
| 1 | 1 | 1.00 | 6.0 | 28.0 | 3/8 | 0 |
| 1 | 2 | 2.00 | 1.0 | 0.0 | 1/1 | 7 |
| 1 | 4 | 4.00 | -- | -- | 0/8 | 8 |
| 2 | 1 | 0.50 | 0.0 | 0.0 | 0/2 | 6 |
| 1 | 3 | 3.00 | -- | -- | 0/8 | 8 |

**Conclusion.** **No tokenizer setting is reliably better than any other.** The mean margins are
1-6 bits against standard deviations of 15-28 -- every setting is indistinguishable from zero
within noise, and the number of corpora where the correct variant is *invisible* (below the
collision threshold, not merely outranked) ranges from 0 to 8 out of 8.

This retires the parameter-tuning line rather than resolving it:

- EXP-010's "weight 2 improves both sensitivities" and EXP-011's "the optimum is somewhere between
  1 and 4" were both readings of a metric too knife-edge to support them. The margin metric shows
  the settings are within noise of each other.
- **The dominant failure is not ranking but visibility.** In most corpora the correct variant does
  not reach the collision threshold at all, so there is nothing to rank. That is consistent with
  the EXP-007 sensitivity surface: at 200 units a 2-token edit moves ~1.5 bits on average, which
  is frequently zero -- the correct variant is invisible because the edit is below the hash's
  resolution, not because a competing variant outranked it.

**What this means for the layer.** The unit count -- the document's token length -- is the binding
constraint, not the tokenizer choice. At these sizes the layer cannot reliably rank near-duplicates
at all, and tuning the tokenizer cannot fix that: every setting fails for the same reason.

**Follow-up (PLANNED).** Repeat the margin measurement at document sizes where the sensitivity
surface says a 2-token edit is comfortably visible (50-100 units, ~4-6 bits), and see whether the
tokenizer settings separate there. If they do not separate even in the resolvable band, the
tokenizer choice is genuinely not the lever and the layer's usefulness is bounded by document
length.


---

## EXP-013 — Do the tokenizer settings separate in the resolvable band?

**Repository:** `oag-collisions`
**Hypothesis.** EXP-012 located the binding constraint at the unit count, and the EXP-007 surface
says a 2-token edit is comfortably visible at 50-100 units (~4-6 bits). So the tokenizer settings
should separate there: bigrams add order sensitivity, and at a size where unigrams work they might
win on both axes.

**Procedure.** Repeat the 8-corpus margin measurement at 40, 60, 100 and 200 document tokens, for
five tokenizer settings, reporting the mean margin, its standard deviation, how many corpora gave
a positive margin, how many made the correct variant *invisible*, and the unit count each setting
hashes.

**Result (RUN, `10f78f6`+).**

| tokens | setting | mean margin | std dev | positive | invisible | units |
|---|---|---|---|---|---|---|
| 40 | 1:0 | 12.5 | 3.5 | 2/2 | 6 | 40 |
| 40 | 1:1 | -- | -- | 0/0 | 8 | 79 |
| 60 | 1:0 | 22.0 | 22.0 | 1/2 | 6 | 60 |
| 60 | 1:1 | 3.2 | 6.7 | 2/5 | 3 | 119 |
| 60 | 1:2 | 8.0 | 0.0 | 1/1 | 7 | 119 |
| 100 | 1:0 | 14.8 | 30.8 | 2/4 | 4 | 100 |
| 100 | 1:1 | **-8.5** | 6.0 | 0/6 | 2 | 199 |
| 100 | 1:2 | **-5.2** | 3.8 | 0/6 | 2 | 199 |
| 200 | 1:0 | 5.7 | 15.3 | 2/6 | 2 | 200 |
| 200 | 1:1 | 6.0 | 28.0 | 3/8 | 0 | 399 |

**Conclusion: the tokenizer settings do not separate in the resolvable band -- they anti-separate.**
At 100 tokens, where unigrams give a positive mean margin of 14.8, both bigram settings give
**negative** margins (-8.5 and -5.2) across six corpora each, with small standard deviations.
Bigrams are reliably *worse* exactly where unigrams work.

**The mechanism is visible in the units column.** Bigram settings hash 79 units for a 40-token
document and 199 for 100 -- roughly **double** the unigram count, because every token yields a
unigram *and* a bigram. EXP-007 measured that doubling the unit count collapses sensitivity: 200
units moves ~1.5 bits for a 2-token edit, and 100 units moves ~2. So the bigrams do not merely
fail to help; they push the document into the band where the vote is too large to move.

This closes the parameter-tuning line with a mechanism rather than a shrug:

- **Unigrams at 50-100 units** are the only configuration measured that works at all.
- **Every bigram configuration hashes about twice the units and lands in the frozen band.** The
  tokenizer choice and the unit-count constraint are the *same* constraint.
- The `bigram_weight` knob therefore cannot rescue order sensitivity at practical document sizes:
  to make order visible it must contribute enough votes to matter, and those votes are what freeze
  substitution sensitivity.

**The lever this points at.** If the unit count is the binding constraint, the direction that
matters is **reducing** it, not changing its composition: hash a *sample* of the document's tokens
rather than all of them, keeping the unit count in the 50-100 band regardless of document length.
That is a genuinely different mechanism -- it trades coverage for resolution -- and it is
unmeasured. It would also make the index's behaviour independent of document size, which every
measurement here says is the thing that currently decides success.

**Follow-up (PLANNED).** Implement token sampling (deterministic, e.g. every k-th token for a
k chosen from the document length and a target unit budget) and re-run EXP-007's surface. The
prediction to test: a fixed unit budget should flatten the surface's rows, making sensitivity
independent of document length. If it does, the layer's usefulness stops being bounded by document
size; if it does not, the surface is measuring something else and the unit-count story is wrong.


---

## EXP-014 — A fixed unit budget flattens the sensitivity surface

**Repository:** `oag-collisions`
**Hypothesis.** EXP-013 closed the tokenizer line with a mechanism: bigram settings hash about
double the units, and doubling the unit count is what collapses sensitivity. If the unit count is
the binding constraint, then holding it *fixed* while varying document length should flatten the
EXP-007 surface's rows.

**Procedure.** New `Tokenizer::WordsBudget`: word units, but only a deterministic stride sample of
them, with stride `ceil(len / target)`, so the unit count is bounded no matter how long the
document is. The trade is explicit in the type's docs -- coverage for resolution. Then:
1. Re-run the EXP-007 surface with a budget of 50 units across document sizes 10-800.
2. Sweep the budget at a fixed 800-token document.

**Result 1 -- the surface flattens.** Mean bits moved for e=1..2:

| document tokens | e=1 | e=2 |
|---|---|---|
| 50 (unbudgeted) | 3.0 | 4.9 |
| 100 | 2.4 | 2.4 |
| 200 | 3.0 | 3.0 |
| 400 | 2.6 | 2.6 |
| 800 | 4.9 | 4.9 |

Against the unbudgeted 800-token row (0.5 and 0.9), the budgeted rows are all in the same 2.4-4.9
band. Document length has stopped deciding sensitivity; the budget does.

**Result 2 -- the budget is the knob, and it is monotone.** Fixed 800-token document, 2-token edit:

| budget | 20 | 50 | 100 | 200 | 400 | 800 |
|---|---|---|---|---|---|---|
| bits moved | 5.2 | 4.9 | 2.6 | 1.2 | 1.0 | 0.9 |

Sensitivity tracks the budget alone, declining monotonically as the budget grows, with no reference
to the document's 800 tokens.

**Conclusion.** The unit-count constraint is *bypassable*. A stride-sampled budget decouples the
index's resolution from document length, which is exactly what every previous experiment said was
the binding constraint. The cost is coverage: a budget of 20 on an 800-token document samples 2.5%
of it, so two documents differing only in the unsampled region hash identically. That trade is now
explicit and tunable rather than implicit and accidental.

**Honest limits.** The budgeted sensitivity at a given budget is somewhat *below* the unbudgeted
sensitivity of a document with that many tokens -- 5.2 bits at budget 20 versus 9.8 for a 20-token
document. The stride sample of a long document has gaps, so a small edit can miss the sampled units
entirely. The budget flattens the surface but does not make a long document as sensitive as a
short one of the same unit count.

**Follow-up (PLANNED).** Compare stride sampling against *random* sampling with the same budget,
and against hashing a fixed number of *windows* rather than individual tokens. Stride sampling is
deterministic and spreads coverage, but it is also periodic -- a document whose edits fall between
strides is invisible, and a random sample would not have that blind spot at the cost of clustering.
Which failure mode is more acceptable is unmeasured.


---

## EXP-015 — Stride versus shuffle sampling at the same budget

**Repository:** `oag-collisions`
**Hypothesis.** EXP-014's stride sampling flattens the sensitivity surface but is periodic: with
stride k it sees only positions congruent to 0 mod k, so an edit landing entirely between two
strides is structurally invisible. A deterministic *shuffle* has no blind spot -- every position is
equally likely to be sampled -- at the cost of clustering, so a small edit can be missed by chance
instead of by structure. Which failure mode is better is measurable.

**Procedure.** New `Tokenizer::WordsShuffledBudget`: Fisher-Yates from a deterministic LCG over the
unit indices, then restore document order, so the sample is comparable with the stride variant.
Both schemes given the same budget (200 units of an 800-token document, stride 4). Edits placed
relative to the stride: on a boundary (sampled), halfway between (not sampled), on the next
boundary, halfway after. Each averaged over 8 distinct seeds.

**Result (RUN, `40f588c`+).**

| edit at | stride bits | shuffle bits |
|---|---|---|
| 0-1 (on stride) | 1.0 | 0.9 |
| 2-3 (between) | **0.0** | 0.9 |
| 4-5 (on stride) | 1.0 | 0.6 |
| 6-7 (between) | **0.0** | 1.5 |

**Conclusion.** The stride is a **step function**: it moves the hash for edits that land on a
stride boundary and not at all for edits between them. That is structural, not chance -- no amount
of averaging removes it, because the mean over seeds is the same step function. The shuffle has no
blind spot: its mean is positive and smooth at every position, and its variance is the price.

**A bias in the stride's favour, found while measuring.** The stride always samples index 0, so a
document's opening is over-weighted by construction. That is not coverage; it is a systematic bias
toward the beginning of every document. Part of the stride's apparently better behaviour at
position 0-1 comes from that.

**Which failure mode is more acceptable.** For a crawler comparing a page against its previous
fetch, edits are arbitrary rather than periodic, so the shuffle's random failure is the better one:
it fails sometimes and unpredictably, whereas the stride fails always and predictably for a whole
class of positions. For a task where edits are known to be aligned -- comparing fixed-width records
at fixed offsets -- the stride's blind spot coincides with positions that cannot change anyway, and
it is the better choice. Neither is universally right, and the choice should follow the edit model.

**Honest limits.** One document size (800 tokens), one budget (25% coverage), word unigrams only.
The shuffle's per-seed variance is not reported here -- only its mean -- so "the shuffle is noisier"
is inferred from the spread across positions rather than measured directly.

**Follow-up (PLANNED).** The third option the EXP-014 record named: hash a fixed number of
*windows* rather than individual tokens. A window spans several tokens, so a small edit changes
every window that overlaps it -- that is neither a periodic blind spot nor a clustering coin flip,
and it may dominate both. Unmeasured.


---

## EXP-016 — Window sampling: spread coverage *and* no blind spot

**Repository:** `oag-collisions`
**Hypothesis.** EXP-015 left a third option: hash a fixed budget of *windows* (runs of `window`
consecutive tokens) rather than individual tokens. A 2-token edit touches 2 of 800 token-units; it
touches every window overlapping it -- about `window + 1` of them. Neither a periodic blind spot nor
a clustering coin flip, and the scheme may dominate both.

**Procedure.** New `Tokenizer::WordsWindowBudget`. Same 800-token document, same budget of 200
(stride 4 for tokens; the 798 candidate windows are sampled with the same stride). Edits placed at
a stride boundary (sampled) and halfway between (not sampled).

**Result (RUN, `ca9c2f4`+).**

| edit at | stride (token) | windows (3-wide) |
|---|---|---|
| 0-1 (on stride) | 1.0 | 1.0 |
| 2-3 (between) | **0.0** | **1.0** |
| 4-5 (on stride) | 1.0 | 2.0 |
| 6-7 (between) | **0.0** | **1.0** |

**Conclusion -- corrected by EXP-017.** The window scheme has **no zeros** at the positions
tested, but EXP-017 measured the *rate* over many positions and found the reason is coverage, not
the absence of a blind spot: each window spans 3 tokens, so a sampled window is likely to include
the edited tokens. The shuffle has no blind spot but samples the same *fraction* of tokens, so its
rate is nearly the stride's. The window advantage is real but it comes from unit width. See the
correction below before quoting the "dominates both" claim.

- the **stride's** spread coverage and deterministic, position-independent unit selection;
- the **shuffle's** absence of any periodic blind spot.

The cost is granularity: a unit is a 3-token window, so a change anywhere inside it replaces the
whole window's hash. The index says *which window* changed, not *which token* -- acceptable for
near-duplicate detection, not for localising an edit.

**Honest limits.** One window width (3), one document size (800 tokens), one budget (25%), one
seed for the window scheme. The window scheme's own stride still has a periodic structure -- it
samples candidate windows every 4 -- but because each sampled unit covers 3 tokens, an edit is
caught as long as it overlaps any sampled window, which is 4 tokens wide in coverage rather than 1.
A wider window would widen that further at the cost of coarser units.

**Follow-up (PLANNED).** Measure the false-negative rate of each scheme as a function of edit
position over many positions and seeds, rather than four hand-picked ones. The hand-picked test
shows the blind spot exists and that windows remove it at the sampled points; it does not show the
rate, and the rate is what would justify choosing one scheme in a deployment.


---

## EXP-017 — False-negative rate: coverage matters, not periodicity

**Repository:** `oag-collisions`
**Hypothesis.** EXP-016 showed windows remove the stride's zeros at four hand-picked positions.
That does not establish a *rate*, and the hand-picked positions could be misleading. This measures
the false-negative rate of all three schemes over many positions and seeds.

**Procedure.** 800-token document, budget 200 (stride 4). For every second position in 0..798, and
8 seeds, replace two tokens and count how often the hash moves at all. Report detected / missed /
rate.

**Result (RUN, `a819f12`+).**

| scheme | detected | missed | rate |
|---|---|---|---|
| stride | 1130 | 2062 | 35.4% |
| shuffle | 1296 | 1896 | 40.6% |
| windows | 2762 | 430 | **86.5%** |

**This corrects EXP-015 and EXP-016's framing.** The window scheme wins, but not because it lacks a
periodic blind spot:

- **The shuffle has no blind spot and is barely better than the stride** -- 40.6% vs 35.4%. Both
  sample 200 of 800 units, so a 2-token edit has roughly the same chance of being sampled at all
  (~50% by position, less after the vote absorbs a single-unit change). Removing periodicity
  changes *which* positions fail, not *how many*.
- **The window scheme's advantage is unit width.** Each window spans 3 tokens, so a sampled window
  is likely to include the edited tokens: effective coverage is 600 of 800 token positions rather
  than 200. That is where the 86.5% comes from.
- **The stride's 35.4% is below the naive 50%** for the same reason the shuffle's is: even when an
  edited token *is* sampled, changing one unit out of 200 does not always flip a bit -- the
  majority vote absorbs a single-unit change unless it was within one of a tie.

**Conclusion.** At equal budget, the binding quantity is **coverage** -- how many token positions a
unit spans -- not whether the sampling pattern is periodic. Periodicity decides *which* edits are
missed (systematically, for a whole residue class), while coverage decides *how many*. A deployment
that needs a predictable worst case should prefer windows; one that needs an even distribution of
misses should prefer the shuffle; the stride is dominated on both counts but is the cheapest to
reason about.

**Honest limits.** One document size, one budget, word unigrams, and "detected" means *any* bit
moved, not that the edit was localised or correctly ranked. The rates are for detection only; the
ranking quality behind each scheme is measured separately and was found to be within noise
(EXP-012).

**Follow-up (PLANNED).** Repeat the rate measurement while varying the window width (2, 3, 5) at a
fixed budget. The prediction from the coverage argument: the rate should scale with the fraction of
token positions covered, so window 5 should approach 100% and window 2 should fall toward the
shuffle's. If it does, coverage is confirmed as the mechanism; if not, something else is driving
the window scheme's advantage.

---

## EXP-018 — Window width: coverage is necessary but not sufficient

**Repository:** `oag-collisions`
**Hypothesis.** EXP-017 concluded that coverage is the binding quantity and predicted the detection
rate should track `window * budget / tokens`: width 2 covers 50% of positions, width 3 covers 75%,
and widths 5 and 8 saturate.

**Procedure.** Same rate measurement as EXP-017 -- every second position of an 800-token document,
8 seeds, budget fixed at 200 -- with the window width varied over 2, 3, 5 and 8.

**Result (RUN, `b402cdd`+).**

| window | detected | missed | rate | coverage prediction |
|---|---|---|---|---|
| 2 | 1300 | 1892 | 40.7% | 50% |
| 3 | 2762 | 430 | 86.5% | 75% |
| 5 | 3102 | 90 | 97.2% | 100% |
| 8 | 3045 | 147 | 95.4% | 100% |

**The trend confirms coverage; the absolutes do not.** Width 2 lands at 40.7% against a predicted
50%, width 3 at 86.5% against 75%, and both saturate around 95-97% rather than 100%.

**Why the absolutes miss -- the vote absorbs a changed unit.** A 2-token edit changes 3 windows out
of 799 under width 2, or 4 out of 797 under width 3. With a budget of 200, the expected number of
changed sampled units is under one. A single changed unit out of 200 almost never flips a bit -- the
majority vote absorbs it unless that bit's vote was within one of a tie. So the rate is roughly
`coverage x (1 - absorption)`, and the absorption floor is the ~3.5% residual seen at saturation.

**Conclusion.** Coverage is necessary but not sufficient. The decisive quantity is the **fraction
of sampled units that changed** -- `edit_span / document_length` scaled by how many units the edit
touches -- not the fraction of positions covered. Widening the window raises both coverage and the
number of units an edit touches, which is why the rate rises, but the vote's absorption sets a floor
that coverage alone cannot predict. The usable predictor for a deployment: an edit is detectable when
it changes a substantial fraction of the *sampled* units, not merely when it lands inside a sampled
window.

**Honest limits.** One document size (800 tokens), one budget (25%), word unigrams, and a 2-token
edit. The absorption floor was inferred from the saturation residual rather than measured directly --
a direct test would fix the edit size and vary the budget until the floor moves, which is unmeasured.

**Follow-up (PLANNED).** Measure the absorption floor directly: for a fixed edit, sweep the budget
from 1 to 400 and find where the detection rate stops rising. The prediction is that the floor falls
as the budget grows, because a changed unit becomes a larger fraction of a smaller sample. If it does
not, absorption is not the mechanism and the coverage story is incomplete.

---

## EXP-020 — There is an optimal unit budget; hashing everything is worse

**Repository:** `oag-collisions`
**Hypothesis.** EXP-018 inferred an absorption floor: the majority vote absorbs a changed unit when
that unit is a small fraction of the sample. EXP-019 set out to measure the floor directly by
fixing the edit and sweeping the budget, predicting the floor falls as the budget grows because a
changed unit becomes a larger fraction of a smaller sample.

**Procedure.** 800-token document, 2-token edit, window 3, 8 seeds, edit position swept every
second position. Budget varied over 1, 5, 10, 20, 50, 100, 200, 400, 800. (Budget 800 hashes every
window -- no sampling at all.)

**Result (RUN, `819cc25`+).**

| budget | detected | missed | rate |
|---|---|---|---|
| 1 | 16 | 3176 | 0.5% |
| 5 | 80 | 3112 | 2.5% |
| 10 | 160 | 3032 | 5.0% |
| 20 | 320 | 2872 | 10.0% |
| 50 | 797 | 2395 | 25.0% |
| 100 | 1561 | 1631 | 48.9% |
| **200** | **2762** | **430** | **86.5%** |
| 400 | 1794 | 1398 | 56.2% |
| 800 | 1433 | 1759 | 44.9% |

**The rate peaks at budget 200 and falls on both sides.** This is the opposite of the naive
prediction in two ways:

- Below 200 the rate is **coverage-limited**: too few units are hashed for the edit to be sampled
  often (budget 1 hashes a single window out of 798).
- Above 200 the rate is **absorption-limited**: more units are hashed, but a changed unit becomes
  a smaller fraction of a larger vote, and the vote margin grows with `sqrt(N)`, so a fixed-size
  edit flips *fewer* bits.

**Conclusion: sampling is not merely a memory optimisation -- it improves detection sensitivity.**
Hashing every window (budget 800) is *worse* than sampling 25% of them (86.5% vs 44.9%). The
mechanism is that SimHash's per-bit vote margin scales with the square root of the unit count, so a
fixed-size edit is relatively larger in a smaller sample and flips more bits. There is an optimum
where coverage and sensitivity balance, and here it sits near 25% of the candidate windows.

**Consequence for the layer.** The unit budget is a sensitivity dial as well as a memory dial, and
"index everything" is the wrong default. A deployment should measure its own optimum against its
edit model; the 25% figure is specific to a 2-token edit in an 800-token document with 3-token
windows.

**Honest limits.** One document size, one window width, one edit size, one corpus generator. The
peak's location should move with all four. The mechanism (vote margin ~ `sqrt(N)`) is inferred from
the shape of the curve rather than derived; a direct test would fix the edit size and vary the
document length, predicting the peak moves to keep the changed-unit fraction constant.

**Follow-up (PLANNED).** Repeat the sweep at several document sizes and window widths. The
`sqrt(N)` prediction is that the optimal budget scales with document length, so the optimal
*fraction* stays roughly constant. If the optimal fraction moves instead, the mechanism is not the
vote margin and the absorption story needs revision.

---

## EXP-021 — Does the optimal budget fraction stay constant?

**Repository:** `oag-collisions`
**Hypothesis.** EXP-020's mechanism is that SimHash's vote margin scales with `sqrt(N)`, so the
optimal unit budget should scale with document length -- the optimal *fraction* should stay
roughly constant as the document grows.

**Procedure.** Find the peak detection rate and the budget at which it occurs, for documents of
400, 800 and 1600 tokens (window 3, 2-token edit). Four seeds and an 8-token position step: the
full sweep at 1600 tokens hashes hundreds of millions of units, and the absolute rates are not the
finding -- the location of the peak is.

**Result (RUN, `8ddc2fa`+).**

| tokens | peak rate | peak budget | peak fraction |
|---|---|---|---|
| 400 | 94.5% | 141 | 35.2% |
| 800 | 88.2% | 281 | 35.1% |
| 1600 | 68.0% | 401 | 25.1% |

**The prediction survives one doubling and breaks at the next.** From 400 to 800 tokens the peak
fraction is essentially identical (35.2% vs 35.1%) and the peak budget doubles (141 → 281), which
is exactly what a `sqrt(N)`-driven optimum should do. At 1600 tokens the fraction falls to 25.1%
and the peak rate drops from 88.2% to 68.0%.

**Conclusion.** The optimal fraction is not constant. It holds across one doubling and then falls,
which means absorption dominates *more* at larger sizes, pushing the optimum toward hashing a
larger share of the document -- and even at its optimum, a 1600-token document is detected less
often than a 400-token one (68.0% vs 94.5%). So the unit budget is not a free dial: there is a
best fraction, it drifts with document length, and beyond some length no fraction recovers the
detection rate of a short document.

**Honest limits.** The peak is located by a coarse budget sweep, so its position has a resolution
of one step (5% of the document), and the three sizes are one corpus generator. The 400→800
agreement could be coincidence at that resolution. The 1600 drop is large enough (35% → 25%) and
the peak-rate drop consistent enough (88% → 68%) that the direction is probably real, but the
exact fraction at any size should be measured, not predicted.

**Follow-up (PLANNED).** Repeat at 3200 tokens. If the fraction continues to fall, the optimum is
drifting toward "hash everything" and the absorption story needs a second term; if it returns to
35%, the 1600 point is an artefact of the coarse sweep and the constant-fraction model holds.
