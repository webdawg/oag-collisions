//! Prints the solar-system aberration experiment's numbers.
//!
//! Run with `cargo run --example aberration`.

use oag_collisions::aberration::{
    aberration_constant, cmb_dipole_speed, drift, lightspeed_drift, measured_drift, AberrationDrift,
    AU, CMB_DIPOLE_AMPLITUDE, CMB_TEMPERATURE, EARTH_ORBITAL_SPEED, IAU_ABERRATION_CONSTANT, C,
    SUN_ORBITAL_SPEED,
};

fn main() {
    let measured = measured_drift();
    let at_c = lightspeed_drift();

    println!("solar aberration experiment");
    println!("---------------------------");
    println!(
        "measured orbital speed   {:>12.0} m/s ({:.0} km/s)",
        SUN_ORBITAL_SPEED,
        SUN_ORBITAL_SPEED / 1000.0
    );
    println!(
        "  -> drift               {:>12.3} microarcsec/year  ({:.8} deg/year)",
        measured.microarcseconds_per_year, measured.degrees_per_year
    );
    println!();
    println!(
        "hypothesis: light speed  {:>12.0} m/s",
        C
    );
    println!(
        "  -> drift               {:>12.3} microarcsec/year  ({:.6} deg/year)",
        at_c.microarcseconds_per_year, at_c.degrees_per_year
    );
    println!(
        "  -> in arcseconds/year  {:>12.3}",
        at_c.degrees_per_year * 3600.0
    );
    println!();
    println!(
        "ratio (scales as v^2)    {:>12.3e}",
        at_c.microarcseconds_per_year / measured.microarcseconds_per_year
    );
    println!(
        "measured by VLBI/Gaia   {:>12.3} microarcsec/year",
        5.0
    );

    // Second control: Earth's own orbit, at a speed known from the AU and the year, against the
    // classical aberration constant every star catalogue is corrected by.
    println!();
    println!("second control: earth's orbit");
    println!("------------------------------");
    println!(
        "earth orbital speed      {:>12.0} m/s ({:.0} km/s)",
        EARTH_ORBITAL_SPEED,
        EARTH_ORBITAL_SPEED / 1000.0
    );
    println!(
        "  aberration constant    {:>12.5} arcsec  (IAU: {IAU_ABERRATION_CONSTANT})",
        aberration_constant(EARTH_ORBITAL_SPEED)
    );
    let earth: AberrationDrift = drift(EARTH_ORBITAL_SPEED, AU);
    println!(
        "  drift                  {:>12.3} arcsec/year",
        earth.degrees_per_year * 3600.0
    );
    println!(
        "  ratio to galactic      {:>12.3e}",
        (earth.degrees_per_year * 3600.0) / (measured.degrees_per_year * 3600.0)
    );

    // Third measurement, sharing no method with either of the above: the CMB dipole. The solar
    // system's motion Doppler-shifts the oldest light in the universe, and the temperature
    // difference between the hot and cold directions is v/c.
    println!();
    println!("third measurement: the cosmic microwave background");
    println!("---------------------------------------------------");
    println!(
        "cmb temperature          {:>12.4} K   dipole {:>10.4} mK",
        CMB_TEMPERATURE,
        CMB_DIPOLE_AMPLITUDE * 1000.0
    );
    // Note this is the solar system's TOTAL speed relative to the CMB, not its galactic orbital
    // speed -- it includes the galaxy's own motion through the universe. So it is not fed through
    // the orbital-drift formula; it stands as an independent speed measurement.
    println!(
        "  solar speed vs cmb     {:>12.1} km/s",
        cmb_dipole_speed() / 1000.0
    );
    println!(
        "  ratio to light speed   {:>12.3e}",
        cmb_dipole_speed() / C
    );
}
