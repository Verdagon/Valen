pub struct Small2 {
    pub a: i32,
    pub b: i32,
}

impl Small2 {
    pub fn new(a: i32, b: i32) -> Small2 {
        Small2 { a, b }
    }
    pub fn sum(&self) -> i32 {
        self.a + self.b
    }
}

pub fn add_small(s: Small2) -> i32 {
    s.a + s.b
}
