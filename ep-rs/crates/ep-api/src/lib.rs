//! C-compatible API for the EnergyPlus Rust simulation engine.
//!
//! Provides opaque handle-based access for creating simulations,
//! setting parameters, registering callbacks, running, and querying results.
//! Mirrors the EnergyPlus C API (api/runtime.h, api/datatransfer.h).

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int};
use std::ptr;

use ep_core::state::SimulationState;
use ep_sim::{SimulationCallback, SimulationConfig, SimulationDriver, SimulationResult};

/// Opaque simulation handle.
pub struct SimHandle {
    state: SimulationState,
    driver: SimulationDriver,
    result: Option<SimulationResult>,
    /// Named variables for data exchange.
    variables: Vec<Variable>,
    /// External callback function pointers.
    callbacks: ExternalCallbacks,
    /// Last error message.
    last_error: Option<CString>,
}

/// A named variable for data exchange with external programs.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Variable {
    name: String,
    key: String,
    value: f64,
    writable: bool,
}

/// External callback function pointers (C function pointers).
#[derive(Default)]
struct ExternalCallbacks {
    begin_environment: Option<extern "C" fn(*mut SimHandle)>,
    after_timestep: Option<extern "C" fn(*mut SimHandle)>,
    end_environment: Option<extern "C" fn(*mut SimHandle)>,
}

/// Callback adapter that invokes external C function pointers.
struct ExternalCallbackAdapter {
    handle_ptr: *mut SimHandle,
    begin_environment: Option<extern "C" fn(*mut SimHandle)>,
    after_timestep: Option<extern "C" fn(*mut SimHandle)>,
    end_environment: Option<extern "C" fn(*mut SimHandle)>,
}

impl SimulationCallback for ExternalCallbackAdapter {
    fn begin_environment(&mut self, _state: &mut SimulationState) {
        if let Some(cb) = self.begin_environment {
            cb(self.handle_ptr);
        }
    }

    fn end_timestep(&mut self, _state: &mut SimulationState) {
        if let Some(cb) = self.after_timestep {
            cb(self.handle_ptr);
        }
    }

    fn end_environment(&mut self, _state: &mut SimulationState) {
        if let Some(cb) = self.end_environment {
            cb(self.handle_ptr);
        }
    }
}

// --- Handle lifecycle ---

/// Create a new simulation handle with default configuration.
///
/// # Safety
/// Returns a heap-allocated handle. Must be freed with `sim_free`.
#[no_mangle]
pub extern "C" fn sim_new() -> *mut SimHandle {
    let config = SimulationConfig::default();
    let state = SimulationState::new(config.timesteps_per_hour);
    let driver = SimulationDriver::new(config);
    let handle = Box::new(SimHandle {
        state,
        driver,
        result: None,
        variables: Vec::new(),
        callbacks: ExternalCallbacks::default(),
        last_error: None,
    });
    Box::into_raw(handle)
}

/// Free a simulation handle.
///
/// # Safety
/// `handle` must be a valid pointer from `sim_new` and must not be used after this call.
#[no_mangle]
pub unsafe extern "C" fn sim_free(handle: *mut SimHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

// --- Configuration ---

/// Set timesteps per hour.
///
/// # Safety
/// `handle` must be a valid pointer from `sim_new`.
#[no_mangle]
pub unsafe extern "C" fn sim_set_timesteps_per_hour(handle: *mut SimHandle, tph: c_int) {
    if let Some(h) = handle.as_mut() {
        let tph = tph.clamp(1, 60) as u8;
        h.driver.config.timesteps_per_hour = tph;
        h.state = SimulationState::new(tph);
    }
}

/// Set maximum warmup days.
///
/// # Safety
/// `handle` must be a valid pointer from `sim_new`.
#[no_mangle]
pub unsafe extern "C" fn sim_set_max_warmup_days(handle: *mut SimHandle, days: c_int) {
    if let Some(h) = handle.as_mut() {
        h.driver.config.max_warmup_days = days.max(1) as u32;
    }
}

/// Set HVAC convergence tolerance (W).
///
/// # Safety
/// `handle` must be a valid pointer from `sim_new`.
#[no_mangle]
pub unsafe extern "C" fn sim_set_hvac_tolerance(handle: *mut SimHandle, tol: c_double) {
    if let Some(h) = handle.as_mut() {
        h.driver.config.hvac_tolerance = tol.max(0.0);
    }
}

// --- Callbacks ---

/// Register a callback for the beginning of each environment.
///
/// # Safety
/// `handle` must be a valid pointer. `cb` must be a valid function pointer or null.
#[no_mangle]
pub unsafe extern "C" fn sim_on_begin_environment(
    handle: *mut SimHandle,
    cb: Option<extern "C" fn(*mut SimHandle)>,
) {
    if let Some(h) = handle.as_mut() {
        h.callbacks.begin_environment = cb;
    }
}

/// Register a callback for after each timestep.
///
/// # Safety
/// `handle` must be a valid pointer. `cb` must be a valid function pointer or null.
#[no_mangle]
pub unsafe extern "C" fn sim_on_after_timestep(
    handle: *mut SimHandle,
    cb: Option<extern "C" fn(*mut SimHandle)>,
) {
    if let Some(h) = handle.as_mut() {
        h.callbacks.after_timestep = cb;
    }
}

/// Register a callback for the end of each environment.
///
/// # Safety
/// `handle` must be a valid pointer. `cb` must be a valid function pointer or null.
#[no_mangle]
pub unsafe extern "C" fn sim_on_end_environment(
    handle: *mut SimHandle,
    cb: Option<extern "C" fn(*mut SimHandle)>,
) {
    if let Some(h) = handle.as_mut() {
        h.callbacks.end_environment = cb;
    }
}

// --- Data exchange ---

/// Register a variable for data exchange. Returns a variable handle (index), or -1 on error.
///
/// # Safety
/// `handle` must be a valid pointer. `name` and `key` must be valid C strings.
#[no_mangle]
pub unsafe extern "C" fn sim_register_variable(
    handle: *mut SimHandle,
    name: *const c_char,
    key: *const c_char,
    writable: c_int,
) -> c_int {
    let h = match handle.as_mut() {
        Some(h) => h,
        None => return -1,
    };
    let name = match CStr::from_ptr(name).to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return -1,
    };
    let key = match CStr::from_ptr(key).to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return -1,
    };

    let idx = h.variables.len();
    h.variables.push(Variable {
        name,
        key,
        value: 0.0,
        writable: writable != 0,
    });
    idx as c_int
}

/// Get the current value of a registered variable.
///
/// # Safety
/// `handle` must be a valid pointer. `var_handle` must be a valid index from `sim_register_variable`.
#[no_mangle]
pub unsafe extern "C" fn sim_get_variable(handle: *const SimHandle, var_handle: c_int) -> c_double {
    let h = match handle.as_ref() {
        Some(h) => h,
        None => return 0.0,
    };
    h.variables
        .get(var_handle as usize)
        .map_or(0.0, |v| v.value)
}

/// Set the value of a writable variable.
///
/// # Safety
/// `handle` must be a valid pointer. `var_handle` must be a valid index.
/// Returns 0 on success, -1 if the variable is not writable or index is invalid.
#[no_mangle]
pub unsafe extern "C" fn sim_set_variable(
    handle: *mut SimHandle,
    var_handle: c_int,
    value: c_double,
) -> c_int {
    let h = match handle.as_mut() {
        Some(h) => h,
        None => return -1,
    };
    match h.variables.get_mut(var_handle as usize) {
        Some(v) if v.writable => {
            v.value = value;
            0
        }
        _ => -1,
    }
}

// --- Run ---

/// Run the simulation. Returns 0 on success, non-zero on error.
///
/// # Safety
/// `handle` must be a valid pointer from `sim_new`.
#[no_mangle]
pub unsafe extern "C" fn sim_run(handle: *mut SimHandle) -> c_int {
    let h = match handle.as_mut() {
        Some(h) => h,
        None => return -1,
    };

    let mut adapter = ExternalCallbackAdapter {
        handle_ptr: handle,
        begin_environment: h.callbacks.begin_environment,
        after_timestep: h.callbacks.after_timestep,
        end_environment: h.callbacks.end_environment,
    };

    let result = h.driver.run(&mut h.state, &mut adapter);
    h.result = Some(result);
    0
}

// --- Results ---

/// Get number of environments completed.
///
/// # Safety
/// `handle` must be a valid pointer, and `sim_run` must have been called.
#[no_mangle]
pub unsafe extern "C" fn sim_environments_completed(handle: *const SimHandle) -> c_int {
    handle
        .as_ref()
        .and_then(|h| h.result.as_ref())
        .map_or(0, |r| r.environments_completed as c_int)
}

/// Get total timesteps simulated.
///
/// # Safety
/// `handle` must be a valid pointer, and `sim_run` must have been called.
#[no_mangle]
pub unsafe extern "C" fn sim_total_timesteps(handle: *const SimHandle) -> c_int {
    handle
        .as_ref()
        .and_then(|h| h.result.as_ref())
        .map_or(0, |r| r.total_timesteps as c_int)
}

/// Get average HVAC iterations per timestep.
///
/// # Safety
/// `handle` must be a valid pointer, and `sim_run` must have been called.
#[no_mangle]
pub unsafe extern "C" fn sim_avg_hvac_iterations(handle: *const SimHandle) -> c_double {
    handle
        .as_ref()
        .and_then(|h| h.result.as_ref())
        .map_or(0.0, |r| r.avg_hvac_iterations())
}

/// Get the last error message, or null if none.
///
/// # Safety
/// `handle` must be a valid pointer. Returned string is valid until next API call on this handle.
#[no_mangle]
pub unsafe extern "C" fn sim_last_error(handle: *const SimHandle) -> *const c_char {
    handle
        .as_ref()
        .and_then(|h| h.last_error.as_ref())
        .map_or(ptr::null(), |e| e.as_ptr())
}

// --- Rust-side public API (non-FFI) ---

/// Builder for constructing simulations from Rust code.
pub struct SimulationBuilder {
    config: SimulationConfig,
}

impl SimulationBuilder {
    pub fn new() -> Self {
        Self {
            config: SimulationConfig::default(),
        }
    }

    pub fn timesteps_per_hour(mut self, tph: u8) -> Self {
        self.config.timesteps_per_hour = tph;
        self
    }

    pub fn max_warmup_days(mut self, days: u32) -> Self {
        self.config.max_warmup_days = days;
        self
    }

    pub fn hvac_tolerance(mut self, tol: f64) -> Self {
        self.config.hvac_tolerance = tol;
        self
    }

    pub fn sizing(mut self, zone: bool, system: bool, plant: bool) -> Self {
        self.config.do_zone_sizing = zone;
        self.config.do_system_sizing = system;
        self.config.do_plant_sizing = plant;
        self
    }

    pub fn build(self) -> (SimulationDriver, SimulationState) {
        let state = SimulationState::new(self.config.timesteps_per_hour);
        let driver = SimulationDriver::new(self.config);
        (driver, state)
    }
}

impl Default for SimulationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_lifecycle() {
        unsafe {
            let handle = sim_new();
            assert!(!handle.is_null());
            sim_free(handle);
        }
    }

    #[test]
    fn set_config_values() {
        unsafe {
            let handle = sim_new();
            sim_set_timesteps_per_hour(handle, 6);
            sim_set_max_warmup_days(handle, 10);
            sim_set_hvac_tolerance(handle, 1.0);

            let h = &*handle;
            assert_eq!(h.driver.config.timesteps_per_hour, 6);
            assert_eq!(h.driver.config.max_warmup_days, 10);
            assert!((h.driver.config.hvac_tolerance - 1.0).abs() < 1e-10);

            sim_free(handle);
        }
    }

    #[test]
    fn variable_registration() {
        unsafe {
            let handle = sim_new();

            let name = CString::new("Zone Temperature").unwrap();
            let key = CString::new("Zone1").unwrap();
            let idx = sim_register_variable(handle, name.as_ptr(), key.as_ptr(), 0);
            assert_eq!(idx, 0);

            // Read-only variable
            assert_eq!(sim_get_variable(handle, 0), 0.0);
            assert_eq!(sim_set_variable(handle, 0, 25.0), -1); // Not writable

            // Writable variable
            let name2 = CString::new("Actuator").unwrap();
            let key2 = CString::new("Act1").unwrap();
            let idx2 = sim_register_variable(handle, name2.as_ptr(), key2.as_ptr(), 1);
            assert_eq!(idx2, 1);
            assert_eq!(sim_set_variable(handle, 1, 42.0), 0);
            assert_eq!(sim_get_variable(handle, 1), 42.0);

            sim_free(handle);
        }
    }

    #[test]
    fn run_empty_simulation() {
        unsafe {
            let handle = sim_new();
            // No run periods or design days → runs but does nothing
            let rc = sim_run(handle);
            assert_eq!(rc, 0);
            assert_eq!(sim_environments_completed(handle), 0);
            assert_eq!(sim_total_timesteps(handle), 0);

            sim_free(handle);
        }
    }

    #[test]
    fn null_handle_safety() {
        unsafe {
            // All functions should handle null gracefully
            sim_free(ptr::null_mut());
            sim_set_timesteps_per_hour(ptr::null_mut(), 4);
            assert_eq!(sim_get_variable(ptr::null(), 0), 0.0);
            assert_eq!(sim_set_variable(ptr::null_mut(), 0, 1.0), -1);
            assert_eq!(sim_run(ptr::null_mut()), -1);
            assert_eq!(sim_environments_completed(ptr::null()), 0);
            assert!(sim_last_error(ptr::null()).is_null());
        }
    }

    #[test]
    fn builder_api() {
        let (driver, _state) = SimulationBuilder::new()
            .timesteps_per_hour(6)
            .max_warmup_days(10)
            .hvac_tolerance(0.25)
            .sizing(true, false, false)
            .build();

        assert_eq!(driver.config.timesteps_per_hour, 6);
        assert_eq!(driver.config.max_warmup_days, 10);
        assert!((driver.config.hvac_tolerance - 0.25).abs() < 1e-10);
        assert!(driver.config.do_zone_sizing);
        assert!(!driver.config.do_system_sizing);
    }
}
