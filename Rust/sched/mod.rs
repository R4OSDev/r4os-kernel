mod migration_policy;
pub(crate) mod placement;
mod refill_cache;
pub mod scheduler;
pub mod scheduler_types;
pub(crate) mod secondary_idle;
pub mod task;
pub mod task_context;
pub mod task_index;
mod wait_counters;
pub mod wait_node;

#[path = "../kernel/deadman.rs"]
pub mod deadman;
pub(crate) mod kernel_check;
pub(crate) mod mutex_pi;
pub(crate) mod sync_guard;
pub mod sync_locks;
pub mod sync_objects;
pub mod sync_selftest;
pub mod sync_stats;
pub mod sync_types;
pub mod sync_wait;
pub mod task_probe;

pub(crate) mod fpu_observation;
pub(crate) mod worker_observation;
