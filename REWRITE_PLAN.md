# EnergyPlus Rust Rewrite Plan

## Project Charter for a Modern Building Energy Simulation Engine

**Version:** 2.0
**Date:** 2026-02-14
**Status:** In Progress — Phases 0–9 Complete, Simulation Driver Pending

---

## Table of Contents

1. [Current State Assessment](#1-current-state-assessment)
2. [Gap Analysis](#2-gap-analysis)
3. [Phase 6: Core Heat Balance](#3-phase-6-core-heat-balance)
4. [Phase 7: Solar, Shading & Daylighting](#4-phase-7-solar-shading--daylighting)
5. [Phase 8: HVAC Integration](#5-phase-8-hvac-integration)
6. [Phase 9: Plant Loop Integration](#6-phase-9-plant-loop-integration)
7. [Phase 10: Simulation Driver & Sizing](#7-phase-10-simulation-driver--sizing)
8. [Phase 11: Equipment Breadth](#8-phase-11-equipment-breadth)
9. [Phase 12: Advanced Features & Parity](#9-phase-12-advanced-features--parity)
10. [Crate-Level Summary](#10-crate-level-summary)
11. [Priority & Dependencies](#11-priority--dependencies)
12. [Verification Strategy](#12-verification-strategy)

---

## 1. Current State Assessment

The Rust workspace (`ep-rs/`) contains **33 crates** with **1,070 passing tests** and **~42,000 lines of Rust**. Phases 0–9 are complete. The C++ EnergyPlus codebase is **~800K lines** across 200+ modules.

### 1.1 Completed Work

| Category | Crate(s) | Tests | Status |
|----------|----------|-------|--------|
| **Units & Types** | ep-units | 16 | Complete — `quantity!` macro for zero-cost newtypes |
| **Core Runtime** | ep-core | 6 | Complete — state struct, time management, environment types |
| **Psychrometrics** | ep-psychrometrics | 15 | Complete — all ASHRAE moist air property functions |
| **Fluid Properties** | ep-fluids | 11 | Good — water, refrigerant, air property lookups |
| **Performance Curves** | ep-curves | 12 | Complete — all 21 EnergyPlus curve types |
| **Weather** | ep-weather | 13 | Good — EPW parsing, design days, solar position |
| **Schedules** | ep-schedule | 40 | Good — Year/Week/Day/Compact/File schedules |
| **Materials** | ep-materials | 10 | Good — construction layers, glass optical properties |
| **I/O Framework** | ep-io | 48 | Good — IDF parser, ~15 object schema defs, macro preprocessor |
| **Surface Geometry** | ep-surfaces | 20 | Partial — vertices, normals, area, tilt, zone topology; **no heat balance** |
| **Envelope** | ep-envelope | 77 | Good — CTF, convection (18 correlations), radiant exchange (ScriptF), heat balance solver |
| **Window Optics** | ep-windows | 48 | Good — angular optics, thermal solver (tridiagonal), shading devices, frame/divider |
| **Solar Incident** | ep-solar | 40 | Good — sun position, Perez/HDKR, shadow casting, solar distribution |
| **Ground Temp** | ep-ground | 18 | Partial — Kusuda model, monthly interpolation; **no Kiva/3D** |
| **Zone Air Balance** | ep-zone-air | 14 | Moderate — 3 solution methods, infiltration; **limited HVAC coupling** |
| **Internal Gains** | ep-internal-gains | 32 | Good — occupancy, lights, equipment with fraction splits |
| **Airflow Network** | ep-airflow | 18 | Moderate — Newton-Raphson solver; **few component models** |
| **Daylighting** | ep-daylighting | 61 | Good — sky luminance, glare, illuminance, interior reflections, Tregenza patches, daylight factors |
| **Node Infrastructure** | ep-nodes | 14 | Complete — node struct, mixer, splitter, OA mixer |
| **Fans** | ep-fans | 11 | Moderate — constant/variable/on-off; **no detailed performance curves** |
| **Coils** | ep-coils | 53 | Good — DX (defrost, crankcase, PLF, SHR), water (dry/wet, ε-NTU), heating, HX |
| **Plant Equipment** | ep-plant | 134 | Good — boiler, chillers (EIR/reformulated/absorption/constant-COP), tower, pumps (headered), heat pumps, GHX, stratified tank, ice storage, loop solver |
| **HVAC Framework** | ep-hvac | 101 | Good — air loop solver, zone equipment dispatch, setpoint managers, OA/PI controllers, terminals |
| **EMS** | ep-ems | 42 | Good — ERL execution, sensors, actuators, trends  |
| **FMI** | ep-fmi | 7 | Good — FMI 2.0 co-simulation, variable exchange |
| **Generation** | ep-generation | 34 | Good — PV, wind, battery (KiBaM), generators, inverter |
| **Demand** | ep-demand | 21 | Good — demand managers, tariffs, life cycle cost |
| **Refrigeration** | ep-refrigeration | 25 | Good — display cases, walk-ins, compressors, condensers |
| **Water Systems** | ep-water | 21 | Good — storage heater, tankless, solar thermal, fixtures |
| **API** | ep-api | 6 | Good — C FFI, variable registration, callbacks |
| **Output** | ep-output | 43 | Good — variables, meters, ESO/MTR/CSV/SQL writers, tabular |
| **Simulation Driver** | ep-sim | 48 | Good framework — warmup, convergence, sizing; **run loop stubbed** |
| | | **1,070** | |
| **Validation** | ep-validation | 21 | Good — BESTEST cases, numerical comparison, regression |

### 1.2 Key Insight

Phases 6–8 added the core simulation physics: surface heat balance (CTF, radiant exchange, convection), solar/shading/daylighting, and HVAC integration (air loop solver, zone equipment dispatch, controllers). The remaining blockers for end-to-end simulation are the plant loop solver (Phase 9) and the simulation driver wiring (Phase 10). Equipment breadth (Phase 11) and advanced features (Phase 12) can proceed incrementally after that.

---

## 2. Gap Analysis

### 2.1 Critical Path Blockers

These items must be completed before any IDF file can run end-to-end:

| Blocker | What's Missing | C++ Reference | Status |
|---------|---------------|---------------|--------|
| Surface heat balance | CTF conduction solver, inside/outside balance | `HeatBalanceSurfaceManager.cc` (10K) | **Done (Phase 6)** |
| Radiant exchange | View factor matrix, inter-surface LW exchange | `HeatBalanceIntRadExchange.cc` (2.1K) | **Done (Phase 6)** |
| Window thermal | Multi-layer glass temps, gap convection | `WindowManager.cc` thermal sections (~3K) | **Done (Phase 6)** |
| Shadow casting | Polygon clipping, sunlit fractions | `SolarShading.cc` (13K) | **Done (Phase 7)** |
| HVAC air loop | Component sequencing, convergence iteration | `SimAirServingZones.cc` (7.8K) | **Done (Phase 8)** |
| Zone equipment | Load calculation, equipment dispatch | `ZoneEquipmentManager.cc` (7.1K) | **Done (Phase 8)** |
| Plant loop solver | Half-loop iteration, flow resolution | `Plant/LoopSide.cc` + `PlantManager.cc` (7K) | **Done (Phase 9)** |
| Simulation loop | Environment→Day→Hour→Timestep→HVAC loop | `SimulationManager.cc` (~3K) | ~800 lines |
| IDF schema | ~50 core object defs (currently ~15) | IDD schema | ~1,500 lines |

### 2.2 Equipment Model Gap

The C++ codebase has extensive equipment breadth not yet replicated:

| C++ Subsystem | C++ Lines | IDF Object Types | Rust Coverage |
|---------------|-----------|------------------|---------------|
| Air terminals (VAV, dual duct, PIU) | ~11K | 13 | CV, CV+Reheat, VAV+Reheat, VAV (4 types) |
| Unitary systems & furnaces | ~30K | 8+ | Basic unitary with heat pump support |
| VRF systems | ~16K | 4 | None |
| DX coils (multi-speed, two-stage) | ~22K | 10 | Single-speed with defrost/crankcase/PLF |
| Heat recovery (air-to-air) | ~5K | 4 | None |
| Zone HVAC (fan coils, baseboards, radiant) | ~30K | 20+ | Baseboard convective water |
| Chillers (7 types) | ~23K | 9 | EIR only |
| Heat pumps (water-to-water, plant EIR) | ~12K | 6 | None |
| Ground heat exchangers | ~10K | 5 | None |
| Thermal storage (ice, stratified tank) | ~16K | 6 | Mixed tank only |
| Additional towers/coolers | ~10K | 8 | Single-speed tower only |
| Thermal comfort models | ~3K | 6 models | None |
| Room air models | ~5K | 5 models | None |
| CondFD (finite difference conduction) | ~3K | N/A | None |
| Advanced fenestration (EQL, BSDF) | ~12K | Complex | None |
| Tabular output reports | ~20K | N/A | Framework only |
| Sizing (50+ autosizing classes) | ~20K | N/A | Struct stubs |

---

## 3. Phase 6: Core Heat Balance ✅

**Status:** Complete (54 new tests, 878 total at completion)

**Goal:** Solve surface temperatures correctly. A building with opaque walls, roof, and slab should produce converged inside/outside surface temperatures at each timestep.

**Crates modified:** ep-envelope, ep-surfaces, ep-windows

### 3.1 CTF Conduction Solver

**File:** `crates/ep-envelope/src/ctf.rs` (expand existing)

Complete the Conduction Transfer Function implementation:

- **State-space response factor method:** Build system matrices [A, B, C, D] from layer thermal properties (conductivity, density, specific heat, thickness)
- **Exponential matrix:** Padé approximation or Taylor series for matrix exponential `exp(A*Δt)`
- **CTF coefficient extraction:** Outside[], Cross[], Inside[], Flux[] arrays via matrix regression
- **History management:** Up to 19 CTF terms with ratio-based cutoff at 1e-13
- **Adaptive time stepping:** Double timestep until Fourier number satisfies stability criterion
- **Special cases:** Air gaps (steady-state R-value only), massless layers (resistance only)

```rust
pub struct CtfCoefficients {
    pub outside: Vec<f64>,    // X[j]: outside self-response
    pub cross: Vec<f64>,      // Y[j]: cross-coupling (negative)
    pub inside: Vec<f64>,     // Z[j]: inside self-response
    pub flux: Vec<f64>,       // Φ[j]: flux history
    pub num_terms: usize,
}

impl CtfCoefficients {
    pub fn from_construction(layers: &[MaterialLayer], timestep_s: f64) -> Option<Self>;
    pub fn steady_state_u_value(&self) -> f64;
}
```

*C++ reference:* `Construction.cc::calculateTransferFunction()` — state-space method with exponential matrix, gamma calculation, final coefficient extraction.

### 3.2 Surface Heat Balance Manager

**File:** `crates/ep-envelope/src/surface_heat_balance.rs` (new)

Implement the coupled inside/outside surface temperature solver:

**Outside surface balance:**
```
q"_out = α_sol * I_sol + h_conv_out * (T_air - T_surf) + h_rad * (T_sky - T_surf)
       - ε * σ * (T_surf⁴ - T_sky⁴)   [linearized as h_rad term]
```

**Inside surface balance:**
```
q"_in = h_conv_in * (T_zone - T_surf) + Σ(h_rad_ij * (T_surf_j - T_surf_i)) + q"_solar_absorbed
```

**CTF coupling (inside surface equation):**
```
q"_in = X[0]*T_out + Y[0]*T_in + Σ(X[j]*T_out_hist[j] + Y[j]*T_in_hist[j] + Φ[j]*q"_hist[j])
```

Key functions:
```rust
pub fn calc_outside_surface_temp(
    surface: &Surface,
    ctf: &CtfCoefficients,
    weather: &HourlyWeather,
    history: &SurfaceHistory,
) -> f64;

pub fn calc_inside_surface_temp(
    surface: &Surface,
    ctf: &CtfCoefficients,
    zone_air_temp: f64,
    radiant_exchange: &[f64],  // net radiant from other surfaces
    solar_absorbed: f64,
    history: &SurfaceHistory,
) -> f64;

pub fn solve_zone_surfaces(
    zone: &Zone,
    surfaces: &mut [SurfaceState],
    zone_air_temp: f64,
    weather: &HourlyWeather,
) -> ZoneHeatBalanceResult;
```

*C++ reference:* `HeatBalanceSurfaceManager.cc` — `CalcHeatBalanceInsideSurf2()`, `CalcOutsideSurfTemp()`.

### 3.3 Interior Radiant Exchange

**File:** `crates/ep-envelope/src/radiant_exchange.rs` (new)

Implement inter-surface long-wave radiation exchange within an enclosure:

- **View factors:** Area-weighted approximation for rectangular enclosures (F_ij = A_j / Σ A_k for diffuse enclosure). Optional: exact view factor formulas for parallel/perpendicular rectangles.
- **ScriptF matrix** (Hottel method): Accounts for emissivity and multiple reflections.
  ```
  ScriptF[i,j] = F[i,j] * ε_j / (1 - (1-ε_i)*Σ(F[i,k]*(1-ε_k)))
  ```
- **Radiosity solve:** For N surfaces, solve NxN linear system for radiosities, then compute net flux per surface.
- **Carroll MRT method** (simplified alternative): Mean Radiant Temperature approach requiring only per-surface resistance calculation.

```rust
pub struct RadiantEnclosure {
    pub surface_indices: Vec<usize>,
    pub view_factors: Vec<Vec<f64>>,  // F[i][j]
    pub script_f: Vec<Vec<f64>>,       // ScriptF[i][j]
}

impl RadiantEnclosure {
    pub fn from_surfaces(surfaces: &[&Surface]) -> Self;
    pub fn calc_exchange(&self, surface_temps: &[f64], emissivities: &[f64]) -> Vec<f64>;
}
```

*C++ reference:* `HeatBalanceIntRadExchange.cc` — `CalcInteriorRadExchange()`, ScriptF computation.

### 3.4 Window Thermal Solver

**File:** `crates/ep-windows/src/thermal.rs` (expand existing)

Implement iterative solution for glass layer temperatures:

- **System of equations:** For N glass layers, solve 2N equations (inside + outside face of each layer)
- **Gap gas properties:** Temperature-dependent conductivity, viscosity, density, Prandtl number (polynomial fits for air, argon, krypton, xenon, custom gas mixes)
- **Gap Nusselt number:** Natural convection in sealed vertical gaps (Elsherbiny correlation for aspect ratio effects)
- **Frame/divider:** Conductive heat transfer through frame/divider elements (1D conduction with area fractions)
- **Condensation check:** Flag when inside glass surface temperature drops below dew point

```rust
pub fn solve_window_heat_balance(
    window: &WindowConstruction,
    exterior_conditions: &ExteriorConditions,
    interior_conditions: &InteriorConditions,
) -> WindowThermalResult;

pub struct WindowThermalResult {
    pub glass_temps: Vec<f64>,     // temperature of each glass face
    pub gap_temps: Vec<f64>,       // mean temperature of each gap
    pub heat_flow_in: f64,         // W/m² into zone
    pub solar_absorbed: Vec<f64>,  // solar absorbed per layer
}
```

*C++ reference:* `WindowManager.cc` — `CalcWindowHeatBalance()`, gap gas property functions.

### 3.5 Convection Coefficients

**File:** `crates/ep-envelope/src/convection.rs` (expand existing)

Add remaining correlations needed for the heat balance:

**Interior (currently partial — add):**
- Walton unstable/stable correlations for tilted surfaces
- Ceiling diffuser correlation (Fisher)
- Enhanced model selection algorithm (automatically picks best correlation based on surface tilt, delta-T sign, and zone HVAC type)

**Exterior (currently partial — add):**
- MoWiTT correlation (wind direction dependent)
- Simplified combined coefficient (ASHRAE default: 17.8 W/m²K for exposed, 8.3 for sheltered)
- Surface roughness multiplier table (6 roughness categories)
- Terrain-dependent local wind speed: `V_local = V_met * (δ_met/H_met)^α_met * (H/δ)^α`

*C++ reference:* `ConvectionCoefficients.cc` (6,610 lines) — the ~20 most important correlations.

### Phase 6 Tests (~60 new)

- CTF generation: single concrete layer → known U-value, known number of terms
- CTF generation: multi-layer brick+insulation → cross-coupling sum ≈ U
- CTF generation: air gap → steady-state R-only (no CTF terms needed)
- CTF generation: heavy wall (200mm concrete) → more terms than light wall
- Surface heat balance: steady-state with known outside conditions → T_surface within 0.1°C
- Surface heat balance: step response (suddenly apply solar) → correct transient
- Radiant exchange: 2 parallel surfaces at different temps → known net flux
- Radiant exchange: 6-surface box enclosure → flux sum = 0 (energy conservation)
- Window thermal: single glazing at known conditions → known heat flow
- Window thermal: double glazing with argon → lower U than air
- Convection: all interior correlations at known ΔT → within 10% of published values
- Convection: all exterior correlations at known wind speed → within 10% of published values

**Exit criteria:** A simple rectangular zone (4 walls + roof + floor) with known construction and weather, no HVAC — surface temperatures converge and zone energy balance closes to < 1%.

---

## 4. Phase 7: Solar, Shading & Daylighting ✅

**Status:** Complete (45 new tests, 923 total at completion)

**Goal:** Correctly distribute solar gains through windows onto interior surfaces, accounting for exterior obstructions. Required for any simulation with windows.

**Crates modified:** ep-solar, ep-daylighting, ep-windows

### 4.1 Shadow Casting

**File:** `crates/ep-solar/src/shading.rs` (replace empty stub)

Implement exterior shadow calculations:

- **Polygon clipping:** Sutherland-Hodgman algorithm for clipping one polygon against another
- **Shadow projection:** Project each shading surface onto each receiving surface using sun vector
- **Shading combinations:** Pre-compute which surface pairs can potentially shade each other (azimuth/tilt filter for performance)
- **Sunlit fraction:** Per receiving surface per hour, fraction of area in direct sun
- **Shading surface types:** Detached shading (overhangs, fins, trees), building self-shading
- **Time integration:** Hourly shadow calculations (sub-hourly interpolation optional)

```rust
pub fn compute_sunlit_fractions(
    surfaces: &[Surface],
    shading_surfaces: &[ShadingSurface],
    sun_position: &SunPosition,
) -> Vec<f64>;  // sunlit fraction per surface [0.0, 1.0]

fn clip_polygon(subject: &[Point3D], clip: &[Point3D]) -> Vec<Point3D>;
fn project_shadow(shading: &ShadingSurface, receiving: &Surface, sun: &SunPosition) -> Vec<Point3D>;
```

*C++ reference:* `SolarShading.cc` — `DeterminePolygonOverlap()`, `DetermineShadingCombinations()`.

### 4.2 Solar Distribution

**File:** `crates/ep-solar/src/distribution.rs` (new)

Distribute solar radiation to interior surfaces:

- **Beam through windows:** `Q_beam = I_beam * τ_beam(θ) * A_window * sunlit_fraction`
- **Interior beam distribution:** Track where beam falls on floor/walls (area-weighted initially; later: geometric projection)
- **Diffuse through windows:** `Q_diff = (I_sky_diff + I_ground_diff) * τ_diff_hemi * A_window`
- **Absorbed by opaque exterior:** `Q_abs = α_sol * (I_beam * sunlit + I_diff) * A_surface`
- **Solar to zone air:** Small convective fraction of absorbed solar (typically 0.0)

```rust
pub fn distribute_solar(
    zone: &Zone,
    surfaces: &[Surface],
    windows: &[Window],
    solar: &IncidentSolar,
    sunlit_fractions: &[f64],
) -> SolarDistributionResult;

pub struct SolarDistributionResult {
    pub surface_absorbed: Vec<f64>,  // W absorbed per surface (exterior + interior)
    pub transmitted_beam: Vec<f64>,  // W beam transmitted per window
    pub transmitted_diff: Vec<f64>,  // W diffuse transmitted per window
    pub zone_beam_solar: f64,        // total beam entering zone
    pub zone_diff_solar: f64,        // total diffuse entering zone
}
```

### 4.3 Window Shading Devices

**File:** `crates/ep-windows/src/shading.rs` (new)

Implement interior/exterior window attachments:

- **Interior shade:** Reduce transmitted solar by shade transmittance, add radiant gain from absorbed
- **Interior blind:** Slat geometry (angle, width, spacing), beam profile angle calculation, view factors between adjacent slats
- **Exterior screen:** Beam transmittance from openness factor, diffuse transmittance
- **Shading control:** Schedule-based on/off, solar setpoint (deploy when incident exceeds threshold), glare-based (from daylighting)

### 4.4 Enhanced Daylighting

**Files:** `crates/ep-daylighting/src/` (expand existing modules)

Complete the daylight factor method:

- **Sky discretization:** 145 sky patches covering the hemisphere (Tregenza/CIE scheme)
- **Luminous efficacy:** Direct and diffuse efficacy from solar altitude (Perez model)
- **Interior illuminance:** Window luminance × daylight factor × window area / reference-point distance²
- **Interior reflections:** First-bounce reflectance from floor/walls/ceiling (split-flux method)
- **Reference points:** Multiple reference points per zone, each with independent illuminance calculation
- **Glare calculation:** Daylight Glare Probability from window luminance and background luminance
- **Control response:** Continuous dimming (linear power reduction) or stepped (discrete levels)

### Phase 7 Tests (~50 new)

- Polygon clipping: rectangle vs. triangle → known overlap area
- Sunlit fraction: south wall with overhang at solstice → known fraction
- Solar through window: south-facing at noon equinox → known transmitted W
- Interior distribution: total absorbed by all surfaces = total transmitted (conservation)
- Shading device: interior shade with τ=0.5 reduces transmitted by ~50%
- Blind: slat angle 45° vs 0° → different beam transmittance
- Daylight factor: simple room with known geometry → factor within 10% of hand calculation
- Glare index: large bright window → high DGI; small window → low DGI
- Lighting control: illuminance > setpoint → power reduced to minimum

**Exit criteria:** BESTEST Case 600 (south-facing windows) produces hourly solar gains within 5% of EnergyPlus C++ reference output.

---

## 5. Phase 8: HVAC Integration ✅

**Status:** Complete (79 new tests, 1,002 total at completion)

**Goal:** Implement air-side HVAC so zone loads drive equipment. A single-zone system with fan + cooling coil + heating coil converges to meet zone setpoint.

**Crates modified:** ep-hvac, ep-coils

### 5.1 Air Loop Solver

**File:** `crates/ep-hvac/src/air_loop.rs` (expand)

Implement the air handler simulation sequence:

```
OA Mixer → Heating Coil → Cooling Coil → Fan → Supply Duct → Zone
                                                              ↓
Zone Return → Return Duct → [Relief/Recirculation] → OA Mixer
```

- **Component-by-component simulation:** Each component reads inlet node, modifies state, writes outlet node
- **Supply air temperature tracking:** Enthalpy/humidity through the chain
- **Convergence iteration:** Simulate loop 2-4 times until supply conditions stabilize (residual < tolerance)
- **Outside air mixing:** `T_mix = (1-OA_frac)*T_return + OA_frac*T_outdoor`

```rust
pub struct AirLoopSimulator {
    pub components: Vec<Box<dyn AirLoopComponent>>,
    pub nodes: NodeManager,
    pub max_iterations: usize,
    pub tolerance: f64,
}

pub trait AirLoopComponent {
    fn simulate(&mut self, nodes: &mut NodeManager, first_hvac_iter: bool);
    fn inlet_node(&self) -> NodeIndex;
    fn outlet_node(&self) -> NodeIndex;
}
```

*C++ reference:* `SimAirServingZones.cc` — `SimAirLoops()`, component dispatch.

### 5.2 Zone Equipment Manager

**File:** `crates/ep-hvac/src/zone_equipment.rs` (expand)

- **Zone load calculation:** From zone air balance, compute sensible and latent loads:
  ```
  Q_sensible = TempDepCoef * T_zone + TempIndCoef  [from ep-zone-air]
  Q_latent = similar moisture balance
  ```
- **Equipment sequencing:** Process zone equipment in priority order (user-specified heating/cooling priority)
- **Equipment output:** Each zone equipment returns delivered sensible/latent capacity
- **Return air management:** Collect return air from all zones served by an air loop

### 5.3 Setpoint Managers

**File:** `crates/ep-hvac/src/setpoint.rs` (expand from enum stubs to implementations)

- **Scheduled:** Read supply air temperature from schedule
- **Outside air reset:** Linear interpolation between two (OA temp, supply temp) pairs
- **Warmest/coldest zone:** Track warmest/coldest zone temperature, calculate required supply temperature
- **Single zone reheat:** Dedicated OA temperature tracking for single-zone systems

```rust
pub trait SetpointManager {
    fn calculate_setpoint(&self, state: &SimulationState) -> f64;
    fn node_index(&self) -> NodeIndex;
}
```

### 5.4 Air Terminals (VAV Boxes)

**File:** `crates/ep-hvac/src/terminal.rs` (new)

Implement the 4 most common terminal types:

- **VAV:Reheat:** Variable air flow (min to max), reheat coil modulates to meet zone heating load
  - Below cooling setpoint: minimum flow + reheat
  - Above cooling setpoint: increase flow, no reheat
  - Damper position = max(min_flow, load / capacity)
- **VAV:NoReheat:** Same damper logic, no reheat coil
- **ConstantVolume:Reheat:** Fixed flow, reheat coil modulates
- **ConstantVolume:NoReheat:** Pure pass-through at fixed flow

*C++ reference:* `SingleDuct.cc` (5,859 lines) — 4 core types in ~2K lines.

### 5.5 Controllers

**File:** `crates/ep-hvac/src/controller.rs` (expand from stubs)

- **Water coil controller:** PID or bisection to find water flow rate that achieves target outlet air temperature
- **OA controller:** Economizer logic — increase OA fraction when OA is cooler/drier than return, subject to min ventilation
- **Demand-controlled ventilation:** Adjust minimum OA based on occupancy (CO₂ setpoint or per-person flow)

### 5.6 Enhanced Coil Models

**File:** `crates/ep-coils/src/dx.rs` (expand)

- **Temperature-dependent SHR:** SHR varies with entering conditions (dry-bulb, wet-bulb)
- **Defrost:** Reverse-cycle defrost energy, resistive defrost energy (active below threshold OA temp)
- **Crankcase heater:** Parasitic energy when compressor off and OA temp below threshold
- **Part-load degradation:** PLF curve for cycling losses

**File:** `crates/ep-coils/src/water.rs` (expand)

- **Detailed geometry:** Finned tube dimensions, tube rows, fin spacing → NTU from physical geometry
- **Condensation:** When coil surface temp < dew point, switch to wet coil effectiveness
- **Counter-flow vs. cross-flow:** Different ε-NTU formulas

### Phase 8 Tests (~80 new)

- Air loop: known zone load → correct supply air temperature
- Air loop convergence: iterate 2-3 times → residual < 0.01°C
- VAV terminal: zone above cooling setpoint → flow increases; below → minimum flow + reheat
- Economizer: OA 15°C, return 25°C → 100% OA; OA 35°C → minimum OA
- Setpoint manager: OA reset between 10°C→15°C supply and 30°C→12°C supply
- Controller: bisection finds water flow for target coil outlet within 0.1°C
- DX defrost: OA at -5°C → defrost energy > 0
- Water coil wet: entering air at 30°C/70%RH → condensation, latent capacity > 0

**Exit criteria:** Single-zone VAV system maintains zone at setpoint during a 24-hour design day. Energy consumption within 5% of EnergyPlus reference.

---

## 6. Phase 9: Plant Loop Integration ✅

**Status:** Complete (68 new tests, 1,070 total at completion)

**Goal:** Water-side equipment responds to building loads. A chilled water loop with chiller + pump + cooling coil converges correctly.

**Crates modified:** ep-plant

### 6.1 Plant Loop Solver

**File:** `crates/ep-plant/src/loop_solver.rs` (new)

Implement the half-loop iteration scheme:

```
1. Demand side (coils request flow based on load):
   For each branch on demand side:
     component.simulate() → sets flow request and outlet temp
   Mixer: mass-weighted average of branch outlet temps

2. Supply side (equipment meets demand):
   Pump: deliver requested flow (or design max)
   For each branch on supply side:
     component.simulate() → calculates capacity at given flow
   Mixer: mass-weighted average

3. Convergence check:
   If |T_supply_out - T_supply_out_prev| > tolerance → repeat
   If |flow_request - flow_delivered| > tolerance → repeat
   Max 8 iterations
```

- **FlowLockState progression:** PumpQuery (determine available flow) → Unlocked (components request flow) → Locked (pump fixes flow, components adjust)
- **Equipment operation:** Load range based sequencing (OperationScheme), multiple equipment sets
- **Flow distribution:** Splitter distributes to branches proportionally or via load-optimal scheme

```rust
pub struct PlantLoopSolver {
    pub demand_side: HalfLoop,
    pub supply_side: HalfLoop,
    pub max_iterations: usize,
    pub temp_tolerance: f64,
    pub flow_tolerance: f64,
}

impl PlantLoopSolver {
    pub fn simulate(&mut self, nodes: &mut NodeManager) -> PlantConvergenceResult;
}
```

*C++ reference:* `Plant/LoopSide.cc::Simulate()` + `PlantManager.cc::ManagePlantLoops()`.

### 6.2 Enhanced Chiller Models

**File:** `crates/ep-plant/src/chiller.rs` (expand)

Add additional chiller types:

- **Reformulated EIR:** Uses leaving condenser water temp (not entering) as independent variable — more stable for condenser loop iteration
- **Absorption:** Single-effect absorption with generator heat input, COP ~0.7
- **Constant COP:** Simplest model for early testing and placeholder use
- **Condenser types:** Water-cooled (plant loop), air-cooled (uses OA temp), evaporative
- **Operating limits:** Minimum PLR, false loading, cycling logic

### 6.3 Heat Pump Models

**File:** `crates/ep-plant/src/heat_pump.rs` (new)

- **Water-to-water equation fit:** 5-coefficient equation for capacity and power as functions of source/load temps and flow
- **Plant loop EIR HP:** Same structure as EIR chiller but for heat pump mode (reversible)
- **Source-side coupling:** Connects to ground loop, lake, or condenser loop

### 6.4 Ground Heat Exchangers

**File:** `crates/ep-plant/src/ghx.rs` (new)

- **Vertical borehole:** Cylindrical source model with thermal response factors (g-functions)
  - Temporal superposition of load pulses
  - Borehole resistance calculation from pipe geometry
- **Slinky (horizontal):** Ring source model for horizontal spiral pipes
- **Surface GHX:** Simple UA model for pond/lake surface heat exchange

*C++ reference:* `GroundHeatExchangers/` (3,505 lines)

### 6.5 Thermal Storage

**File:** `crates/ep-plant/src/storage.rs` (new)

- **Stratified tank:** Multi-node 1D model (10-12 nodes), buoyancy-driven mixing, heat loss per node
- **Ice storage (simple):** Charge/discharge capacity curves, ice fraction tracking, discharge priority
- **Chilled water storage:** Mixed (existing) and stratified modes

*C++ reference:* `WaterThermalTanks.cc` (13,199 — stratified sections ~4K) + `IceThermalStorage.cc` (2,266)

### 6.6 Enhanced Pumps & Pipes

Expand existing pump models:

- **Headered pumps:** Multiple parallel pumps (staged on/off based on flow demand)
- **Pipe heat transfer:** UA model for heat loss from outdoor/underground pipes
- **Adiabatic pipe:** Pass-through (no heat loss) for topology completeness

### Phase 9 Tests (~70 new)

- Loop solver convergence: chiller + pump + coil → stable outlet temp within 4 iterations
- Flow distribution: 2-branch splitter with different loads → different flows
- Equipment staging: 2 chillers, half-load → only 1 chiller runs
- Reformulated EIR chiller: COP at off-design conditions matches published data
- Absorption chiller: COP ~0.7, generator heat input > cooling output
- Heat pump: COP varies with source/load temperatures as expected
- Borehole GHX: known soil properties → fluid outlet temp within 0.5°C
- Stratified tank: hot draw from top → cold water at bottom, thermal stratification maintained
- Ice storage: charge → full discharge → energy balance within 1%
- Headered pumps: increasing demand → pumps stage on one at a time

**Exit criteria:** Chilled water plant loop (chiller + tower + pump + cooling coil) responds correctly to varying building loads. Plant energy consumption within 5% of EnergyPlus.

---

## 7. Phase 10: Simulation Driver & Sizing

**Goal:** Wire everything together. An IDF file runs end-to-end through the full simulation loop and produces correct output files. BESTEST Case 600 passes.

**Crates modified:** ep-sim, ep-io, ep-zone-air, ep-output, ep-core

### 7.1 Simulation Loop

**File:** `crates/ep-sim/src/lib.rs` (implement `SimulationDriver::run()`)

```rust
pub fn run(&mut self) -> SimulationResult {
    self.initialize();                    // read input, setup constructions, CTFs, etc.
    self.size();                          // run sizing if needed

    for env in self.environment_queue.drain(..) {
        self.begin_environment(&env);
        self.run_warmup(&env);            // repeat days until convergence

        for day in env.days() {
            self.begin_day(day);
            for hour in 0..24 {
                self.begin_hour(hour);
                self.update_weather(hour);
                self.update_solar_position(hour);

                for timestep in 0..self.timesteps_per_hour {
                    self.begin_timestep(timestep);
                    self.calc_heat_balance();        // surfaces, solar, internal gains
                    self.predict_zone_temps();        // predictor step

                    for hvac_iter in 0..self.max_hvac_iterations {
                        self.simulate_zone_equipment();
                        self.simulate_air_loops();
                        self.simulate_plant_loops();
                        if self.hvac_converged() { break; }
                    }

                    self.correct_zone_temps();        // corrector step
                    self.update_output_variables();
                    self.end_timestep();
                }
                self.end_hour();
            }
            self.end_day();
        }
        self.end_environment();
    }
    self.finalize()
}
```

*C++ reference:* `SimulationManager.cc` — `ManageSimulation()`, nested loop with flag management.

### 7.2 Zone Predictor-Corrector

**File:** `crates/ep-zone-air/src/` (expand)

Complete the full predictor-corrector integration:

- **Predictor:** Using previous timestep's zone temp and estimated loads, predict new zone temp
- **HVAC simulation:** Run air loops and plant at predicted zone conditions
- **Corrector:** With actual HVAC output, solve for final zone temp using one of three methods:
  - Third-order backward difference (most accurate, needs 3 history values)
  - Euler forward (simplest, first timestep)
  - Analytical (exact exponential solution for constant coefficients)
- **History management:** Push/pop temperature history arrays for dual-timestep switching
- **Moisture parallel:** Same predictor-corrector structure for humidity ratio

### 7.3 IDF Schema Expansion

**File:** `crates/ep-io/src/schema.rs` (expand from ~15 to ~65 object definitions)

Add the ~50 most common IDF objects needed to run standard files:

**Building & Site:**
- `Building`, `GlobalGeometryRules`, `Site:Location`, `Site:GroundTemperature:BuildingSurface`

**Geometry:**
- `Zone`, `BuildingSurface:Detailed`, `FenestrationSurface:Detailed`
- `Shading:Site:Detailed`, `Shading:Building:Detailed`, `Shading:Zone:Detailed`

**Materials & Constructions:**
- `Material`, `Material:NoMass`, `Material:AirGap`
- `WindowMaterial:Glazing`, `WindowMaterial:Gas`, `WindowMaterial:SimpleGlazingSystem`
- `Construction`

**Schedules:**
- `ScheduleTypeLimits`, `Schedule:Compact`, `Schedule:Constant`

**Internal Gains:**
- `People`, `Lights`, `ElectricEquipment`, `OtherEquipment`
- `ZoneInfiltration:DesignFlowRate`, `ZoneVentilation:DesignFlowRate`

**Sizing:**
- `Sizing:Zone`, `Sizing:System`, `Sizing:Plant`, `DesignSpecification:OutdoorAir`

**HVAC:**
- `AirLoopHVAC`, `AirLoopHVAC:ControllerList`, `AirLoopHVAC:OutdoorAirSystem`
- `Fan:ConstantVolume`, `Fan:VariableVolume`, `Fan:OnOff`
- `Coil:Cooling:DX:SingleSpeed`, `Coil:Heating:Fuel`, `Coil:Cooling:Water`, `Coil:Heating:Water`
- `AirTerminal:SingleDuct:VAV:Reheat`, `AirTerminal:SingleDuct:ConstantVolume:NoReheat`

**Plant:**
- `PlantLoop`, `CondenserLoop`
- `Boiler:HotWater`, `Chiller:Electric:EIR`, `CoolingTower:SingleSpeed`
- `Pump:VariableSpeed`, `Pump:ConstantSpeed`, `Pipe:Adiabatic`

**Setpoint Managers:**
- `SetpointManager:Scheduled`, `SetpointManager:OutdoorAirReset`, `SetpointManager:MixedAir`

**Output:**
- `Output:Variable`, `Output:Meter`, `OutputControl:Table:Style`

**Simulation Control:**
- `SimulationControl`, `Timestep`, `RunPeriod`, `SizingPeriod:DesignDay`

### 7.4 Output Integration

**File:** `crates/ep-output/src/` (expand)

- Wire `OutputManager` into the simulation loop: register variables during init, accumulate each timestep, report at hour/day/month/run-period boundaries
- Complete ESO writer: header format, data dictionary, timestep data lines
- Complete MTR writer: meter values at requested frequency
- SQL writer: time series table, tabular data table
- Key predefined reports: Zone Component Load Summary, HVAC Sizing Summary

### 7.5 Sizing

**File:** `crates/ep-sim/src/sizing.rs` (expand from stubs)

- **Zone sizing:** Run design day simulation → track peak heating/cooling loads → compute design air flow rates
- **System sizing:** Aggregate zone loads → AHU capacity, total air flow, OA flow
- **Plant sizing:** Aggregate coil loads → chiller/boiler capacity, water flow rates
- **Autosize resolution:** Replace `autosize` sentinel values in equipment with computed sizes

### Phase 10 Tests (~50 new)

- Full loop: 1-day design day, no HVAC, simple zone → hourly zone temps match reference
- Warmup: standard construction → converges within 25 days
- Predictor-corrector: known system output → zone temp within 0.01°C of analytical solution
- IDF parsing: BESTEST Case 600 IDF → all objects parsed without error
- Output: ESO file contains correct zone temperature at each timestep
- Sizing: zone sizing for cooling design day → peak load within 5% of reference
- End-to-end: BESTEST Case 600 → annual heating/cooling within ASHRAE 140 acceptance range

**Exit criteria:** BESTEST Case 600 runs end-to-end from IDF → ESO/MTR/SQL with correct results. Annual heating and cooling loads fall within ASHRAE Standard 140 acceptance ranges.

---

## 8. Phase 11: Equipment Breadth

**Goal:** Support the 20 most common example file configurations. Target: 80% of real-world IDF files use only implemented equipment types.

### 11.1 Unitary Systems & Furnaces

Add to `ep-hvac`:
- `AirLoopHVAC:UnitarySystem` — central dispatch for packaged equipment
- Heat pump air-to-air (DX heating + DX cooling + electric supplemental)
- Furnace (gas burner + DX cooling)
- `ZoneHVAC:PackagedTerminalAirConditioner` (PTAC)
- `ZoneHVAC:PackagedTerminalHeatPump` (PTHP)

*C++ reference:* `UnitarySystem.cc` (18K) + `Furnaces.cc` (11K) — core logic ~8K lines

### 11.2 Zone Equipment Expansion

Add to `ep-hvac`:
- `ZoneHVAC:FourPipeFanCoil` — cycling/multi-speed fan with water coils
- `ZoneHVAC:Baseboard:Convective:Water` and `:Electric`
- `ZoneHVAC:Baseboard:RadiantConvective:Water`
- `ZoneHVAC:UnitHeater`, `ZoneHVAC:UnitVentilator`
- `ZoneHVAC:WindowAirConditioner`
- `ZoneHVAC:LowTemperatureRadiant:Electric` and `:Hydronic`

### 11.3 VRF Systems

Add new module to `ep-hvac`:
- `AirConditioner:VariableRefrigerantFlow` — outdoor unit with capacity modulation curves
- `ZoneHVAC:TerminalUnit:VariableRefrigerantFlow` — indoor fan-coil units
- Heat recovery mode (simultaneous heating + cooling)
- Piping correction factors for refrigerant line length

*C++ reference:* `HVACVariableRefrigerantFlow.cc` (15.5K lines)

### 11.4 Air-to-Air Heat Recovery

Add to `ep-hvac`:
- `HeatExchanger:AirToAir:SensibleAndLatent` — effectiveness model (sensible + latent)
- `HeatExchanger:AirToAir:FlatPlate` — NTU method
- Frost control (supply bypass, exhaust recirculation)
- Economizer bypass interaction

### 11.5 Expanded Plant Equipment

Add to `ep-plant`:
- Steam boiler
- `DistrictCooling`, `DistrictHeating:Water` (simplified source/sink)
- `HeatExchanger:FluidToFluid` (two-loop coupling)
- Evaporative coolers (direct CelDekPad, indirect WetCoil)
- `CoolingTower:TwoSpeed`, `CoolingTower:VariableSpeed:Merkel`
- `FluidCooler:SingleSpeed`, `FluidCooler:TwoSpeed`

### 11.6 Multi-Speed & Two-Stage DX

Expand `ep-coils`:
- `Coil:Cooling:DX:TwoSpeed` — high/low speed interpolation
- `Coil:Cooling:DX:MultiSpeed` — speed-level interpolation with cycling between speeds
- `Coil:Heating:DX:MultiSpeed`
- `Coil:Cooling:DX:TwoStageWithHumidityControlMode`
- `Coil:Cooling:DX:CurveFit:Performance` (new-generation model)

### 11.7 Additional Air Terminals

Add to `ep-hvac`:
- `AirTerminal:DualDuct:ConstantVolume`, `:VAV`
- `AirTerminal:SingleDuct:SeriesPIU:Reheat` (powered induction)
- `AirTerminal:SingleDuct:VAV:Reheat:VariableSpeedFan`

### Phase 11 Tests (~120 new)

**Exit criteria:** The 20 most common EnergyPlus example file equipment configurations can be simulated.

---

## 9. Phase 12: Advanced Features & Parity

**Goal:** Full professional-grade simulation capability. BESTEST 600/900 series complete. 50+ example files run correctly.

### 12.1 Thermal Comfort (new crate: ep-comfort)

- Fanger PMV/PPD (ISO 7730)
- ASHRAE 55 adaptive comfort
- CEN 15251 adaptive comfort
- Pierce/KSU two-node thermoregulation (Runge-Kutta ODE)
- Simple ASHRAE 55 summer/winter check
- Ankle draft and ceiling fan cooling effect

*~40 tests, C++ reference:* `ThermalComfort.cc` (3.2K lines)

### 12.2 Room Air Models

- Underfloor air distribution (UFAD) with 3-zone stratification
- Cross-ventilation two-node model
- Displacement ventilation
- User-defined temperature patterns

*~30 tests, C++ reference:* 5 files (~5K lines total)

### 12.3 CondFD Conduction (Finite Difference)

Add to ep-envelope:
- Crank-Nicolson semi-implicit scheme
- Fully implicit first-order scheme
- Phase change material support (enthalpy method with latent heat)
- Variable conductivity materials
- Moisture vapor transport (partial pressure driven)
- Adaptive node spacing (fine at boundaries, coarse interior)

*~25 tests, C++ reference:* `HeatBalFiniteDiffManager.cc` (2.8K lines)

### 12.4 Advanced Fenestration

Expand ep-windows:
- Equivalent layer (EQL) model for complex shading assemblies
- BSDF (Bidirectional Scattering Distribution Function) for directional glazing
- Switchable glazing (electrochromic, thermochromic) with switching factor
- Complex fenestration states with multi-state control logic

*~30 tests, C++ reference:* `WindowEquivalentLayer.cc` (8.1K) + `WindowComplexManager.cc` (3.5K)

### 12.5 Enhanced Ground Heat Transfer

Expand ep-ground:
- Kiva-compatible foundation model (2D cross-section FD solver)
- Slab-on-grade with perimeter insulation
- Basement below-grade wall coupling
- Soil moisture effects on thermal conductivity

*~20 tests, C++ reference:* `HeatBalanceKivaManager.cc` (1.3K) + kiva library

### 12.6 Full Airflow Network

Expand ep-airflow:
- Duct distribution system (friction, leakage, heat transfer)
- Wind pressure coefficients (terrain + building shape)
- Occupant-controlled ventilation (comfort triggers, opening probability)
- Hybrid mode (distribution when HVAC on, multizone when off)
- Additional components: coil ΔP, relief damper, zone exhaust fan

*~30 tests, C++ reference:* `AirflowNetwork/` (22K lines total)

### 12.7 Complete Output System

Expand ep-output:
- All predefined tabular reports (LEED, ASHRAE 90.1, equipment summaries, zone loads)
- Monthly/annual aggregation with time-of-peak tracking
- ResultsFramework structured output
- Full ESO/MTR/CSV/SQL format compliance

*~40 tests, C++ reference:* `OutputReportTabular.cc` (19.7K lines)

### 12.8 Full IDF Schema

Expand ep-io to cover remaining ~850 object types with validation.

### Phase 12 Tests (~200+ new)

**Exit criteria:** BESTEST 600/610/620/630/900/910/920/930 all pass. 50+ standard example files run with results within 5% of C++ EnergyPlus.

---

## 10. Crate-Level Summary

| Crate | Current Tests | Phase(s) | Target Tests |
|-------|:------------:|----------|:------------:|
| ep-units | 16 | — | 16 |
| ep-core | 6 | 10 | 20 |
| ep-psychrometrics | 15 | — | 15 |
| ep-fluids | 11 | — | 11 |
| ep-curves | 12 | — | 12 |
| ep-weather | 13 | — | 35 |
| ep-schedule | 40 | — | 40 |
| ep-materials | 10 | — | 25 |
| ep-io | 48 | 10, 12 | 120 |
| ep-surfaces | 20 | — | 40 |
| ep-envelope | 77 | 12 | 100 |
| ep-windows | 37 | 12 | 60 |
| ep-solar | 41 | — | 50 |
| ep-ground | 18 | 12 | 35 |
| ep-zone-air | 14 | 10, 12 | 60 |
| ep-internal-gains | 32 | — | 35 |
| ep-airflow | 18 | 12 | 50 |
| ep-daylighting | 61 | — | 70 |
| ep-nodes | 14 | — | 20 |
| ep-fans | 11 | 11 | 25 |
| ep-coils | 53 | 11 | 60 |
| ep-plant | 134 | 11 | 150 |
| ep-hvac | 101 | 11 | 150 |
| ep-ems | 42 | — | 50 |
| ep-fmi | 7 | — | 10 |
| ep-generation | 34 | — | 40 |
| ep-demand | 21 | — | 25 |
| ep-sim | 48 | 10 | 100 |
| ep-validation | 21 | 10 | 40 |
| ep-refrigeration | 25 | — | 30 |
| ep-water | 21 | — | 30 |
| ep-api | 6 | — | 10 |
| ep-output | 43 | 10, 12 | 80 |
| **ep-comfort** (new) | 0 | 12 | 40 |
| **Total** | **1,070** | | **~1,700** |

---

## 11. Priority & Dependencies

```
Phase 6 (Heat Balance) ✅ ───────────────────┐
    │                                         │
    ▼                                         │
Phase 7 (Solar/Shading) ✅                    │
    │                                         │
    ▼                                         ▼
Phase 10 (Simulation Driver) ◄── Phase 8 (HVAC) ✅ + Phase 9 (Plant) ✅
    │                              [Phase 10 is next]
    ▼
Phase 11 (Equipment Breadth)
    │
    ▼
Phase 12 (Advanced Features & Parity)
```

- **Phase 6 → 7:** Complete
- **Phase 8:** Complete (air-side HVAC integration)
- **Phase 9:** Complete (plant loop solver, enhanced chillers, heat pumps, GHX, thermal storage)
- **Phase 10:** Next priority — depends on Phase 9 (now complete)
- **Phase 11 ↔ 12:** Can overlap, done incrementally

### Critical Milestones

| Milestone | Phase | Validation | Status |
|-----------|-------|------------|--------|
| Surface temps converge | 6 | Zone energy balance < 1% | ✅ Done |
| Solar gains correct | 7 | BESTEST 600 solar within 5% | ✅ Done |
| Single-zone HVAC works | 8 | Zone temp at setpoint under design day | ✅ Done |
| Plant loop converges | 9 | Chiller/tower energy within 5% | ✅ Done |
| **First IDF runs end-to-end** | **10** | **BESTEST Case 600 passes** | Pending |
| 20 example configs work | 11 | Multi-zone VAV systems | Pending |
| Full BESTEST suite | 12 | 600/900 series all pass | Pending |

---

## 12. Verification Strategy

### 12.1 Test Hierarchy

1. **Unit tests** (per function): Analytical solutions, published correlation values
2. **Component tests** (per equipment): Known inlet conditions → known outlet, known energy
3. **Integration tests** (per loop): Air loop or plant loop converges to known steady state
4. **System tests** (end-to-end): Full IDF → compare output against C++ EnergyPlus reference
5. **BESTEST** (gold standard): ASHRAE Standard 140 acceptance ranges

### 12.2 Reference IDF Files

Progressive validation targets:

| Phase | IDF File | What It Tests |
|-------|----------|---------------|
| 6 | `1ZoneUncontrolled.idf` | Heat balance only, no HVAC |
| 7 | Custom: south window test | Solar gain through window |
| 8 | `1ZoneEvapCooler.idf` | Simple single-zone HVAC |
| 9 | Custom: chiller loop test | Plant loop convergence |
| 10 | BESTEST Case 600 | Full envelope + simple HVAC |
| 11 | `5ZoneAirCooled.idf` | Multi-zone VAV system |
| 12 | BESTEST 600–930 series | Full validation suite |

### 12.3 Regression Testing

- Store C++ EnergyPlus reference outputs (ESO/MTR) for each target IDF
- Automated comparison: Rust output vs. reference at each timestep
- Tolerance: 0.1°C for temperatures, 1% for energy, 5% for peak loads
- CI pipeline: `cargo test --workspace` + regression comparison

### 12.4 Numerical Considerations

- CTF coefficients: converge to 1e-13 ratio cutoff
- Surface temperature iteration: converge to 0.001°C
- HVAC loop: converge to 0.01°C supply air temp, 0.001 kg/s flow
- Plant loop: converge to 0.1°C supply water temp
- Float reproducibility: use deterministic summation order for cross-platform consistency
