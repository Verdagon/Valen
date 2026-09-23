//! The window shell, stubbed: the public `MainLoopCallback` trait and `NobiliaWindow` methods
//! `driver_check.valen` drives. The real winit/wgpu event loop is gone with the dependencies.
//!
//! This is the `&mut`-signature shape: `on_tick(&mut self, w: &mut NobiliaWindow, …)` and the mutating
//! window methods take `&mut self` — NobiliaV's real driver. The frame loop is a deterministic, bounded
//! `&mut self` counter (no `Cell`, no wall clock, no windowing), so it always terminates. The auto-
//! generated anon substruct renders `&mut` in its projected Rust impl; the callback churns the window,
//! which `valen build --no-borrow-check` steps around (churn enforcement is unbuilt).

use crate::FrameInput;

/// A per-frame callback the library invokes each frame. A Valen struct (here the compiler-synthesized
/// anonymous substruct for a lambda) implements this via the rust-interop trait mechanism. `&mut self`
/// plus a `&mut NobiliaWindow` param is NobiliaV's real churning shape.
pub trait MainLoopCallback {
    /// React to one frame: read `input`, and mutate `w`.
    fn on_tick(&mut self, w: &mut NobiliaWindow, input: &FrameInput);
}

/// A hard upper bound on frames, so the loop always terminates even if the callback never asks to exit
/// — a run-to-completion test that could hang is worse than a link-only one.
const MAX_FRAMES: i32 = 100;

/// A window a Valen driver builds and hands a callback. Opaque to the interop (Valen never reads these
/// fields — it sees the type as an opaque blob), so a plain frame counter + exit flag are a
/// deterministic, dependency-free stand-in for the real (GPU-backed) state, mutated through `&mut self`.
pub struct NobiliaWindow {
    pub width: i32,
    pub height: i32,
    frame_index: i32,
    should_exit: bool,
}

impl NobiliaWindow {
    /// A window of this logical size.
    pub fn new(width: i32, height: i32) -> NobiliaWindow {
        return NobiliaWindow { width, height, frame_index: 0, should_exit: false };
    }

    /// How many frames have run so far.
    pub fn frame_index(&self) -> i32 {
        return self.frame_index;
    }

    /// Load the checked-in reference map.
    pub fn load_terrain(&self) {}

    /// Frame the whole current level at this viewpoint (whole `i32` degrees).
    pub fn fit_camera(&self, _az: i32, _el: i32) {}

    /// Nudge the camera by whole degrees of azimuth/elevation — the real churn (`&mut self`).
    pub fn rotate_camera(&mut self, _d_az: i32, _d_el: i32) {}

    /// Print what a click landed on.
    pub fn report_pick(&self, _x: i32, _y: i32) {}

    /// Ask the loop to exit after this frame.
    pub fn request_exit(&mut self) {
        self.should_exit = true;
    }

    /// Run a deterministic, bounded frame loop, calling `cb.on_tick(self, &input)` each frame until the
    /// callback asks to exit (`request_exit`) or the hard cap is hit. A generic *method* caller — the
    /// shape that exercises the reverse-callback dispatch. Input is empty every frame (no key/mouse), so
    /// `driver_check`'s callback just counts frames and calls `request_exit` at frame 30 → the loop runs
    /// ~31 deterministic frames and returns, and `main` returns 7.
    pub fn main_loop<C: MainLoopCallback>(&mut self, cb: &mut C) {
        while !self.should_exit && self.frame_index < MAX_FRAMES {
            let input = FrameInput { quit: false, key: -1, mouse_x: -1, mouse_y: -1 };
            cb.on_tick(self, &input);
            self.frame_index += 1;
        }
    }
}
