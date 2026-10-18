//! Stable direct platform callback facade. No state, wrapper calls or dispatch.
//! Implementations keep their original program parent and exact typed owners.
pub(super) use super::platform_callbacks_async_io::*;
pub(super) use super::platform_callbacks_desktop::*;
pub(super) use super::platform_callbacks_drawing::*;
pub(super) use super::platform_callbacks_files_storage::*;
pub(super) use super::platform_callbacks_process::*;
pub(super) use super::platform_callbacks_services::*;
pub(super) use super::platform_callbacks_system::*;
