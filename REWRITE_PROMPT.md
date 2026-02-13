# Claude Code Prompt: EnergyPlus → Rust Modular Rewrite Plan

## Context

You are a senior systems architect with deep expertise in building energy simulation, numerical methods, Rust systems programming, and large-scale software migration. Your task is to produce a **comprehensive, actionable, modular plan** for rewriting the EnergyPlus building energy simulation engine in Rust.

## Background

EnergyPlus is the U.S. Department of Energy's flagship whole-building energy simulation engine. Key facts:

- **Codebase**: ~500K–700K lines of C++ with legacy Fortran heritage (DOE-2 / BLAST lineage)
- **Repository**: https://github.com/NREL/EnergyPlus (open source, BSD license)
- **Architecture**: Monolithic with a central simulation manager, global state, sequential zone/system/plant solver loops, and a massive IDD/IDF schema (~900 object types)
- **Core physics**: Zone heat balance (CTF/CondFD), HVAC component models, fenestration (Window 7), airflow networks, daylighting, plant loops, demand-side management, and co-simulation interfaces (FMI)
- **Validation**: ASHRAE Standard 140 (BESTEST), extensive empirical and comparative test suites, ~700 example files
- **Engineering Reference**: https://energyplus.net/documentation — the canonical specification of all algorithms

This is NOT a naive line-by-line port. The goal is a **modern Rust reimagining** of EnergyPlus's physics and solver capabilities with clean architecture, taking advantage of Rust's type system, memory safety, concurrency, and ecosystem.

## Your Deliverable

Produce a detailed plan document (`REWRITE_PLAN.md`) structured as follows:

---

### 1. Architecture Overview

Design a high-level architecture for the Rust-based engine. Address:

- **Crate structure**: Define a workspace of Rust crates (libraries) with clear boundaries. Suggest a mono-repo layout with crates such as `ep-core`, `ep-envelope`, `ep-hvac`, `ep-plant`, `ep-airflow`, `ep-solar`, `ep-daylighting`, `ep-schedule`, `ep-io`, `ep-weather`, `ep-solver`, `ep-fmi`, etc.
- **Type safety**: Design a units/quantities type system using newtypes or a dimensional analysis crate (e.g., `uom`) to encode physical units (`Watts`, `Kelvin`, `kg_per_s`, `Pascal`, etc.) at compile time.
- **State management**: Replace EnergyPlus's global mutable state with an explicit simulation state struct, passed through the solver pipeline. Discuss ownership, borrowing patterns, and how to avoid the "god object" anti-pattern.
- **ECS-inspired design**: Evaluate whether an Entity-Component-System pattern (or a simpler component-based design) is suitable for modeling zones, surfaces, HVAC components, and plant equipment as composable entities.
- **Solver orchestration**: Design the main simulation loop (sequential or parallel), including the predictor-corrector scheme, the zone/system/plant iteration hierarchy, and timestep management.
- **Concurrency model**: Identify opportunities for parallelism (independent zone solves, surface-by-surface solar calculations, parametric runs) and propose a concurrency strategy using Rayon, async, or explicit threading.
- **Error handling**: Define an error handling strategy using `Result<T, E>` with a custom error enum hierarchy, replacing EnergyPlus's `ShowFatalError` / `ShowWarningError` pattern.
- **Plugin / extension model**: Design a trait-based component interface so users can implement custom HVAC components, EMS-like scripting, or co-simulation adapters without modifying the core engine.

### 2. Module Decomposition

For EACH of the following EnergyPlus subsystems, produce a dedicated section containing:

- **Scope**: What EnergyPlus source files / modules does this cover?
- **Key algorithms**: List the core mathematical/physical models with references to the Engineering Reference sections
- **Rust crate name** and public API sketch (key structs, traits, functions)
- **Dependencies**: Which other crates does this depend on?
- **Complexity estimate**: Lines of Rust (rough), effort in person-months, difficulty (Low / Medium / High / Very High)
- **Migration strategy**: Can this be ported incrementally? Can it be validated independently against EnergyPlus outputs?
- **Risks and challenges**: What makes this hard? Legacy assumptions, implicit coupling, numerical stability concerns, etc.

#### Subsystems to cover:

1. **Weather Processing** — TMY/EPW file parsing, solar position, sky models, ground temperatures
2. **Schedules & Internal Gains** — Schedule types, people, lights, equipment, infiltration schedules
3. **Surface Heat Balance** — CTF (Conduction Transfer Functions), CondFD (Conduction Finite Difference), combined convection/radiation
4. **Fenestration & Window Models** — Multi-layer glazing optics, shading devices, Window 7 integration, solar distribution
5. **Solar & Shading Calculations** — Sun position, shadow casting, diffuse sky models, solar gains on surfaces
6. **Daylighting** — DElight / split-flux / radiosity methods, glare, daylight controls
7. **Zone Air Heat Balance** — Predictor-corrector, zone mixing, cross-mixing, ventilation
8. **HVAC Air-Side Systems** — AHU components (coils, fans, humidifiers, heat exchangers, DX systems), unitary equipment, VAV/CAV
9. **HVAC Water-Side / Plant** — Boilers, chillers, cooling towers, heat pumps, plant loop solvers, condenser loops
10. **Airflow Network** — Multi-zone pressure-based airflow, natural ventilation, duct leakage
11. **Refrigeration Systems** — Supermarket refrigeration, walk-in coolers, secondary loops
12. **Water Systems** — Domestic hot water, water heaters, solar thermal collectors
13. **On-Site Generation** — PV, wind, fuel cells, micro-CHP, battery storage, generators
14. **Demand-Side Management / EMS** — Energy Management System, demand limiting, load management, Erl scripting
15. **Output & Reporting** — Output variables, meters, tabular reports, SQL output, ESO/MTR files
16. **Input Processing (IDD/IDF)** — Schema parsing, object validation, input translation (consider migrating to a modern format like TOML, JSON Schema, or a custom DSL)
17. **Co-Simulation / FMI** — Functional Mock-up Interface, ExternalInterface, BCVTB coupling
18. **Ground Heat Transfer** — Slab, basement, Kiva foundation models
19. **Simulation Manager / Orchestrator** — Warmup, sizing, run periods, design days, timestep control, convergence management

### 3. Phased Roadmap

Propose a phased implementation plan:

- **Phase 0 — Foundation** (months 1–3): Core types, units, weather, schedules, I/O framework
- **Phase 1 — Envelope** (months 4–8): Surface heat balance, fenestration, solar/shading, ground heat transfer
- **Phase 2 — Zone** (months 9–12): Zone air heat balance, airflow network, daylighting
- **Phase 3 — HVAC** (months 13–20): Air-side systems, plant loops, refrigeration
- **Phase 4 — Advanced** (months 21–26): EMS, FMI, on-site generation, demand management
- **Phase 5 — Parity & Validation** (months 27–30): Full BESTEST compliance, example file regression testing, performance benchmarking

For each phase, specify:
- Entry/exit criteria
- Minimum viable validation targets
- Which EnergyPlus example files should pass at the end of the phase
- Team size and skill requirements
- Key technical risks

### 4. Validation & Testing Strategy

- How to set up regression testing against EnergyPlus reference outputs
- Property-based testing strategy for numerical kernels (e.g., using `proptest`)
- ASHRAE Standard 140 (BESTEST) compliance roadmap
- Continuous integration pipeline design
- Fuzzing strategy for input parsing
- Numerical tolerance and floating-point reproducibility considerations

### 5. Build & Tooling

- Cargo workspace configuration
- CI/CD pipeline (GitHub Actions)
- Documentation strategy (`rustdoc`, mdBook for user guide)
- Benchmarking framework (`criterion`)
- FFI strategy for calling into existing C/Fortran libraries during transition (e.g., Window 7, KIVA)
- Python bindings via PyO3 for scripting and existing EnergyPlus ecosystem integration
- WebAssembly compilation target for browser-based simulations

### 6. Comparison with Existing Efforts

Briefly compare this approach with:
- PassiveLogic's physics engine (from-scratch, real-time digital twins)
- Modelica-based tools (OpenModelica, Modelon)
- OpenStudio SDK's relationship to EnergyPlus
- Spawn of EnergyPlus (LBNL's Modelica-based successor project)
- Why a Rust rewrite offers advantages over these alternatives

### 7. Open Questions & Decision Points

List key architectural decisions that need to be resolved early, such as:
- Fixed vs. variable timestep solvers
- IDF backward compatibility vs. clean-break new input format
- Implicit vs. explicit solver formulations
- Degree of parallelism vs. deterministic reproducibility
- Whether to embed a scripting language (Lua, Rhai, Python) for EMS replacement
- Licensing model for the Rust engine

---

## Constraints & Guidelines

- **Be specific**: Include actual Rust code snippets for key type definitions, trait interfaces, and API sketches. Don't just describe — show.
- **Be honest about complexity**: EnergyPlus is one of the most complex building simulation engines ever written. Don't underestimate the effort.
- **Reference the EnergyPlus source**: When discussing modules, reference the actual EnergyPlus C++ source file names (e.g., `HeatBalanceSurfaceManager.cc`, `PlantLoopSolver.cc`) so the plan can be cross-referenced.
- **Prioritize correctness over performance**: The Rust engine should first be correct, then fast. But identify where Rust's zero-cost abstractions provide "free" performance wins.
- **Consider the ecosystem**: The plan should account for integration with OpenStudio, Ladybug Tools, and other downstream consumers of EnergyPlus.
- **Target audience**: The plan will be read by a team of engineers with backgrounds in building physics, numerical methods, and systems programming. Assume familiarity with both EnergyPlus internals and Rust.

## Output

Write the complete plan to `REWRITE_PLAN.md` in the current working directory. Use clear Markdown formatting with code blocks for Rust snippets. The document should be comprehensive (target 3,000–5,000 lines) and serve as a genuine project charter for a multi-year engineering effort.

Before writing, clone or browse the EnergyPlus repository to understand the current codebase structure, identify key source files, and verify module boundaries. Use `gh repo clone NREL/EnergyPlus` or browse the source tree on GitHub.
