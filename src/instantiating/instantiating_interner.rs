use bumpalo::Bump;

use crate::utils::arena_index_map::ArenaIndexMap;
use std::hash::Hash;
use std::marker::PhantomData;

pub struct InstantiatingInterner<'s, 'i>
where 's: 'i,
{
    bump: &'i Bump,
    _marker: PhantomData<&'s ()>,
}

impl<'s, 'i> InstantiatingInterner<'s, 'i>
where 's: 'i,
{
    pub fn new(bump: &'i Bump) -> Self {
        InstantiatingInterner { bump, _marker: PhantomData }
    }

    pub fn bump(&self) -> &'i Bump { self.bump }
    pub fn alloc<T>(&self, val: T) -> &'i mut T { self.bump.alloc(val) }
    pub fn alloc_slice_copy<T: Copy>(&self, src: &[T]) -> &'i [T] {
        self.bump.alloc_slice_copy(src)
    }
    pub fn alloc_slice_from_vec<T>(&self, vec: Vec<T>) -> &'i [T] {
        self.bump.alloc_slice_fill_iter(vec.into_iter())
    }

    pub fn alloc_index_map<K: Hash + Eq + Clone, V>(&self) -> ArenaIndexMap<'i, K, V> {
        ArenaIndexMap::new_in(self.bump)
    }

    pub fn alloc_index_map_from_iter<K, V, I>(&self, iter: I) -> ArenaIndexMap<'i, K, V>
    where K: Hash + Eq + Clone, I: IntoIterator<Item = (K, V)>
    {
        ArenaIndexMap::from_iter_in(iter, self.bump)
    }
}

// V: figure out a better place for these
#[cfg(all(test, any()))]
mod tests {
    use super::*;

    #[test]
    fn intern_struct_it_si_canonicalizes() {
        let bump = Bump::new();
        let intr = InstantiatingInterner::new(&bump);

        let v1 = StructITValI::<'_, '_, sI> { id: IdI(PhantomData) };
        let v2 = StructITValI::<'_, '_, sI> { id: IdI(PhantomData) };

        let r1 = intr.intern_struct_it_si(v1);
        let r2 = intr.intern_struct_it_si(v2);

        // Two equal Val inputs should be the same pointer.
        assert!(eq(r1, r2));
    }

    #[test]
    fn intern_kind_payload_si_dispatches() {
        let bump = Bump::new();
        let intr = InstantiatingInterner::new(&bump);

        let val = InternedKindPayloadValI::<'_, '_, sI>::StructIT(StructITValI { id: IdI(PhantomData) });
        let r1 = intr.intern_kind_payload_si(val);
        let r2 = intr.intern_kind_payload_si(val);

        match (r1, r2) {
            (InternedKindPayloadI::StructIT(a), InternedKindPayloadI::StructIT(b)) => {
                assert!(eq(a, b));
            }
            _ => panic!("expected StructIT variant"),
        }
    }


    #[test]
    fn intern_name_si_canonicalizes_via_family() {
        use crate::instantiating::ast::names::{
            INameI, INameValI, PackageTopLevelNameI,
        };
        let bump = Bump::new();
        let intr = InstantiatingInterner::new(&bump);

        let v1 = PackageTopLevelNameI::<'_, '_, sI>(PhantomData);
        let v2 = PackageTopLevelNameI::<'_, '_, sI>(PhantomData);

        let r1 = match intr.intern_name_si(INameValI::PackageTopLevel(v1)) {
            INameI::PackageTopLevel(r) => r,
            _ => unreachable!(),
        };
        let r2 = match intr.intern_name_si(INameValI::PackageTopLevel(v2)) {
            INameI::PackageTopLevel(r) => r,
            _ => unreachable!(),
        };

        // Two equal Val inputs should be the same pointer.
        assert!(eq(r1, r2));
    }

    #[test]
    fn intern_name_per_concrete_wrapper_works() {
        let bump = Bump::new();
        let intr = InstantiatingInterner::new(&bump);

        let v = PackageTopLevelNameI::<'_, '_, sI>(PhantomData);
        let r1 = intr.intern_package_top_level_name_si(v);
        let r2 = intr.intern_package_top_level_name_si(v);

        assert!(eq(r1, r2));
    }
}
