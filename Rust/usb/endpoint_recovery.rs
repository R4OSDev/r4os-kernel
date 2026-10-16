//! Preserve exact Running/Halted/Stopped/Error recovery order and refresh rule.
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum EndpointState {
    Disabled = 0,
    Running = 1,
    Halted = 2,
    Stopped = 3,
    Error = 4,
}
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum PrepareAction {
    Reconfigure = 0,
    SetDequeue = 1,
    Stop = 2,
    Reset = 3,
}
pub const CONTEXT_STATE_ERROR: u8 = 19;
pub fn state_from_raw(raw: u8) -> Option<EndpointState> {
    match raw {
        0 => Some(EndpointState::Disabled),
        1 => Some(EndpointState::Running),
        2 => Some(EndpointState::Halted),
        3 => Some(EndpointState::Stopped),
        4 => Some(EndpointState::Error),
        _ => None,
    }
}
pub fn prepare_action(state: EndpointState) -> PrepareAction {
    match state {
        EndpointState::Running => PrepareAction::Stop,
        EndpointState::Halted => PrepareAction::Reset,
        EndpointState::Stopped | EndpointState::Error => PrepareAction::SetDequeue,
        EndpointState::Disabled => PrepareAction::Reconfigure,
    }
}
pub fn completion_needs_state_refresh(code: u8) -> bool {
    code == CONTEXT_STATE_ERROR
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_endpoint_state(raw: u8) -> u8 {
    state_from_raw(raw).map_or(255, |state| state as u8)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_endpoint_prepare(raw: u8) -> u8 {
    prepare_action(state_from_raw(raw).expect("Typed USB endpoint state")) as u8
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_usb_endpoint_refresh(code: u8) -> bool {
    completion_needs_state_refresh(code)
}
