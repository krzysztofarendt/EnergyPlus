# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

EnergyPlus is a whole building energy simulation program written in C++17. It models energy consumption and water use in buildings. The codebase has 200+ modules, Fortran utilities, a C/Python API layer, and uses CMake with Ninja as the build system.

## Build Commands

```bash
# Configure (from repo root)
mkdir build && cd build
cmake -G Ninja -DCMAKE_BUILD_TYPE=Release -DBUILD_TESTING=ON ..

# Build everything
ninja

# Build only the main executable
ninja energyplus

# Build only the test executable
ninja energyplus_tests
```

Key CMake options: `-DLINK_WITH_PYTHON=ON`, `-DPYTHON_CLI=ON`, `-DBUILD_FORTRAN=ON`, `-DENABLE_PCH=ON` (default on).

## Testing

```bash
# Run all tests
ctest -j $(nproc)

# Run only unit tests (skip integration)
ctest -E "integration.*" -j $(nproc)

# Run a specific test by name
ctest -R "AirTerminalSingleDuct" -VV

# Run tests matching a pattern
ctest -R "Boiler.*GetInput" -j 1

# Run test executable directly with GoogleTest filter
./Products/energyplus_tests --gtest_filter="EnergyPlusFixture.TestName"
./Products/energyplus_tests --gtest_filter="*Boiler*"

# Rerun only failed tests
ctest --rerun-failed -VV
```

## Linting & Formatting

- **C++ formatting**: clang-format-19 with config at `src/.clang-format` (4-space indent, 150 column limit, Allman braces for functions/classes/structs, K&R for control flow)
- **Python formatting**: black + isort
- **Pre-commit hooks**: `pre-commit run --all-files` runs clang-format, black, isort, and custom checks (constexpr usage, license headers, C-style comment detection, enum validation, format string checks)
- No C-style comments (`/* */`) in src/EnergyPlus or tst/ - use `//` only
- No tabs in IDF/IMF files

## Architecture

### Central State Pattern

All simulation state lives in `EnergyPlusData` (`src/EnergyPlus/Data/EnergyPlusData.hh`), a struct with 150+ `unique_ptr` members for module-specific data. Every function receives `EnergyPlusData &state` as its first parameter. Access module data via `state.dataBoilers->...`, `state.dataPlnt->PlantLoop`, `state.dataLoopNodes->Node`, etc.

### Module Structure (GetInput/Init/Calc/Update Pattern)

Equipment modules follow a consistent pattern (e.g., `Boilers.hh/cc`):

1. **Data struct** inherits `BaseGlobalStruct` with `clear_state()` override (uses placement new: `new (this) BoilersData()`)
2. **Component struct** (e.g., `BoilerSpecs`) inherits `PlantComponent` for plant equipment
3. **`factory()` static method** with one-time gating flag: checks `state.dataBoilers->getBoilerInputFlag`, calls `GetBoilerInput()` once
4. **`simulate()`** virtual method calls Init → Calc → Update sequence
5. **`GetInput()`** parses IDF via InputProcessor, populates component vector

### Simulation Loop (SimulationManager.cc)

Nested loop: Environment → Day → Hour → TimeStep. Inside the timestep: `ManageWeather()` → `ManageHeatBalance()` → `ManageHVAC()`. Key flags: `BeginSimFlag`, `BeginEnvrnFlag`, `BeginDayFlag`, `BeginHourFlag`, `WarmupFlag`.

### Input Processing

InputProcessor (`src/EnergyPlus/InputProcessing/`) converts IDF to JSON internally using nlohmann::json. Input schema validation is built in. Access via `state.dataInputProcessing->inputProcessor->...`.

### API Layer

C API in `src/EnergyPlus/api/` exposes C++ functions; Python bindings built on top. Entry point: `EnergyPlusPgm()` in `api/EnergyPlusPgm.hh`.

### Key Directories

- `src/EnergyPlus/` - Core simulation engine (200+ modules)
- `src/EnergyPlus/Data/` - State data structs (one per module)
- `src/EnergyPlus/InputProcessing/` - IDF/JSON input parsing
- `src/EnergyPlus/api/` - C and Python API
- `tst/EnergyPlus/unit/` - GoogleTest unit tests (280+ test files)
- `tst/EnergyPlus/unit/Fixtures/` - Test fixtures (`EnergyPlusFixture` is the main one)
- `testfiles/` - IDF integration test files
- `third_party/` - Dependencies (ObjexxFCL, nlohmann/json, fmt, btwxt, re2, SQLite, kiva, penumbra)

## Writing Unit Tests

Tests use `EnergyPlusFixture` which provides a fresh `state` pointer per test:

```cpp
TEST_F(EnergyPlusFixture, MyModule_MyTestName)
{
    std::string const idf_objects = delimited_string({
        "  Object:Type,",
        "    Field1,",
        "    Field2;",
    });
    ASSERT_TRUE(process_idf(idf_objects));

    // Set up state, call functions, assert results
    EXPECT_EQ(expected, state->dataMyModule->someValue);
}
```

Key fixture methods: `process_idf()`, `delimited_string()`, `compare_err_stream()`, `compare_eso_stream()`, `compare_eio_stream()`, `has_err_output()`. Custom matchers: `EXPECT_ENUM_EQ`, `EXPECT_ENUM_NE`.

New test files must be added to `tst/EnergyPlus/unit/CMakeLists.txt` in the `test_src` list.

## Coding Conventions

- Namespace pattern: `namespace EnergyPlus::ModuleName { ... }`
- Use `Real64` (alias for `double`) for floating-point values
- Enums use `enum class` with a terminal `Num` sentinel value
- Arrays use ObjexxFCL `Array1D`/`Array2D` (1-indexed) or `std::vector` (0-indexed)
- Compiler warnings are errors (`-Werror`)
