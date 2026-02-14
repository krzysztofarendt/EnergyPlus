//! Supermarket refrigeration system simulation.
//!
//! Models compressor racks with air- or evaporatively-cooled condensers,
//! refrigerated display cases, walk-in coolers/freezers, and secondary
//! glycol/CO2 loops.

pub mod case;
pub mod compressor;
pub mod condenser;
pub mod secondary;
pub mod system;
pub mod walkin;
