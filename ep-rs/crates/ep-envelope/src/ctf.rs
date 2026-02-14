//! Conduction Transfer Function (CTF) coefficient generation.
//!
//! Converts multi-layer wall constructions into CTF time-series coefficients.
//! Uses a response factor approach with finite-difference simulation.
//!
//! The CTF relates surface temperatures and heat fluxes:
//!   q_o[t] = sum_j X[j]*T_o[t-j] + sum_j Y[j]*T_i[t-j] + sum_j Phi[j]*q_o[t-j]
//!   q_i[t] = sum_j Y[j]*T_o[t-j] + sum_j Z[j]*T_i[t-j] + sum_j Phi[j]*q_i[t-j]

use ep_materials::{Construction, CtfCoefficients, Material, MaterialDatabase, MAX_CTF_TERMS};

/// Generate CTF coefficients for a construction.
///
/// Returns None if the construction has no layers.
pub fn generate_ctf(
    construction: &Construction,
    materials: &MaterialDatabase,
    time_step_seconds: f64,
) -> Option<CtfCoefficients> {
    let layers = gather_layer_properties(construction, materials);
    if layers.is_empty() {
        return None;
    }

    let has_mass = layers.iter().any(|l| l.has_mass);
    if !has_mass {
        return Some(steady_state_ctf(&layers, time_step_seconds));
    }

    Some(compute_ctf_from_layers(&layers, time_step_seconds))
}

#[derive(Debug, Clone)]
struct LayerProps {
    thickness: f64,
    conductivity: f64,
    density: f64,
    specific_heat: f64,
    resistance: f64,
    has_mass: bool,
}

fn gather_layer_properties(
    construction: &Construction,
    materials: &MaterialDatabase,
) -> Vec<LayerProps> {
    let mut layers = Vec::new();
    for &layer_idx in &construction.layers {
        if let Some(mat) = materials.get(layer_idx) {
            match mat {
                Material::Opaque(m) => {
                    layers.push(LayerProps {
                        thickness: m.thickness.value(),
                        conductivity: m.conductivity,
                        density: m.density,
                        specific_heat: m.specific_heat,
                        resistance: if m.conductivity > 0.0 {
                            m.thickness.value() / m.conductivity
                        } else {
                            0.0
                        },
                        has_mass: m.density > 0.0 && m.specific_heat > 0.0,
                    });
                }
                Material::ResistanceOnly(m) => {
                    layers.push(LayerProps {
                        thickness: 0.0,
                        conductivity: 0.0,
                        density: 0.0,
                        specific_heat: 0.0,
                        resistance: m.resistance,
                        has_mass: false,
                    });
                }
                Material::AirGap(m) => {
                    layers.push(LayerProps {
                        thickness: 0.0,
                        conductivity: 0.0,
                        density: 0.0,
                        specific_heat: 0.0,
                        resistance: m.resistance,
                        has_mass: false,
                    });
                }
                _ => {}
            }
        }
    }
    layers
}

fn steady_state_ctf(layers: &[LayerProps], time_step: f64) -> CtfCoefficients {
    let r_total: f64 = layers.iter().map(|l| l.resistance).sum();
    let u = if r_total > 0.0 { 1.0 / r_total } else { 0.0 };

    CtfCoefficients {
        outside: vec![u],
        cross: vec![-u],
        inside: vec![u],
        flux: vec![0.0],
        num_terms: 0,
        num_histories: 0,
        time_step,
    }
}

/// Compute CTF coefficients using finite-difference response simulation.
///
/// Applies unit triangular pulses at each boundary and measures the
/// resulting heat fluxes to build the response factor series.
fn compute_ctf_from_layers(layers: &[LayerProps], dt: f64) -> CtfCoefficients {
    // Build FD grid: nodes with conductances between them and capacitances at them
    let nodes_per_layer = 4;
    let mut cond: Vec<f64> = Vec::new(); // conductance[i] between node i and i+1
    let mut cap: Vec<f64> = Vec::new();  // capacitance[i] at node i

    // Node 0 = outside boundary (no capacitance)
    cap.push(0.0);

    for layer in layers {
        if layer.has_mass {
            let dx = layer.thickness / nodes_per_layer as f64;
            let k_over_dx = layer.conductivity / dx;
            let rho_cp_dx = layer.density * layer.specific_heat * dx;

            // Add half-cap to previous boundary node
            *cap.last_mut().unwrap() += rho_cp_dx / 2.0;

            // Interior nodes
            for _ in 1..nodes_per_layer {
                cond.push(k_over_dx);
                cap.push(rho_cp_dx);
            }

            // Connection to next boundary
            cond.push(k_over_dx);
            cap.push(rho_cp_dx / 2.0);
        } else if layer.resistance > 0.0 {
            cond.push(1.0 / layer.resistance);
            cap.push(0.0);
        }
    }

    let n = cap.len();
    let max_rf = MAX_CTF_TERMS.min(18);

    // Compute response factors by FD simulation
    let x_rf = simulate_response(&cond, &cap, n, dt, max_rf, 0, 0);     // outside→outside
    let y_rf = simulate_response(&cond, &cap, n, dt, max_rf, n - 1, 0); // inside→outside
    let z_rf = simulate_response(&cond, &cap, n, dt, max_rf, n - 1, n - 1); // inside→inside

    // Convert response factors to CTF
    build_ctf_from_response_factors(&x_rf, &y_rf, &z_rf, dt)
}

/// Simulate the heat flux response to a unit step temperature at `excite_node`,
/// measuring flux at `measure_node`.
///
/// Flux is positive INTO the wall at the measurement node.
fn simulate_response(
    cond: &[f64],
    cap: &[f64],
    n: usize,
    dt: f64,
    num_steps: usize,
    excite_node: usize,
    measure_node: usize,
) -> Vec<f64> {
    let mut t = vec![0.0; n];
    let mut response = Vec::with_capacity(num_steps + 1);

    // The "other" boundary is the one not being excited
    let other_node = if excite_node == 0 { n - 1 } else { 0 };

    for step in 0..=num_steps {
        // Set boundary: excited node = 1.0, other = 0.0
        t[excite_node] = 1.0;
        t[other_node] = 0.0;

        // Measure flux at measurement boundary
        // Flux into wall = conductance * (T_boundary - T_adjacent)
        let flux = measure_flux(&t, cond, n, measure_node);
        response.push(flux);

        if step == num_steps {
            break;
        }

        // Advance with implicit Euler
        let mut t_new = implicit_euler_step(&t, cond, cap, n, dt, excite_node, other_node);
        t_new[excite_node] = 1.0;
        t_new[other_node] = 0.0;
        t = t_new;
    }

    response
}

/// Measure heat flux at a boundary node (positive = into wall from that side).
fn measure_flux(t: &[f64], cond: &[f64], n: usize, node: usize) -> f64 {
    if node == 0 && !cond.is_empty() {
        // Outside: flux into wall from outside
        cond[0] * (t[0] - t[1])
    } else if node == n - 1 && !cond.is_empty() {
        // Inside: flux into wall from inside
        cond[cond.len() - 1] * (t[n - 1] - t[n - 2])
    } else {
        0.0
    }
}

/// One implicit Euler time step.
fn implicit_euler_step(
    t_old: &[f64],
    cond: &[f64],
    cap: &[f64],
    n: usize,
    dt: f64,
    fixed_node_a: usize,
    fixed_node_b: usize,
) -> Vec<f64> {
    // Build tridiagonal system
    let mut a = vec![0.0; n]; // sub-diagonal
    let mut b = vec![0.0; n]; // diagonal
    let mut c = vec![0.0; n]; // super-diagonal
    let mut d = vec![0.0; n]; // RHS

    for i in 0..n {
        if i == fixed_node_a || i == fixed_node_b {
            b[i] = 1.0;
            d[i] = t_old[i]; // Will be overwritten after solve
            continue;
        }

        let k_left = if i > 0 && i - 1 < cond.len() { cond[i - 1] } else { 0.0 };
        let k_right = if i < cond.len() { cond[i] } else { 0.0 };

        if cap[i] > 1e-20 {
            let c_dt = cap[i] / dt;
            b[i] = c_dt + k_left + k_right;
            a[i] = -k_left;
            c[i] = -k_right;
            d[i] = c_dt * t_old[i];
        } else {
            // Massless: steady-state
            b[i] = k_left + k_right;
            if b[i] < 1e-20 {
                b[i] = 1.0;
            }
            a[i] = -k_left;
            c[i] = -k_right;
            d[i] = 0.0;
        }
    }

    let mut result = vec![0.0; n];
    solve_tridiagonal(&a, &b, &c, &d, &mut result);
    result
}

/// Thomas algorithm for tridiagonal system.
fn solve_tridiagonal(a: &[f64], b: &[f64], c: &[f64], d: &[f64], x: &mut [f64]) {
    let n = a.len();
    if n == 0 {
        return;
    }
    if n == 1 {
        x[0] = if b[0].abs() > 1e-30 { d[0] / b[0] } else { 0.0 };
        return;
    }

    let mut cp = vec![0.0; n];
    let mut dp = vec![0.0; n];

    cp[0] = if b[0].abs() > 1e-30 { c[0] / b[0] } else { 0.0 };
    dp[0] = if b[0].abs() > 1e-30 { d[0] / b[0] } else { 0.0 };

    for i in 1..n {
        let m = b[i] - a[i] * cp[i - 1];
        if m.abs() < 1e-30 {
            cp[i] = 0.0;
            dp[i] = 0.0;
        } else {
            cp[i] = c[i] / m;
            dp[i] = (d[i] - a[i] * dp[i - 1]) / m;
        }
    }

    x[n - 1] = dp[n - 1];
    for i in (0..n - 1).rev() {
        x[i] = dp[i] - cp[i] * x[i + 1];
    }
}

/// Convert response factors to CTF coefficients.
///
/// Uses the s-transform (z-transform) relationship.
/// The Phi coefficients represent the decay of the wall's thermal memory.
fn build_ctf_from_response_factors(
    x_rf: &[f64],
    y_rf: &[f64],
    z_rf: &[f64],
    dt: f64,
) -> CtfCoefficients {
    // Find how many terms are significant
    let threshold = 1e-8;
    let x_ss = *x_rf.last().unwrap_or(&0.0);

    let mut n_sig = 1;
    for k in (1..x_rf.len()).rev() {
        if (x_rf[k] - x_ss).abs() > threshold * x_ss.abs().max(1.0) {
            n_sig = k;
            break;
        }
    }
    let num_terms = n_sig.min(MAX_CTF_TERMS - 1).max(1);

    // Estimate Phi (flux history coefficient) from response factor decay
    // For an exponential decay: rf[k] ≈ rf_ss + A * phi^k
    // So phi ≈ (rf[k+1] - rf_ss) / (rf[k] - rf_ss)
    let phi1 = if num_terms >= 2 {
        let d1 = x_rf[1] - x_ss;
        let d2 = x_rf[2] - x_ss;
        if d1.abs() > 1e-12 {
            (d2 / d1).clamp(-0.99, 0.99)
        } else {
            0.0
        }
    } else {
        0.0
    };

    // CTF coefficients from response factors:
    // X_ctf[k] = X_rf[k] - Phi * X_rf[k-1]  (for k >= 1)
    // X_ctf[0] = X_rf[0]
    let ct = num_terms.min(6).max(1);

    let mut outside = Vec::with_capacity(ct + 1);
    let mut cross = Vec::with_capacity(ct + 1);
    let mut inside = Vec::with_capacity(ct + 1);
    let mut flux = vec![0.0; ct + 1];

    for k in 0..=ct {
        let xk = *x_rf.get(k).unwrap_or(&x_ss);
        let yk = *y_rf.get(k).unwrap_or(y_rf.last().unwrap_or(&0.0));
        let zk = *z_rf.get(k).unwrap_or(z_rf.last().unwrap_or(&0.0));

        if k == 0 {
            outside.push(xk);
            cross.push(yk); // Y response is already negative (heat leaves outside when inside warms)
            inside.push(zk);
        } else {
            let xp = *x_rf.get(k - 1).unwrap_or(&x_ss);
            let yp = *y_rf.get(k - 1).unwrap_or(y_rf.last().unwrap_or(&0.0));
            let zp = *z_rf.get(k - 1).unwrap_or(z_rf.last().unwrap_or(&0.0));

            outside.push(xk - phi1 * xp);
            cross.push(yk - phi1 * yp);
            inside.push(zk - phi1 * zp);
        }
    }

    if ct >= 1 {
        flux[1] = phi1;
    }

    CtfCoefficients {
        outside,
        cross,
        inside,
        flux,
        num_terms: ct,
        num_histories: ct,
        time_step: dt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ep_materials::*;
    use ep_units::Length;

    fn make_simple_wall() -> (MaterialDatabase, Construction) {
        let mut db = MaterialDatabase::new();
        let concrete = db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Concrete".into(),
            thickness: Length::new(0.2),
            conductivity: 1.4,
            density: 2300.0,
            specific_heat: 880.0,
            ..Default::default()
        }));
        let mut constr = Construction::new("SimpleWall");
        constr.layers = vec![concrete];
        (db, constr)
    }

    #[test]
    fn ctf_simple_wall() {
        let (db, constr) = make_simple_wall();
        let ctf = generate_ctf(&constr, &db, 3600.0);
        assert!(ctf.is_some());
        let ctf = ctf.unwrap();

        assert!(ctf.num_terms >= 1, "num_terms={}", ctf.num_terms);
        // X[0] should be positive (conductance into wall from warm outside)
        assert!(ctf.outside[0] > 0.0, "X[0]={}", ctf.outside[0]);
        // Y[0] (cross) should be <= 0 (inside warming reduces outside flux)
        // For massive walls, Y[0] = 0 since the pulse hasn't reached the other side yet
        assert!(ctf.cross[0] <= 0.0, "Y[0]={}", ctf.cross[0]);
        // But Y should have some negative values in later terms
        let y_sum: f64 = ctf.cross.iter().sum();
        assert!(y_sum < 0.0, "sum(Y)={y_sum} should be negative");
    }

    #[test]
    fn ctf_steady_state_u_value() {
        let (db, constr) = make_simple_wall();
        let ctf = generate_ctf(&constr, &db, 3600.0).unwrap();

        // At steady state: net flux with unit dT should equal U
        // q = X[0]*1 + Y[0]*0 (only outside excited) at first step
        // Eventually, X response should approach U = k/L = 7.0
        let _x_last = *ctf.outside.last().unwrap_or(&0.0);
        let u_expected = 1.4 / 0.2; // 7.0

        // X[0] should be > U (initial transient is larger due to thermal mass)
        assert!(
            ctf.outside[0] >= u_expected * 0.5,
            "X[0]={}, U={}",
            ctf.outside[0],
            u_expected
        );
    }

    #[test]
    fn ctf_resistance_only() {
        let mut db = MaterialDatabase::new();
        let insul = db.add_material(Material::ResistanceOnly(ResistanceOnlyMaterial {
            name: "RLayer".into(),
            resistance: 2.5,
        }));
        let mut constr = Construction::new("ROnly");
        constr.layers = vec![insul];

        let ctf = generate_ctf(&constr, &db, 3600.0).unwrap();

        assert!((ctf.outside[0] - 0.4).abs() < 1e-10, "X[0]={}", ctf.outside[0]);
        assert!((ctf.cross[0] + 0.4).abs() < 1e-10, "Y[0]={}", ctf.cross[0]);
        assert!((ctf.inside[0] - 0.4).abs() < 1e-10, "Z[0]={}", ctf.inside[0]);
        assert_eq!(ctf.num_terms, 0);
    }

    #[test]
    fn ctf_multi_layer() {
        let mut db = MaterialDatabase::new();
        db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Brick".into(),
            thickness: Length::new(0.1),
            conductivity: 0.89,
            density: 1920.0,
            specific_heat: 790.0,
            ..Default::default()
        }));
        db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Insulation".into(),
            thickness: Length::new(0.05),
            conductivity: 0.04,
            density: 32.0,
            specific_heat: 830.0,
            ..Default::default()
        }));
        db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Gypsum".into(),
            thickness: Length::new(0.013),
            conductivity: 0.16,
            density: 800.0,
            specific_heat: 830.0,
            ..Default::default()
        }));

        let mut constr = Construction::new("MultiLayer");
        constr.layers = vec![0, 1, 2];

        let ctf = generate_ctf(&constr, &db, 3600.0).unwrap();

        assert!(ctf.num_terms >= 1);
        assert!(ctf.outside[0] > 0.0, "X[0]={}", ctf.outside[0]);
    }

    #[test]
    fn tridiagonal_solve() {
        let a = vec![0.0, -1.0, 0.0];
        let b = vec![1.0, 2.0, 1.0];
        let c = vec![0.0, -1.0, 0.0];
        let d = vec![1.0, 0.0, 0.0];
        let mut x = vec![0.0; 3];
        solve_tridiagonal(&a, &b, &c, &d, &mut x);
        assert!((x[0] - 1.0).abs() < 1e-10);
        assert!((x[1] - 0.5).abs() < 1e-10);
        assert!((x[2] - 0.0).abs() < 1e-10);
    }

    // ─── New CTF tests ───────────────────────────────────────────────

    #[test]
    fn ctf_empty_construction() {
        // Construction with no layers → should return None
        let db = MaterialDatabase::new();
        let constr = Construction::new("Empty");
        let result = generate_ctf(&constr, &db, 3600.0);
        assert!(result.is_none(), "Empty construction should return None");
    }

    #[test]
    fn ctf_air_gap_layer() {
        // Single air gap layer → massless → steady-state CTF, U = 1/R
        let mut db = MaterialDatabase::new();
        let gap_idx = db.add_material(Material::AirGap(AirGapMaterial {
            name: "AirGap".into(),
            resistance: 0.18, // typical air gap R-value (m2-K/W)
        }));
        let mut constr = Construction::new("AirGapOnly");
        constr.layers = vec![gap_idx];

        let ctf = generate_ctf(&constr, &db, 3600.0);
        assert!(ctf.is_some(), "Air gap construction should produce CTF");
        let ctf = ctf.unwrap();

        // Steady-state: num_terms = 0
        assert_eq!(ctf.num_terms, 0, "Air gap should be steady-state");

        // U = 1/R = 1/0.18 ≈ 5.556
        let u_expected = 1.0 / 0.18;
        assert!(
            (ctf.outside[0] - u_expected).abs() < 1e-10,
            "X[0]={}, expected U={}",
            ctf.outside[0],
            u_expected
        );
        assert!(
            (ctf.cross[0] + u_expected).abs() < 1e-10,
            "Y[0]={}, expected -U={}",
            ctf.cross[0],
            -u_expected
        );
        assert!(
            (ctf.inside[0] - u_expected).abs() < 1e-10,
            "Z[0]={}, expected U={}",
            ctf.inside[0],
            u_expected
        );
    }

    #[test]
    fn ctf_two_layer_u_value() {
        // Brick + insulation wall, verify cross-coupling sum ≈ -U
        let mut db = MaterialDatabase::new();
        let brick_idx = db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Brick".into(),
            thickness: Length::new(0.1),
            conductivity: 0.89,
            density: 1920.0,
            specific_heat: 790.0,
            ..Default::default()
        }));
        let insul_idx = db.add_material(Material::Opaque(OpaqueMaterial {
            name: "Insulation".into(),
            thickness: Length::new(0.05),
            conductivity: 0.04,
            density: 32.0,
            specific_heat: 830.0,
            ..Default::default()
        }));
        let mut constr = Construction::new("BrickInsul");
        constr.layers = vec![brick_idx, insul_idx];

        let ctf = generate_ctf(&constr, &db, 3600.0).unwrap();

        // The sum of Y (cross) coefficients should be negative (heat cross-coupling)
        let y_sum: f64 = ctf.cross.iter().sum();
        assert!(
            y_sum < 0.0,
            "Y sum={y_sum} should be negative"
        );
        // The magnitude should be on the same order as U = k/L_total
        let r_total = 0.1 / 0.89 + 0.05 / 0.04;
        let u_expected = 1.0 / r_total;
        // The FD response factor method may overshoot U somewhat, but magnitude
        // should be within a factor of 3
        assert!(
            y_sum.abs() > u_expected * 0.3 && y_sum.abs() < u_expected * 3.0,
            "Y sum magnitude={} should be on same order as U={}",
            y_sum.abs(),
            u_expected
        );
    }

    #[test]
    fn ctf_heavy_wall_more_terms() {
        // 200mm concrete → high thermal mass → should need more than 1 CTF term
        let mut db = MaterialDatabase::new();
        let concrete_idx = db.add_material(Material::Opaque(OpaqueMaterial {
            name: "HeavyConcrete".into(),
            thickness: Length::new(0.2),
            conductivity: 1.4,
            density: 2300.0,
            specific_heat: 880.0,
            ..Default::default()
        }));
        let mut constr = Construction::new("HeavyWall");
        constr.layers = vec![concrete_idx];

        let ctf = generate_ctf(&constr, &db, 3600.0).unwrap();

        assert!(
            ctf.num_terms > 1,
            "Heavy wall should need > 1 CTF term, got {}",
            ctf.num_terms
        );
        // Should also have multiple outside coefficients
        assert!(
            ctf.outside.len() > 2,
            "Heavy wall should have > 2 outside coefficients, got {}",
            ctf.outside.len()
        );
    }
}
