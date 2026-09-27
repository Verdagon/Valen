use std::cell::Cell;
use std::ptr::null;

thread_local! {
  pub(super) static BIFROST_STATE: Cell<*const ()> = const { Cell::new(null()) };
}

// Should be called by the same thread that will be reading BIFROST_STATE.
pub fn set_bifrost_state_ptr(state: *const ()) {
  BIFROST_STATE.with(|c| c.set(state));
}
