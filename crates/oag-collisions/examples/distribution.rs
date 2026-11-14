//! EXP-023: the detection outcome as a distribution, not a peak.
//!
//! ```text
//! cargo run -p oag-collisions --example distribution
//! ```
//!
//! EXP-022 withdrew the "optimal budget fraction" claim: the rate-over-budget curve is jagged
//! enough that the peak is a property of the measurement grid, not of the document. The follow-up
//! it left open is recorded verbatim: "the honest measurement is not a peak over a budget grid but
//! the **distribution** of detection outcomes over many (budget, position, seed) triples at a fixed
//! size, reported as a histogram rather than a single rate."
//!
//! This is that measurement. Two questions, both about what a deployment should expect:
//!
//! 1. **Is the jaggedness a few outlier budgets or the normal case?** A fine budget sweep with a
//!    Wilson interval per point shows whether the rate moves smoothly or in steps.
//! 2. **What does a detection actually look like?** A per-budget histogram of *bits moved* --
//!    not just detected/missed -- shows whether the detections are a few large margins or many
//!    small ones, and how close the misses came.
//!
//! The document is 800 synthetic tokens, the edit is 2 tokens, the tokenizer is window sampling
//! with width 3 (the scheme EXP-016/EXP-017 settled on). The source is an LCG, so a run is exactly
//! reproducible. The base tokenization is computed once per budget, not once per trial, so the
//! whole run finishes in seconds.

use oag_collisions::{simhash_weighted, Tokenizer};

/// A deterministic generator, so the surface is identical on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

fn document(tokens: usize) -> Vec<String> {
    (0..tokens).map(|i| format!("w{i}")).collect()
}

/// Wilson score interval for a binomial proportion, in percentage points. Used instead of a normal
/// approximation because the rate is often near 0 or 1, where the normal interval is undefined.
fn wilson(hits: usize, n: usize, z: f64) -> (f64, f64) {
    if n == 0 {
        return (0.0, 0.0);
    }
    let n = n as f64;
    let p = hits as f64 / n;
    let z2 = z * z;
    let denom = 1.0 + z2 / n;
    let centre = (p + z2 / (2.0 * n)) / denom;
    let variance = p * (1.0 - p) / n;
    let correction = z2 / (4.0 * n * n);
    let half = (z / denom) * (variance + correction).sqrt();
    (100.0 * (centre - half), 100.0 * (centre + half))
}

/// The bits moved by one 2-token edit at `offset`, given the base document's already-tokenized
/// units. Tokenizing the base once per budget rather than once per trial is the difference between
/// a minute-long run and a seconds-long one.
fn bits_moved_from(base_refs: &[(&str, i32)], flat: &[String], offset: usize, budget: usize, seed: u64) -> u32 {
    let tokenizer = Tokenizer::WordsWindowBudget { window: 3, target_units: budget };
    let mut changed = flat.to_vec();
    let mut rng = Lcg(seed);
    changed[offset] = format!("x{}", rng.next() % 1_000_000);
    changed[offset + 1] = format!("x{}", rng.next() % 1_000_000);
    let units = tokenizer.tokenize(&changed.join(" "));
    let refs: Vec<(&str, i32)> = units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
    (simhash_weighted(base_refs) ^ simhash_weighted(&refs)).count_ones()
}

/// One budget's whole distribution: how many trials moved 0 bits, 1-2, 3-5, 6-10, or 11+.
#[derive(Debug, Default, Clone)]
struct Histogram {
    zero: usize,
    one_two: usize,
    three_five: usize,
    six_ten: usize,
    eleven_plus: usize,
}

impl Histogram {
    fn add(&mut self, bits: u32) {
        match bits {
            0 => self.zero += 1,
            1 | 2 => self.one_two += 1,
            3..=5 => self.three_five += 1,
            6..=10 => self.six_ten += 1,
            _ => self.eleven_plus += 1,
        }
    }

    fn total(&self) -> usize {
        self.zero + self.one_two + self.three_five + self.six_ten + self.eleven_plus
    }

    fn detected(&self) -> usize {
        self.total() - self.zero
    }

    fn row(&self, label: &str) -> String {
        let total = self.total().max(1) as f64;
        format!(
            "{:>8} {:>6.1}% {:>8.1}% {:>9.1}% {:>9.1}% {:>10.1}%",
            label,
            100.0 * self.zero as f64 / total,
            100.0 * self.one_two as f64 / total,
            100.0 * self.three_five as f64 / total,
            100.0 * self.six_ten as f64 / total,
            100.0 * self.eleven_plus as f64 / total,
        )
    }
}

fn main() {
    let tokens = 800_usize;
    let flat = document(tokens);
    // A fine sweep: every 10 units from 10 to 200. EXP-022's coarse grid is what made the peak
    // unresolvable, so the point here is a grid fine enough to see the shape.
    let budgets: Vec<usize> = (10..=200).step_by(10).collect();
    let seeds = 5_u64;
    // The position step must be coprime with the window stride, or the sample lands only on
    // sampled-window starts. The stride is `ceil(798 / budget)` -- 16 at budget 50, 8 at 100,
    // 4 at 200 -- so a step of 7 spreads the sample across every residue of every stride. A step
    // of 16 (the first version of this example) measured 99.8% at budget 50 purely because every
    // tested position was a sampled window's start; that was a sampling artifact, not a result.
    let position_step = 7_usize;
    let positions: Vec<usize> = (0..tokens - 2).step_by(position_step).collect();

    println!("EXP-023: detection as a distribution, not a peak");
    println!("================================================");
    println!(
        "document {tokens} tokens, 2-token edit, window 3, {} positions x {seeds} seeds = {} trials per budget",
        positions.len(),
        positions.len() * seeds as usize
    );
    println!();
    println!("panel 1: the rate over a fine budget grid, with its Wilson 95% interval");
    println!("========================================================================");
    println!(
        "{:>8} {:>10} {:>10} {:>9} {:>18}",
        "budget", "detected", "missed", "rate", "95% CI"
    );

    let mut rates = Vec::new();
    let mut histograms: Vec<(usize, Histogram)> = Vec::new();
    for budget in &budgets {
        let tokenizer = Tokenizer::WordsWindowBudget { window: 3, target_units: *budget };
        let base_units = tokenizer.tokenize(&flat.join(" "));
        let base_refs: Vec<(&str, i32)> = base_units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
        let mut histogram = Histogram::default();
        for &offset in &positions {
            for seed in 0..seeds {
                histogram.add(bits_moved_from(&base_refs, &flat, offset, *budget, 0x5EED + seed * 7919));
            }
        }
        let detected = histogram.detected();
        let total = histogram.total();
        let (lo, hi) = wilson(detected, total, 1.96);
        let rate = 100.0 * detected as f64 / total as f64;
        rates.push(((*budget, rate), (lo, hi)));
        histograms.push((*budget, histogram));
        println!(
            "{:>8} {:>10} {:>10} {:>8.1}% {:>8.1}%-{:>5.1}%",
            budget,
            detected,
            total - detected,
            rate,
            lo,
            hi
        );
    }

    // Is the jaggedness a few outlier budgets or the normal case? Measure the local variation:
    // how much does the rate change between adjacent budgets, and how often is that change larger
    // than the interval's half-width? If adjacent points differ by more than their uncertainty,
    // the curve is genuinely jagged rather than a smooth rate with noise.
    println!();
    println!("is the curve jagged or smooth?");
    println!("=============================");
    let mut jumps = Vec::new();
    for window in rates.windows(2) {
        let ((b0, r0), (lo0, hi0)) = window[0];
        let ((b1, r1), (lo1, hi1)) = window[1];
        let half0 = (hi0 - lo0) / 2.0;
        let half1 = (hi1 - lo1) / 2.0;
        let gap = (r1 - r0).abs();
        // A jump is "real" if it exceeds the sum of the two half-widths -- the intervals do not
        // overlap, so the two rates are distinguishable.
        let distinguishable = gap > half0 + half1;
        jumps.push((b0, b1, gap, distinguishable));
    }
    let real_jumps = jumps.iter().filter(|(_, _, _, d)| *d).count();
    println!(
        "adjacent budget pairs: {}, of which {} differ by more than their combined uncertainty",
        jumps.len(),
        real_jumps
    );
    for (b0, b1, gap, d) in &jumps {
        println!(
            "  {b0:>4} -> {b1:>4}: {:>5.1} points {}",
            gap,
            if *d { "(distinguishable)" } else { "(within noise)" }
        );
    }

    // Panel 2: what a detection looks like. The rate alone does not say whether the detections are
    // a few large margins or many small ones, and a deployment cares about the margin: a 1-bit
    // detection is one flipped bit away from a miss. Reuses panel 1's histograms -- recomputing
    // them would triple the runtime for a table that is already measured.
    println!();
    println!("panel 2: the margin distribution at three budgets");
    println!("=================================================");
    println!(
        "{:>8} {:>10} {:>10} {:>11} {:>11} {:>12}",
        "budget", "0 bits", "1-2 bits", "3-5 bits", "6-10 bits", "11+ bits"
    );
    for budget in [50_usize, 100, 200] {
        let (_, histogram) = histograms
            .iter()
            .find(|(b, _)| *b == budget)
            .expect("panel 1 measured every budget in the grid");
        println!("{}", histogram.row(&format!("{budget}")));
    }
    println!();
    println!("read the 0-bits column as the miss rate and the rest as the margin a detection carries.");
    println!("A deployment that needs a *confident* detection should read the 6-10 and 11+ columns:");
    println!("those are the edits the hash moved far enough that a second edit would not undo them.");

    // Panel 3: the same distribution with the budget axis collapsed -- is the miss rate uniform
    // across the document, or concentrated? A window sample covers the whole document, so a
    // concentrated miss rate would mean the scheme has a positional blind spot after all.
    println!();
    println!("panel 3: where the misses fall");
    println!("=============================");
    println!("each cell is the miss rate over {seeds} seeds at that position, budget 100");
    let budget = 100_usize;
    let tokenizer = Tokenizer::WordsWindowBudget { window: 3, target_units: budget };
    let base_units = tokenizer.tokenize(&flat.join(" "));
    let base_refs: Vec<(&str, i32)> = base_units.iter().map(|(u, w)| (u.as_str(), *w)).collect();
    let mut misses = Vec::new();
    for &offset in &positions {
        let mut missed = 0;
        for seed in 0..seeds {
            if bits_moved_from(&base_refs, &flat, offset, budget, 0x5EED + seed * 7919) == 0 {
                missed += 1;
            }
        }
        misses.push(missed as f64 / seeds as f64);
    }
    let mean_miss = misses.iter().sum::<f64>() / misses.len() as f64;
    let max_miss = misses.iter().copied().fold(0.0_f64, f64::max);
    let min_miss = misses.iter().copied().fold(1.0_f64, f64::min);
    println!(
        "mean miss rate across positions: {:.1}% (min {:.1}%, max {:.1}%)",
        100.0 * mean_miss,
        100.0 * min_miss,
        100.0 * max_miss
    );
    let variance = misses.iter().map(|r| (*r - mean_miss).powi(2)).sum::<f64>() / misses.len() as f64;
    println!(
        "positional variance of the miss rate: {:.4} (std dev {:.1} points)",
        variance,
        100.0 * variance.sqrt()
    );
    println!();
    println!("read the variance honestly: a *low* variance means the misses are spread evenly and the");
    println!("rate is a property of the budget; a variance at or above the mean means the misses are");
    println!("concentrated at particular positions, which is a positional blind spot. The figures");
    println!("above are the measurement -- the conclusion is theirs, not this paragraph's.");
    println!();
    println!("the mechanism behind both panels: an edit is detected only if the changed unit flips");
    println!("at least one bit's majority vote. Sampling a window is necessary but not sufficient --");
    println!("with 200 units, one changed unit often leaves every bit's margin where it was. So the");
    println!("rate is not the window coverage (3*budget/tokens); it is the probability that the");
    println!("changed units cross a bit's margin, which is why it moves in steps rather than");
    println!("tracking the budget smoothly.");
    println!();
}
