
use bumpalo::Bump;


pub struct InstantiatingArena<'i> {
    bump: &'i Bump,
}

impl<'i> InstantiatingArena<'i> {
    pub fn new(bump: &'i Bump) -> Self {
        InstantiatingArena { bump }
    }

    pub fn bump(&self) -> &'i Bump { self.bump }
}
