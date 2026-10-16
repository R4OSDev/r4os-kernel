//! Native kernel module loading owners. Private C facades preserve existing
//! callers until the remaining implementation owners migrate.
pub mod boot;
pub mod file;
pub mod format;
pub mod r4d;
pub mod r4m;
pub mod r4p;
pub mod registry_control;
pub mod registry_loader;
pub mod registry_queries;
pub mod registry_state;
pub mod resources;

pub mod performance;
