
use std::marker::PhantomData;

#[derive(Clone, Copy)]
pub struct Oracles<'ctx, 's, 't>
where
  's: 't,
{
  _marker: PhantomData<(&'ctx (), &'s (), &'t ())>,
}

impl<'ctx, 's, 't> Oracles<'ctx, 's, 't>
where
  's: 't,
{
  pub fn none() -> Self {
    Oracles {
      _marker: PhantomData,
    }
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
