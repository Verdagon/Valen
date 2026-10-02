use std::cell::Cell;
use std::ptr::null;

use super::bifrost_state::BifrostState;

thread_local! {
  // A raw pointer to the current horizon `BifrostState`, set for the duration of one driven pass 2.
  // Thread-local because rustc runs the compilation — `after_expansion`, where it is set, and every
  // provider — on one thread it spawns.
  static HORIZON_STATE: Cell<*const ()> = const { Cell::new(null()) };
}

/// Point the providers at `state`. Called from `after_expansion`, on the rustc thread the providers
/// will fire on; `state` lives in the frame that calls `run_compiler`, so it outlives every provider
/// call.
pub fn set_bifrost_state_ptr(state: *const ()) {
  HORIZON_STATE.with(|c| c.set(state));
}

/// The state the providers read. Panics if no driven pass armed it: horizon's providers are installed
/// only on the driven path, so firing without state is a bug, never a benign fallthrough.
pub(super) fn horizon_state<'a>(what: &str) -> &'a BifrostState<'a, 'a, 'a, 'a> {
  let state_ptr = HORIZON_STATE.with(|c| c.get());
  assert!(!state_ptr.is_null(), "{what} fired with no horizon BifrostState armed");
  // SAFETY: the state lives in the `run_compiler`-calling frame, which outlives every provider call
  // on this thread; it is only ever borrowed shared, and nothing borrowing it escapes a provider.
  unsafe { &*(state_ptr as *const BifrostState) }
}
