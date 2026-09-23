
#[cfg(feature = "rust_interop")]
use crate::typing::rust_interop::RustOracle;

#[cfg(not(feature = "rust_interop"))]
use std::marker::PhantomData;

#[derive(Clone, Copy)]
pub struct Oracles<'ctx, 's, 't>
where
  's: 't,
{
  #[cfg(feature = "rust_interop")]
  pub rust: Option<&'ctx dyn RustOracle<'s, 't>>,

  #[cfg(not(feature = "rust_interop"))]
  _marker: PhantomData<(&'ctx (), &'s (), &'t ())>,
}

impl<'ctx, 's, 't> Oracles<'ctx, 's, 't>
where
  's: 't,
{
  pub fn none() -> Self {
    Oracles {
      #[cfg(feature = "rust_interop")]
      rust: None,
      #[cfg(not(feature = "rust_interop"))]
      _marker: PhantomData,
    }
  }

  #[cfg(feature = "rust_interop")]
  pub fn with_rust(rust: &'ctx dyn RustOracle<'s, 't>) -> Self {
    Oracles { rust: Some(rust) }
  }
}

impl<'ctx, 's, 't> Default for Oracles<'ctx, 's, 't>
where
  's: 't,
{
  fn default() -> Self {
    Oracles::none()
  }
}
