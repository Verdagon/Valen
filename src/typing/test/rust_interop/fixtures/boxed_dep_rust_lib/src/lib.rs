pub struct Gem {
    value: i64,
}

impl Gem {
    pub fn get(&self) -> i64 {
        self.value
    }
}

pub struct Chest {
    gem: Box<Gem>,
}

impl Chest {
    pub fn new() -> Chest {
        Chest { gem: Box::new(Gem { value: 0 }) }
    }

    pub fn replace(&mut self, value: i64) {
        self.gem = Box::new(Gem { value });
    }

    pub fn gem(&self) -> &Gem {
        &self.gem
    }
}
