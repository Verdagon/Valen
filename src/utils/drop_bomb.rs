use std::thread;

pub struct DropBomb {
  armed: bool,
  message: &'static str,
}

impl DropBomb {
  pub fn armed(message: &'static str) -> DropBomb {
    DropBomb { armed: true, message }
  }

  pub fn arm(&mut self) {
    self.armed = true;
  }

  pub fn defuse(&mut self) {
    self.armed = false;
  }
}

impl Drop for DropBomb {
  fn drop(&mut self) {
    debug_assert!(!self.armed || thread::panicking(), "{}", self.message);
  }
}
