pub struct Window { pub ticks: i32 }
pub struct Frame {}

pub trait MainLoop {
    fn on_tick(&mut self, w: &mut Window, input: &Frame);
}

impl Window {
    pub fn new() -> Window { Window { ticks: 0 } }
    pub fn push(&mut self) { self.ticks += 1; }
    pub fn run<C: MainLoop>(&mut self, cb: &mut C) -> i32 {
        let frame = Frame {};
        cb.on_tick(self, &frame);
        7
    }
}
