
use bumpalo::Bump;

#[inline(always)]
pub fn alloc_slice_copy<'x, T: Copy>(arena: &'x Bump, slice: &[T]) -> &'x [T] {
  arena.alloc_slice_copy(slice)
}

#[inline(always)]
pub fn alloc_slice_fill_iter<'x, T, I>(arena: &'x Bump, iter: I) -> &'x [T]
where
  I: IntoIterator<Item = T>,
  I::IntoIter: ExactSizeIterator,
{
  arena.alloc_slice_fill_iter(iter)
}

#[inline(always)]
pub fn alloc_slice_from_vec<'x, T>(arena: &'x Bump, vec: Vec<T>) -> &'x [T] {
  arena.alloc_slice_fill_iter(vec.into_iter())
}

#[inline(always)]
pub fn alloc_slice_from_vec_of_refs<'x, T>(arena: &'x Bump, vec: Vec<&'x T>) -> &'x [&'x T] {
  arena.alloc_slice_copy(&vec)
}
