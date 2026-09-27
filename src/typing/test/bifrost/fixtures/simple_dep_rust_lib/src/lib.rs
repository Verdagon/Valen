pub fn add_i64(a: i64, b: i64) -> i64 {
    a + b
}

pub struct Slot {
    value: Box<i64>,
}

impl Slot {
    pub fn new() -> Slot {
        Slot { value: Box::new(0) }
    }

    pub fn mutate(&mut self, x: i64) {
        *self.value = x;
    }

    pub fn get(&self) -> i64 {
        *self.value
    }
}
