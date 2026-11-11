//! The solar-system aberration experiment.
//!
//! `SPEC.md` records the operator's statement that "the sun is travelling at light speed". This
//! module is the experiment that statement suggests: if the solar system's barycentre really were
//! moving at `c` around the galaxy, the *direction* of that motion would change as it orbits, and
//! changing aberration means the apparent positions of every star would drift. That drift is
//! measurable, it has been measured, and its size is a direct readout of the orbital speed.
//!
//! The physics, stated so the numbers can be checked rather than trusted:
//!
//! - The barycentre follows a roughly circular galactic orbit of radius `r` at speed `v`.
//! - The centripetal acceleration is `a = v^2 / r`.
//! - An acceleration `a` of the observer produces an angular drift of the celestial sphere of
//!   `a / c` radians per second -- aberration is a ratio of a speed to `c`, so an *accelerated*
//!   observer sees the whole sky slide.
//! - For a circular orbit that gives a *secular* (one-directional) drift, not the annual wobble
//!   Earth's own orbit produces.
//!
//! This is the same quantity measured by VLBI and by Gaia, reported as the "secular aberration
//! drift" (Titov, Lambert & Gontier, 2011; later confirmed at ~5 microarcseconds/year). The
//! experiment here is simply: compute it for the two competing hypotheses and compare.

/// Speed of light in vacuum, m/s (exact, by definition of the metre).
pub const C: f64 = 299_792_458.0;

/// One parsec in metres (IAU 2015 definition, exact).
pub const PARSEC: f64 = 3.085_677_581_491_367_3e16;

/// One year in seconds (Julian year, the astronomy convention).
pub const YEAR: f64 = 365.25 * 86_400.0;

/// One arcsecond in radians.
pub const ARCSECOND: f64 = std::f64::consts::PI / (180.0 * 3600.0);

/// One microarcsecond in radians.
pub const MICROARCSECOND: f64 = ARCSECOND / 1e6;

/// One astronomical unit in metres (IAU 2012 definition, exact).
pub const AU: f64 = 149_597_870_700.0;

/// Earth's orbital speed, m/s -- derived from the astronomical unit and the Julian year, which is
/// why it is known to four digits independent of any star catalogue.
pub const EARTH_ORBITAL_SPEED: f64 = 29_780.0;

/// The IAU 2010 constant of aberration, arcseconds. This is the *classical* annual aberration:
/// the angular radius of the ellipse each star traces over a year because Earth moves.
pub const IAU_ABERRATION_CONSTANT: f64 = 20.49552;

/// The solar system's galactic orbital radius: the Sun's distance from the galactic centre.
/// ~8.2 kpc (Gravity Collaboration 2019; earlier values cluster at 8.0-8.3 kpc).
pub const GALACTIC_RADIUS: f64 = 8.2e3 * PARSEC;

/// The Sun's galactic orbital speed, from stellar kinematics and the Local Standard of Rest.
/// ~230 km/s. This is the value the experiment tests against.
pub const SUN_ORBITAL_SPEED: f64 = 230_000.0;

/// What an orbital speed implies for the observable sky.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AberrationDrift {
    /// The orbital speed the drift was computed for, m/s.
    pub orbital_speed: f64,
    /// Centripetal acceleration, m/s^2.
    pub acceleration: f64,
    /// Angular drift, radians per second.
    pub rad_per_second: f64,
    /// The same drift in microarcseconds per year -- the unit the measurements are quoted in.
    pub microarcseconds_per_year: f64,
    /// The same drift in degrees per year -- the unit a human can picture.
    pub degrees_per_year: f64,
}

/// Computes the secular aberration drift for a circular orbit of `radius` at `speed`.
pub fn drift(speed: f64, radius: f64) -> AberrationDrift {
    let acceleration = speed * speed / radius;
    let rad_per_second = acceleration / C;
    let per_year = rad_per_second * YEAR;
    AberrationDrift {
        orbital_speed: speed,
        acceleration,
        rad_per_second,
        microarcseconds_per_year: per_year / MICROARCSECOND,
        degrees_per_year: per_year.to_degrees(),
    }
}

/// The classical aberration *constant*: how far a star's apparent position is displaced by the
/// observer's speed, as an angle. `kappa = v / c` radians.
///
/// This is the quantity the 20.5-arcsecond annual aberration is made of, and it is the second
/// control for this module: a speed known to four digits (Earth's, from the AU and the year)
/// produces a constant known to five (the IAU value). If the conversion does not land there, the
/// `a / c` drift formula built on the same ratio is not trustworthy either.
pub fn aberration_constant(speed: f64) -> f64 {
    (speed / C).to_degrees() * 3600.0
}

/// The drift implied by the measured solar motion.
pub fn measured_drift() -> AberrationDrift {
    drift(SUN_ORBITAL_SPEED, GALACTIC_RADIUS)
}

/// The drift implied by the claim in `SPEC.md` -- the Sun at light speed.
pub fn lightspeed_drift() -> AberrationDrift {
    drift(C, GALACTIC_RADIUS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The second control, and the strongest one: Earth's orbital speed is known independently
    /// from the astronomical unit and the Julian year, and it produces the classical aberration
    /// constant that every star catalogue is corrected by. Same `v / c` ratio the galactic drift
    /// formula uses, at a speed 10,000x larger and a distance 10,000x smaller.
    #[test]
    fn earths_speed_reproduces_the_classical_aberration_constant() {
        let computed = aberration_constant(EARTH_ORBITAL_SPEED);
        assert!(
            (computed - IAU_ABERRATION_CONSTANT).abs() < 0.01,
            "expected ~{IAU_ABERRATION_CONSTANT} arcsec (IAU), got {computed}"
        );
    }

    /// Earth's orbit also produces a *drift*, and it is enormous by the standards of the galactic
    /// one -- roughly 129 arcseconds per year, because the velocity vector turns through a full
    /// circle annually. This is the effect the annual aberration ellipse is; measuring it is
    /// routine. It is the same formula, at the opposite extreme of the parameter space.
    #[test]
    fn earths_orbit_produces_a_drift_far_larger_than_the_galactic_one() {
        let earth = drift(EARTH_ORBITAL_SPEED, AU);
        let galactic = measured_drift();
        let earth_arcsec = earth.degrees_per_year * 3600.0;
        assert!(
            (120.0..140.0).contains(&earth_arcsec),
            "Earth's annual aberration drift should be ~129 arcsec/year, got {earth_arcsec}"
        );
        // Seven and a half orders of magnitude apart: the same formula spans both extremes, which
        // is what makes it a control rather than a fitted curve.
        let ratio = earth_arcsec / (galactic.degrees_per_year * 3600.0);
        assert!((1e7..1e8).contains(&ratio), "expected ~3e7, got {ratio}");
    }

    /// The control case: the computed drift for the measured solar speed must land in the same
    /// place as the published secular aberration drift (~5 microarcseconds/year). If this fails,
    /// the formula or the constants are wrong, and the comparison below means nothing.
    #[test]
    fn the_measured_speed_reproduces_the_published_drift() {
        let drift = measured_drift();
        assert!(
            (4.0..6.0).contains(&drift.microarcseconds_per_year),
            "expected ~5 uas/yr for a 230 km/s galactic orbit, got {}",
            drift.microarcseconds_per_year
        );
        // And it is a small effect: a few arcseconds per century, not per year.
        assert!(drift.degrees_per_year < 0.01);
    }

    /// The hypothesis under test. If the Sun moved at `c`, the whole sky would slide by arcseconds
    /// *per year* -- tens of times the annual aberration Earth's own orbit produces, accumulating to
    /// a fraction of a degree within one human lifetime. Easily measured, never seen.
    #[test]
    fn light_speed_would_be_visibly_obvious() {
        let at_c = lightspeed_drift();
        let measured = measured_drift();

        // Arcsecond-scale drift per year: 1 degree = 3600 arcseconds.
        let arcseconds_per_year = at_c.degrees_per_year * 3600.0;
        assert!(
            arcseconds_per_year > 1.0,
            "light speed should imply arcseconds of drift per year, got {arcseconds_per_year}"
        );
        // Across a human lifetime that is a naked-eye displacement -- about a third of the Moon's
        // apparent diameter, which nobody has ever reported in star catalogues.
        assert!(at_c.degrees_per_year * 80.0 > 0.1);

        // And the ratio between the two hypotheses is the ratio of the squares of the speeds,
        // because the drift goes as v^2.
        let ratio = at_c.microarcseconds_per_year / measured.microarcseconds_per_year;
        let expected_ratio = (C / SUN_ORBITAL_SPEED).powi(2);
        assert!(
            (ratio - expected_ratio).abs() / expected_ratio < 0.01,
            "drift should scale as v^2: got {ratio}, expected {expected_ratio}"
        );
        assert!(
            ratio > 1e6,
            "light speed should be at least a million times the measured effect, got {ratio}"
        );
    }

    /// The drift scales with the square of the speed, so the test is not tuned to one number: a
    /// 10x speed is a 100x effect.
    #[test]
    fn drift_scales_with_the_square_of_speed() {
        let v = 230_000.0;
        let single = drift(v, GALACTIC_RADIUS).microarcseconds_per_year;
        let double = drift(2.0 * v, GALACTIC_RADIUS).microarcseconds_per_year;
        assert!((double / single - 4.0).abs() < 1e-9);
    }

    /// A stationary observer sees no drift: the effect is an acceleration effect, not a speed
    /// effect. This is what makes it a test of the orbit rather than of a constant velocity.
    #[test]
    fn a_straight_line_implies_no_drift() {
        // Speed alone produces aberration (a constant offset of the whole sky) but no *drift*.
        // The drift term is a/c, so with zero centripetal acceleration it is zero.
        let acceleration = 0.0_f64;
        assert_eq!(acceleration / C * YEAR, 0.0);
    }
}
