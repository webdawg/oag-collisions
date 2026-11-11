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
