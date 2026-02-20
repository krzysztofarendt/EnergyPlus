//! BESTEST Case 600 CLI runner.
//!
//! Reads a Denver-compatible EPW weather file and runs a full annual
//! simulation using real CTF envelope + window thermal + solar physics.
//!
//! Usage:
//!   cargo run -p ep-run -- <path/to/weather.epw>

use std::path::Path;
use std::time::Instant;

use ep_core::state::SimulationState;
use ep_sim::bestest600::Bestest600Callback;
use ep_sim::{RunPeriod, SimulationConfig, SimulationDriver};
use ep_weather::WeatherFile;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: ep-run <weather.epw>");
        eprintln!();
        eprintln!("Runs BESTEST Case 600 (lightweight box, Denver) and validates");
        eprintln!("annual heating/cooling energy against ASHRAE Standard 140.");
        std::process::exit(1);
    }

    let epw_path = &args[1];
    println!("Loading weather file: {epw_path}");
    let weather = WeatherFile::from_path(Path::new(epw_path)).unwrap_or_else(|e| {
        eprintln!("Failed to parse EPW file: {e}");
        std::process::exit(1);
    });

    println!(
        "Location: {} (lat={:.2}, lon={:.2}, tz={:.0}, elev={:.0}m)",
        weather.location.name,
        weather.location.latitude,
        weather.location.longitude,
        weather.location.time_zone,
        weather.location.elevation,
    );
    println!("Records: {}", weather.records.len());

    // Create callback with real physics
    let timesteps_per_hour = 1; // BESTEST standard allows 1 ts/hr
    let mut callback = Bestest600Callback::new(weather, timesteps_per_hour);

    // Configure simulation driver
    let config = SimulationConfig {
        timesteps_per_hour,
        max_warmup_days: 25,
        min_warmup_days: 6,
        warmup_tolerance: 0.04,
        max_hvac_iterations: 20,
        hvac_tolerance: 0.5,
        run_design_days: false,
        run_weather_periods: true,
        ..Default::default()
    };
    let mut driver = SimulationDriver::new(config);
    driver.run_periods.push(RunPeriod::new("Annual", 1, 1, 12, 31));

    // Run simulation
    let mut state = SimulationState::new(timesteps_per_hour);
    println!("\nStarting annual simulation...");
    let start = Instant::now();

    let result = driver.run(&mut state, &mut callback);

    let elapsed = start.elapsed();
    println!("Simulation complete in {:.2}s", elapsed.as_secs_f64());
    println!(
        "  Environments: {}, Warmup days: {:?}",
        result.environments_completed, result.warmup_days_used
    );
    println!(
        "  Timesteps: {}, HVAC iterations: {} (avg {:.1}/ts)",
        result.total_timesteps,
        result.total_hvac_iterations,
        result.avg_hvac_iterations()
    );

    // Write CSV output
    let csv_path = "bestest600_results.csv";
    match callback.write_csv(csv_path) {
        Ok(()) => println!("  CSV output: {csv_path} ({} hourly records)", callback.hourly_data.len()),
        Err(e) => eprintln!("  Warning: failed to write CSV: {e}"),
    }

    // Print validation summary
    callback.print_validation_summary();
}
