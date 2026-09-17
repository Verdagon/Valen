// An imported Rust trait whose method takes `&mut self` plus a `&mut`-borrowed imported param, with a
// shared `&` param as the negative control — NobiliaV's `on_tick(&mut self, w: &mut NobiliaWindow,
// input: &FrameInput)` shape. Used to prove the pass-2 (HinputsT-driven) stub generator renders `&mut`
// for the auto-generated anon substruct (not just the parse-driven forwarder path).

pub struct Window { pub ticks: i32 }
pub struct Frame {}

pub trait MainLoop {
    fn on_tick(&mut self, w: &mut Window, input: &Frame);
}

impl Window {
    pub fn new() -> Window { Window { ticks: 0 } }
    pub fn push(&mut self) { self.ticks += 1; }
    /// A generic `&mut self` caller that owns a `Frame` and calls the callback's `&mut self` `on_tick`
    /// with `&mut self` (the window) inbound — the `main_loop`/`on_tick` shape.
    pub fn run<C: MainLoop>(&mut self, cb: &mut C) -> i32 {
        let frame = Frame {};
        cb.on_tick(self, &frame);
        7
    }
}
