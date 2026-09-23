// The crate the driver host compiles for the `&mut`-signature reverse-callback repro.
//
// `Window`, `Frame`, `MainLoop` are re-exported real Rust items; only `MyCb` + its `on_tick` are
// Valen-projected. `on_tick` takes `&mut self` + a `&mut Window` param + a shared `&Frame` param.
#![feature(register_tool)]
#![register_tool(vale)]

extern crate mycrate;

use std::process::exit;

pub use mycrate::{Frame, MainLoop, Window};

pub const __VALE_STUBS_MARKER: () = ();

pub struct MyCb {}

impl MainLoop for MyCb {
    #[vale::emit_consumer_body]
    fn on_tick(&mut self, _w: &mut Window, _input: &Frame) {
        unreachable!()
    }
}

#[vale::emit_consumer_body]
pub fn __vale_main() -> i32 {
    unreachable!()
}

fn main() {
    exit(__vale_main());
}

#[inline(never)]
pub unsafe fn __vale_drop<T>(x: *mut T) {
    core::ptr::drop_in_place(x)
}
