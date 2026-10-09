pub struct Slot {
    value: i32,
}

impl Slot {
    pub fn new() -> Slot {
        Slot { value: 0 }
    }

    pub fn mutate(&mut self, x: i32) {
        self.value = x;
    }

    pub fn get(&self) -> i32 {
        self.value
    }
}
