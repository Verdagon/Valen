pub struct Counter {
    value: i32,
}

impl Counter {
    pub fn new() -> Counter {
        Counter { value: 5 }
    }
    pub fn peek(&self) -> i32 {
        self.value
    }
}

pub trait Ticker {
    fn on_tick(&self, w: &Counter) -> i32;
}

pub fn run_ticker<C: Ticker>(c: &C) -> i32 {
    let w = Counter::new();
    c.on_tick(&w)
}
