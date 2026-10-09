pub struct Small {
    pub a: i32,
    pub b: i32,
}

impl Small {
    pub fn sum(&self) -> i32 {
        self.a + self.b
    }
}

pub trait Summer {
    fn on_sum(&self, s: Small) -> i32;
}

pub fn run_summer<C: Summer>(c: &C) -> i32 {
    c.on_sum(Small { a: 3, b: 6 })
}
