pub fn add_two_numbers(a: i32, b: i32) -> i32 {
    a + b
}

pub fn add_i64(a: i64, b: i64) -> i64 {
    a + b
}

pub struct Counter {
    pub value: i32,
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

pub fn seven() -> i32 {
    7
}

pub fn do_nothing() {}

pub fn make_counter() -> Counter {
    Counter { value: 7 }
}
