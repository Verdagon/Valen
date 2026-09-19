//! Build the borrow checker's group-annotated `KindGT` for a value, and re-express its groups in
//! another frame — now on the canonical `borrow_checker::{kind_g, group_expr, templata_g}` types.
//!
//! `KindGT` mirrors the typing pass's `KindT` variant-for-variant; the only addition is a `GroupExprG`
//! on each borrow. Groups are read from the written `ITypeST` for the outer borrow layers and array
//! elements. Template arguments are **group-free** (groups never flow through the solver), so a
//! citizen's args are a plain structural mirror built groupless.
//!
//! Two entry points:
//!  * `make_kind_g(kind, tyype, param_name, arena)` builds a `KindGT` with groups in `tyype`'s frame.
//!  * `substitute_groups(kindg, subst, arena)` crosses a `KindGT`'s groups into another frame.
//!
//! Plus the payload mirrors `groupify` needs to fill the canonical `*GE` nodes: `struct_gt`,
//! `interface_gt`, `ssa_gt`, `rsa_gt`, `super_kind_gt`, `templata_g`.
//!
//! Everything is arena-allocated in the per-check `'g` bump: compound `KindGT` payloads, the
//! `*TemplataG` wrappers of a citizen's args, and the path slices of every `GroupExprG` live in
//! `arena`; the group roots borrow the `'s`/`'t` scout/typed data.

use bumpalo::Bump;

use crate::postparsing::names::IRuneS;
use crate::postparsing::rules::types::{GroupS, ITypeST, RegionS};
use crate::typing::borrow_checker::experimental::grouped_ast::single_path;
use crate::typing::borrow_checker::group_expr::{GroupChildStepG, GroupExprG, GroupPathG, GroupRootG};
use crate::typing::borrow_checker::kind_g::{
  BoolGT, BorrowRefGT, FloatGT, ISuperKindGT, IntGT, InterfaceGT, KindGT, KindPlaceholderGT, NeverGT,
  OverloadSetGT, OwnRefGT, RuntimeSizedArrayGT, ShareRefGT, StaticSizedArrayGT, StrGT, StructGT,
  USizeGT, VoidGT, WeakRefGT,
};
use crate::typing::borrow_checker::templata_g::{
  ExternFunctionTemplataG, FunctionTemplataG, GroupTemplataG, ITemplataG, ImplDefinitionTemplataG,
  InterfaceDefinitionTemplataG, IsaTemplataG, KindListTemplataG, KindTemplataG, PlaceholderTemplataG,
  PrototypeTemplataG, RuntimeSizedArrayTemplateTemplataG, StaticSizedArrayTemplateTemplataG,
  StructDefinitionTemplataG,
};
use crate::typing::compiler::Compiler;
use crate::typing::names::names::{IdT, INameT, IVarNameT};
use crate::typing::templata::templata::ITemplataT;
use crate::typing::types::types::{
  ISuperKindTT, InterfaceTT, KindT, RuntimeSizedArrayTT, StaticSizedArrayTT, StructTT,
};
use crate::utils::fx::IndexMap;
use std::marker::PhantomData;

impl<'s, 'ctx, 't> Compiler<'s, 'ctx, 't> {
  /// Build a `KindGT`: structure from `kind` (the typing pass's `KindT`), and a group for every borrow
  /// layer from the written `tyype`, walked in parallel. `param_name` keys an unannotated borrow's
  /// anonymous group (`None` outside a parameter). Where the typed kind carries a wrap the written type
  /// never had, the outer borrow takes the parameter's anonymous group and the subtree is groupless
  /// (`make_kind_g_groupless`). Pure.
  pub fn make_kind_g<'g>(
    &self,
    kind: KindT<'s, 't>,
    tyype: &'s ITypeST<'s>,
    param_name: Option<&'t IVarNameT<'s, 't>>,
    arena: &'g Bump,
  ) -> KindGT<'s, 't, 'g> {
    match kind {
      KindT::Never(x) => KindGT::Never(NeverGT { from_break: x.from_break }),
      KindT::Void(_) => KindGT::Void(VoidGT),
      KindT::Int(x) => KindGT::Int(IntGT { bits: x.bits }),
      KindT::Bool(_) => KindGT::Bool(BoolGT),
      KindT::Str(_) => KindGT::Str(StrGT),
      KindT::Float(_) => KindGT::Float(FloatGT),
      KindT::USize(_) => KindGT::USize(USizeGT),
      KindT::KindPlaceholder(p) => {
        KindGT::KindPlaceholder(arena.alloc(KindPlaceholderGT { id: p.id, _phantom: PhantomData }))
      }
      KindT::OverloadSet(o) => {
        KindGT::OverloadSet(arena.alloc(OverloadSetGT { env_id: o.env.id(), _phantom: PhantomData }))
      }

      // A citizen written `List<&Header>` carries its args' written types, so each `Kind` arg gets its
      // groups from there, like an array element does.
      KindT::Struct(s) => KindGT::Struct(arena.alloc(StructGT {
        id: s.id,
        template_args: self.citizen_args_in(self.citizen_template_args(*s.id), tyype, param_name, arena),
      })),
      KindT::Interface(i) => KindGT::Interface(arena.alloc(InterfaceGT {
        id: i.id,
        template_args: self.citizen_args_in(self.citizen_template_args(*i.id), tyype, param_name, arena),
      })),

      // A static-sized array is written `StaticArray<N, T>` — a `Call` whose second arg is the element.
      KindT::StaticSizedArray(a) => match tyype {
        ITypeST::Call(c) if c.args.len() == 2 => KindGT::StaticSizedArray(arena.alloc(StaticSizedArrayGT {
          name: a.name,
          element_type: self.make_kind_g(a.element_type(), c.args[1], param_name, arena),
        })),
        _ => self.make_kind_g_groupless(kind, arena),
      },
      KindT::RuntimeSizedArray(a) => match tyype {
        ITypeST::RuntimeSizedArray(st) => KindGT::RuntimeSizedArray(arena.alloc(RuntimeSizedArrayGT {
          name: a.name,
          element_type: self.make_kind_g(a.element_type(), st.element, param_name, arena),
        })),
        _ => self.make_kind_g_groupless(kind, arena),
      },

      KindT::BorrowRef(b) => match tyype {
        ITypeST::BorrowRef(st) => {
          let inner = self.make_kind_g(b.inner, st.inner, param_name, arena);
          let group = match st.region {
            RegionS::Group(gs) => group_expr_from_group_s(gs, arena),
            RegionS::Unspecified => group_anon(param_name, arena),
            RegionS::Held => group_anon(param_name, arena),
          };
          KindGT::BorrowRef(arena.alloc(BorrowRefGT { group: GroupTemplataG { group, kind: inner }, inner }))
        }
        _ => {
          let inner = self.make_kind_g_groupless(b.inner, arena);
          let group = group_anon(param_name, arena);
          KindGT::BorrowRef(arena.alloc(BorrowRefGT { group: GroupTemplataG { group, kind: inner }, inner }))
        }
      },
      KindT::OwnRef(w) => match tyype {
        ITypeST::OwnRef(st) => {
          KindGT::OwnRef(arena.alloc(OwnRefGT { inner: self.make_kind_g(w.inner, st.inner, param_name, arena) }))
        }
        _ => KindGT::OwnRef(arena.alloc(OwnRefGT { inner: self.make_kind_g_groupless(w.inner, arena) })),
      },
      KindT::WeakRef(w) => match tyype {
        ITypeST::WeakRef(st) => {
          KindGT::WeakRef(arena.alloc(WeakRefGT { inner: self.make_kind_g(w.inner, st.inner, param_name, arena) }))
        }
        _ => KindGT::WeakRef(arena.alloc(WeakRefGT { inner: self.make_kind_g_groupless(w.inner, arena) })),
      },
      // A claim roots the ambient multi `rc`, so this layer carries no group; the written type is a bare
      // citizen, so the payload recurses against the same `tyype`.
      KindT::ShareRef(w) => {
        KindGT::ShareRef(arena.alloc(ShareRefGT { inner: self.make_kind_g(w.inner, tyype, param_name, arena) }))
      }
    }
  }

  /// A citizen kind's generic template args, off its `IdT`'s citizen name.
  fn citizen_template_args(&self, id: IdT<'s, 't>) -> &'t [ITemplataT<'s, 't>] {
    match id.local_name {
      INameT::Struct(n) => n.template_args,
      INameT::Interface(n) => n.template_args,
      _ => &[],
    }
  }

  /// A citizen's `KindGT` generic args with its written type at hand: each `Kind` arg is groupified
  /// against its written argument, walked in parallel (`List<&Header>` gives the inner borrow the
  /// parameter's group, as a top-level unannotated borrow gets). Written as anything but a `Call` with
  /// one argument per templata — a bare rune, or a citizen whose written args don't line up with its
  /// templatas — the args mirror groupless, as `citizen_args` does.
  fn citizen_args_in<'g>(
    &self,
    templata_args: &'t [ITemplataT<'s, 't>],
    tyype: &'s ITypeST<'s>,
    param_name: Option<&'t IVarNameT<'s, 't>>,
    arena: &'g Bump,
  ) -> &'g [ITemplataG<'s, 't, 'g>] {
    let written: &'s [&'s ITypeST<'s>] = match tyype {
      ITypeST::Call(c) if c.args.len() == templata_args.len() => c.args,
      _ => return self.citizen_args(templata_args, arena),
    };
    let v: Vec<ITemplataG<'s, 't, 'g>> = templata_args
      .iter()
      .zip(written)
      .map(|(templata, written_arg)| match (templata, written_arg) {
        (ITemplataT::Kind(k), _) => {
          ITemplataG::Kind(KindTemplataG { kind: self.make_kind_g(k.kind, written_arg, param_name, arena) })
        }
        // A group argument written as a rune, e.g. the `g` in `Vec<T, g>`. No citizen declares a group
        // parameter yet, and the group's referent type is not knowable from the argument alone.
        (ITemplataT::Group(_), ITypeST::Rune(_)) => unimplemented!("vfail: citizen group parameter"),
        (other, _) => self.templata_g(*other, arena),
      })
      .collect();
    arena.alloc_slice_fill_iter(v)
  }

  /// A citizen's `KindGT` generic args with no written type: a groupless structural mirror.
  fn citizen_args<'g>(
    &self,
    templata_args: &'t [ITemplataT<'s, 't>],
    arena: &'g Bump,
  ) -> &'g [ITemplataG<'s, 't, 'g>] {
    let v: Vec<ITemplataG<'s, 't, 'g>> =
      templata_args.iter().map(|t| self.templata_g(*t, arena)).collect();
    arena.alloc_slice_fill_iter(v)
  }

  /// The grouped mirror of a typed struct kind (its args groupless).
  pub(crate) fn struct_gt<'g>(&self, s: &'t StructTT<'s, 't>, arena: &'g Bump) -> &'g StructGT<'s, 't, 'g> {
    arena.alloc(StructGT {
      id: s.id,
      template_args: self.citizen_args(self.citizen_template_args(*s.id), arena),
    })
  }

  /// The grouped mirror of a typed interface kind (its args groupless).
  pub(crate) fn interface_gt<'g>(
    &self,
    i: &'t InterfaceTT<'s, 't>,
    arena: &'g Bump,
  ) -> &'g InterfaceGT<'s, 't, 'g> {
    arena.alloc(InterfaceGT {
      id: i.id,
      template_args: self.citizen_args(self.citizen_template_args(*i.id), arena),
    })
  }

  /// The grouped mirror of a typed static-sized array kind, its element groupless.
  pub(crate) fn ssa_gt<'g>(
    &self,
    a: &'t StaticSizedArrayTT<'s, 't>,
    arena: &'g Bump,
  ) -> &'g StaticSizedArrayGT<'s, 't, 'g> {
    arena.alloc(StaticSizedArrayGT {
      name: a.name,
      element_type: self.make_kind_g_groupless(a.element_type(), arena),
    })
  }

  /// The grouped mirror of a typed runtime-sized array kind, its element groupless.
  pub(crate) fn rsa_gt<'g>(
    &self,
    a: &'t RuntimeSizedArrayTT<'s, 't>,
    arena: &'g Bump,
  ) -> &'g RuntimeSizedArrayGT<'s, 't, 'g> {
    arena.alloc(RuntimeSizedArrayGT {
      name: a.name,
      element_type: self.make_kind_g_groupless(a.element_type(), arena),
    })
  }

  /// The grouped mirror of an upcast's target super kind.
  pub(crate) fn super_kind_gt<'g>(
    &self,
    super_kind: ISuperKindTT<'s, 't>,
    arena: &'g Bump,
  ) -> ISuperKindGT<'s, 't, 'g> {
    match super_kind {
      ISuperKindTT::Interface(i) => ISuperKindGT::Interface(self.interface_gt(i, arena)),
      ISuperKindTT::KindPlaceholder(p) => ISuperKindGT::KindPlaceholder(p),
    }
  }

  /// Mirror one typing templata into an `ITemplataG` — group-free and structural. A `Kind` mirrors
  /// its `KindT` groupless; the solver-domain payloads become their `*TemplataG` wrappers in `arena`;
  /// a `Group` has no written group to read here and panics.
  pub(crate) fn templata_g<'g>(
    &self,
    templata: ITemplataT<'s, 't>,
    arena: &'g Bump,
  ) -> ITemplataG<'s, 't, 'g> {
    match templata {
      ITemplataT::Kind(k) => {
        ITemplataG::Kind(KindTemplataG { kind: self.make_kind_g_groupless(k.kind, arena) })
      }
      ITemplataT::Placeholder(p) => {
        ITemplataG::Placeholder(arena.alloc(PlaceholderTemplataG { id: p.id, tyype: p.tyype }))
      }
      ITemplataT::Integer(v) => ITemplataG::Integer(v),
      ITemplataT::Boolean(v) => ITemplataG::Boolean(v),
      ITemplataT::String(v) => ITemplataG::String(v),
      ITemplataT::Prototype(p) => {
        ITemplataG::Prototype(arena.alloc(PrototypeTemplataG { prototype: p.prototype }))
      }
      ITemplataT::Isa(isa) => ITemplataG::Isa(arena.alloc(IsaTemplataG {
        declaration_range: isa.declaration_range,
        impl_name: isa.impl_name,
        sub_kind: isa.sub_kind,
        super_kind: isa.super_kind,
      })),
      ITemplataT::CoordList(list) => {
        let kinds: Vec<KindGT<'s, 't, 'g>> =
          list.kinds.iter().map(|k| self.make_kind_g_groupless(*k, arena)).collect();
        ITemplataG::CoordList(arena.alloc(KindListTemplataG { kinds: arena.alloc_slice_fill_iter(kinds) }))
      }
      ITemplataT::RuntimeSizedArrayTemplate(_) => {
        ITemplataG::RuntimeSizedArrayTemplate(RuntimeSizedArrayTemplateTemplataG {})
      }
      ITemplataT::StaticSizedArrayTemplate(_) => {
        ITemplataG::StaticSizedArrayTemplate(StaticSizedArrayTemplateTemplataG {})
      }
      // A group argument's group is only knowable from its written form (`citizen_args_in`).
      ITemplataT::Group(_) => panic!(
        "vfail: a group template argument with no written group — a deferred case; see \
         docs/plans/group-generic-closures-plan.md"
      ),
      ITemplataT::Function(f) => ITemplataG::Function(
        arena.alloc(FunctionTemplataG { function_template_id: f.function_template_id }),
      ),
      ITemplataT::StructDefinition(d) => {
        ITemplataG::StructDefinition(arena.alloc(StructDefinitionTemplataG {
          struct_template_id: d.struct_template_id,
          tyype: d.tyype,
        }))
      }
      ITemplataT::InterfaceDefinition(d) => {
        ITemplataG::InterfaceDefinition(arena.alloc(InterfaceDefinitionTemplataG {
          interface_template_id: d.interface_template_id,
          tyype: d.tyype,
        }))
      }
      ITemplataT::ImplDefinition(d) => ITemplataG::ImplDefinition(
        arena.alloc(ImplDefinitionTemplataG { impl_template_id: d.impl_template_id }),
      ),
      ITemplataT::ExternFunction(f) => {
        ITemplataG::ExternFunction(arena.alloc(ExternFunctionTemplataG { header: f.header }))
      }
    }
  }

  /// Build a `KindGT` from a `KindT` with no written type. Every borrow layer would need a group it
  /// cannot derive here, so a borrow panics (a deferred case); non-borrows mirror structurally, with
  /// group-free citizen args.
  pub(crate) fn make_kind_g_groupless<'g>(&self, kind: KindT<'s, 't>, arena: &'g Bump) -> KindGT<'s, 't, 'g> {
    match kind {
      KindT::Never(x) => KindGT::Never(NeverGT { from_break: x.from_break }),
      KindT::Void(_) => KindGT::Void(VoidGT),
      KindT::Int(x) => KindGT::Int(IntGT { bits: x.bits }),
      KindT::Bool(_) => KindGT::Bool(BoolGT),
      KindT::Str(_) => KindGT::Str(StrGT),
      KindT::Float(_) => KindGT::Float(FloatGT),
      KindT::USize(_) => KindGT::USize(USizeGT),
      KindT::KindPlaceholder(p) => {
        KindGT::KindPlaceholder(arena.alloc(KindPlaceholderGT { id: p.id, _phantom: PhantomData }))
      }
      KindT::OverloadSet(o) => {
        KindGT::OverloadSet(arena.alloc(OverloadSetGT { env_id: o.env.id(), _phantom: PhantomData }))
      }
      KindT::Struct(s) => KindGT::Struct(self.struct_gt(s, arena)),
      KindT::Interface(i) => KindGT::Interface(self.interface_gt(i, arena)),
      KindT::StaticSizedArray(a) => KindGT::StaticSizedArray(self.ssa_gt(a, arena)),
      KindT::RuntimeSizedArray(a) => KindGT::RuntimeSizedArray(self.rsa_gt(a, arena)),
      KindT::BorrowRef(_) => panic!(
        "vfail: borrow with no derivable group — a deferred case (closure capture / weak-nested / \
         nested reference field); see docs/plans/group-generic-closures-plan.md"
      ),
      KindT::OwnRef(w) => KindGT::OwnRef(arena.alloc(OwnRefGT { inner: self.make_kind_g_groupless(w.inner, arena) })),
      KindT::ShareRef(w) => {
        KindGT::ShareRef(arena.alloc(ShareRefGT { inner: self.make_kind_g_groupless(w.inner, arena) }))
      }
      KindT::WeakRef(w) => {
        KindGT::WeakRef(arena.alloc(WeakRefGT { inner: self.make_kind_g_groupless(w.inner, arena) }))
      }
    }
  }

  /// Cross a `KindGT`'s groups into another frame: rewrite each borrow's `GroupExprG` through `subst`
  /// (a callee group rune → the caller group it was bound to). Structure is unchanged; template args
  /// carry no groups, so they are copied through.
  pub fn substitute_groups<'g>(
    &self,
    kindg: KindGT<'s, 't, 'g>,
    subst: &IndexMap<IRuneS<'s>, GroupExprG<'s, 't, 'g>>,
    arena: &'g Bump,
  ) -> KindGT<'s, 't, 'g> {
    match kindg {
      KindGT::Never(_) | KindGT::Void(_) | KindGT::Int(_) | KindGT::Bool(_) | KindGT::Str(_)
      | KindGT::Float(_) | KindGT::USize(_) | KindGT::KindPlaceholder(_) | KindGT::OverloadSet(_)
      | KindGT::Struct(_) | KindGT::Interface(_) => kindg,
      KindGT::StaticSizedArray(a) => KindGT::StaticSizedArray(arena.alloc(StaticSizedArrayGT {
        name: a.name,
        element_type: self.substitute_groups(a.element_type, subst, arena),
      })),
      KindGT::RuntimeSizedArray(a) => KindGT::RuntimeSizedArray(arena.alloc(RuntimeSizedArrayGT {
        name: a.name,
        element_type: self.substitute_groups(a.element_type, subst, arena),
      })),
      KindGT::BorrowRef(b) => {
        let inner = self.substitute_groups(b.inner, subst, arena);
        let group = subst_group_expr(b.group.group, subst, arena);
        KindGT::BorrowRef(arena.alloc(BorrowRefGT { group: GroupTemplataG { group, kind: inner }, inner }))
      }
      KindGT::OwnRef(w) => KindGT::OwnRef(arena.alloc(OwnRefGT { inner: self.substitute_groups(w.inner, subst, arena) })),
      KindGT::ShareRef(w) => KindGT::ShareRef(arena.alloc(ShareRefGT { inner: self.substitute_groups(w.inner, subst, arena) })),
      KindGT::WeakRef(w) => KindGT::WeakRef(arena.alloc(WeakRefGT { inner: self.substitute_groups(w.inner, subst, arena) })),
    }
  }
}

/// Convert a scout-side `GroupS` to a `GroupExprG`: one path per union member, each walked root to leaf,
/// allocated in `arena`. A group rune carries its own scout identity, so a root needs no frame.
/// `Elements` maps to `ChildElements` (the destructible collection child group — the only kind a written
/// group produces).
pub(crate) fn group_expr_from_group_s<'s, 't, 'g>(
  group: &'s GroupS<'s>,
  arena: &'g Bump,
) -> GroupExprG<'s, 't, 'g> {
  match group {
    GroupS::Union { members } => {
      let paths: Vec<GroupPathG<'s, 't, 'g>> =
        members.iter().flat_map(|m| group_expr_from_group_s(m, arena).iter().copied()).collect();
      arena.alloc_slice_copy(&paths)
    }
    other => {
      let (root, steps, ellipsis) = group_path_from_group_s(other);
      arena.alloc_slice_copy(&[GroupPathG { root, steps: arena.alloc_slice_copy(&steps), ellipsis }])
    }
  }
}

/// One non-union written group as its root, its root-to-leaf steps, and whether it ends in `...`.
fn group_path_from_group_s<'s, 't>(
  group: &'s GroupS<'s>,
) -> (GroupRootG<'s, 't>, Vec<GroupChildStepG<'s>>, bool) {
  match group {
    GroupS::Rune(ru) => (GroupRootG::Rune(ru.rune), vec![], false),
    GroupS::Local(_) => panic!(
      "vfail: a group written as a local name (`in x`) is not yet supported; see \
       docs/plans/group-generic-closures-plan.md"
    ),
    GroupS::Member { base, member_name } => {
      let (root, mut steps, ellipsis) = group_path_from_group_s(base);
      steps.push(GroupChildStepG::Member { member_name: *member_name });
      (root, steps, ellipsis)
    }
    GroupS::Elements { base } => {
      let (root, mut steps, ellipsis) = group_path_from_group_s(base);
      steps.push(GroupChildStepG::ChildElements {});
      (root, steps, ellipsis)
    }
    GroupS::Ellipsis { base } => {
      let (root, steps, _) = group_path_from_group_s(base);
      (root, steps, true)
    }
    GroupS::Union { .. } => panic!("vfail: a union nested inside a group path"),
  }
}

/// Cross a `GroupExprG` into the caller's frame through `subst` (callee group rune → the caller group it
/// was bound to). A path rooted at a rune becomes one path per bound path, that path's steps followed by
/// this one's; a path rooted at an anonymous parameter group or a local passes through. A rune with no
/// binding is a checker bug: every rune a callee's written type or effect names must be bound at the call.
pub(crate) fn subst_group_expr<'s, 't, 'g>(
  group: GroupExprG<'s, 't, 'g>,
  subst: &IndexMap<IRuneS<'s>, GroupExprG<'s, 't, 'g>>,
  arena: &'g Bump,
) -> GroupExprG<'s, 't, 'g> {
  let mut out: Vec<GroupPathG<'s, 't, 'g>> = vec![];
  for path in group {
    match path.root {
      GroupRootG::Rune(rune) => {
        let bound = subst
          .get(&rune)
          .copied()
          .unwrap_or_else(|| panic!("vfail: callee group rune {:?} not bound at this call", rune));
        for p in bound {
          let steps: Vec<GroupChildStepG<'s>> = p.steps.iter().chain(path.steps.iter()).copied().collect();
          out.push(GroupPathG {
            root: p.root,
            steps: arena.alloc_slice_copy(&steps),
            ellipsis: p.ellipsis || path.ellipsis,
          });
        }
      }
      GroupRootG::ParamAnonymousGroup(_) | GroupRootG::Local(_) => out.push(*path),
    }
  }
  arena.alloc_slice_copy(&out)
}

/// A borrow's group when its written type carries no `in g`: the parameter's anonymous group. Only the
/// surface-most borrow of a parameter reaches here; a borrow with no `in g` and no parameter context
/// has no derivable group (a deferred case) and panics.
fn group_anon<'s, 't, 'g>(
  param_name: Option<&'t IVarNameT<'s, 't>>,
  arena: &'g Bump,
) -> GroupExprG<'s, 't, 'g> {
  match param_name {
    Some(name) => single_path(arena, GroupRootG::ParamAnonymousGroup(*name)),
    None => panic!(
      "vfail: borrow with no group and no parameter context — a deferred case; see \
       docs/plans/group-generic-closures-plan.md"
    ),
  }
}
