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
