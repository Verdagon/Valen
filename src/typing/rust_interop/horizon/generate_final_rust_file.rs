// Horizon's final Rust file: bifrost's final file, plus the reverse-direction projections.
//
// When a Vale citizen implements an imported Rust trait, Rust code can call back into Vale through
// it (`run_callback::<MyCb>(&cb)` reaching `<MyCb as Callback>::on_call`). For rustc to monomorphize
// that generic caller and walk to the override, the Vale citizen must exist in the compiled crate as a
// real Rust type with a real trait impl. So for each such edge this emits:
//
//   pub struct MyCb<T0>(__ValeOpaque<HASH>, ::std::marker::PhantomData<(T0)>);
//   impl<T0> ::mycrate::Callback for MyCb<T0> {
//       #[vale::emit_consumer_body(digest = "…")]
//       fn on_call(&self) -> i32 { unreachable!() }
//   }
//
// The struct is wrapper-as-field: it keeps its own `DefId` so the impl resolves, but rustc never sees
// a real field, and the `PhantomData` makes each generic "used" (E0392). The override bodies are
// `#[vale::emit_consumer_body]` placeholders, so horizon's `per_instance_mir` catches them as
// callbacks and the backend emits the real bodies under the same mangled symbols.
//
// Two kinds of citizen get projected, both read off the typed program's edges:
//   - a Vale struct that implements the trait (`impl Callback for MyCb`), under its own name;
//   - the anonymous substruct the anon-interface macro generates for a lambda handed to the trait
//     (`Callback({ 7 })`), under `anon_substruct_rust_name(<trait>)`.
//
// With no edge to a Rust trait, the output is bifrost's final file byte for byte, so bifrost's own
// goldens hold under horizon too.

use crate::interner::StrI;
use crate::postparsing::rules::types::{EffectS, ITypeST, RegionS};
use crate::typing::ast::ast::{EdgeT, PrototypeT};
use crate::typing::compiler::Compiler;
use crate::typing::compiler_outputs::CompilerOutputs;
use crate::typing::hinputs_t::HinputsT;
use crate::typing::names::names::{IFunctionNameT, IInterfaceTemplateNameT, INameT, IStructTemplateNameT, IdT};
use crate::typing::rust_interop::horizon::typeid::anon_substruct_rust_name;
use crate::typing::rust_interop::{typeid, RustOracle, ValenError};
use crate::typing::types::types::KindT;
use crate::typing::typing_interner::TypingInterner;

/// The universal opaque wrapper a Vale type crosses to Rust as; see bifrost's final file, which emits
/// the same spelling. Its three zero-sized fields are auto-trait markers: `UnsafeCell<()>` makes it
/// `!Freeze` (Vale writes a value's bytes through aliasing group borrows while Rust may hold a `&` to
/// it, so rustc must not emit `noalias readonly`), `PhantomData<*mut ()>` makes it `!Send + !Sync`, and
/// `PhantomPinned` makes it `!Unpin`. Its real size comes from the `layout_of` override.
const VALE_OPAQUE_DECL: &str = "pub struct __ValeOpaque<const T: u64>(::core::cell::UnsafeCell<()>, \
   ::std::marker::PhantomData<*mut ()>, ::std::marker::PhantomPinned);\n";

pub fn generate_final_rust_file_source<'s, 't>(
  hinputs: &HinputsT<'s, 't>,
  coutputs: &CompilerOutputs<'s, 't>,
  typing_interner: &TypingInterner<'s, 't>,
  rust_crates: &[StrI<'s>],
  oracle: &dyn RustOracle<'s, 't>,
  src_digest: u64,
) -> Result<String, ValenError> {
  let projections =
    render_projections(hinputs, coutputs, typing_interner, rust_crates, oracle, src_digest)?;

  let mut exported_fn_names: Vec<String> = Vec::new();
  let mut has_main = false;
  for export in hinputs.function_exports.iter() {
    if export.exported_name.as_str() == "main" {
      has_main = true;
    }
    exported_fn_names.push(export.exported_name.as_str().to_string());
  }

  let mut out = String::new();
  out.push_str("#![feature(register_tool)]\n#![register_tool(vale)]\n\n");
  for crate_name in rust_crates {
    out.push_str(&format!("extern crate {};\n", crate_name.as_str()));
  }
  if has_main {
    out.push_str("\nuse std::process::exit;\n");
  }
  out.push_str("\npub const __VALE_STUBS_MARKER: () = ();\n\n");
  out.push_str(VALE_OPAQUE_DECL);
  out.push('\n');
  out.push_str(&projections);
  for name in &exported_fn_names {
    out.push_str(&format!(
      "#[vale::emit_consumer_body(digest = \"{src_digest:016x}\")]\n\
       pub fn __vale_{name}() -> i32 {{\n    unreachable!()\n}}\n\n"
    ));
  }
  if has_main {
    out.push_str("fn main() {\n    exit(__vale_main());\n}\n\n");
  }
  out.push_str(
    "#[inline(never)]\npub unsafe fn __vale_drop<T>(x: *mut T) {\n    core::ptr::drop_in_place(x)\n}\n",
  );
  Ok(out)
}

/// One Vale citizen that implements an imported Rust trait, ready to render.
struct Projection<'s, 't, 'e> {
  /// The citizen's Rust name: its own for a struct, `<trait>__anon` for an anonymous substruct.
  rust_name: String,
  /// The trait's Rust path, e.g. `::mycrate::Callback`.
  trait_path: String,
  /// How many generic parameters the projected struct has.
  generic_count: usize,
  edge: &'e EdgeT<'s, 't>,
}

/// Every projection, rendered: each projected struct once, then each of its trait impls. Sorted by
/// struct then trait, and each impl's methods by name, because the edges come out of `HashMap`s and the
/// file must be byte-stable.
fn render_projections<'s, 't>(
  hinputs: &HinputsT<'s, 't>,
  coutputs: &CompilerOutputs<'s, 't>,
  interner: &TypingInterner<'s, 't>,
  rust_crates: &[StrI<'s>],
  oracle: &dyn RustOracle<'s, 't>,
  src_digest: u64,
) -> Result<String, ValenError> {
  let mut projections: Vec<Projection> = Vec::new();
  for sub_to_edge in hinputs.interface_template_to_sub_citizen_to_edge.values() {
    for edge in sub_to_edge.values() {
      if !rust_crates.contains(&edge.super_interface.package_coord.module) {
        continue;
      }
      let trait_name = citizen_human_name(&edge.super_interface).ok_or_else(|| {
        ValenError::UnsupportedCallbackType(format!("imported trait {:?}", edge.super_interface))
      })?;
      let sub_id = edge.sub_citizen.id();
      let rust_name = match sub_id.local_name {
        INameT::AnonymousSubstruct(_) | INameT::AnonymousSubstructTemplate(_) => {
          anon_substruct_rust_name(trait_name.as_str())
        }
        _ => citizen_human_name(&sub_id)
          .ok_or_else(|| ValenError::UnsupportedCallbackType(format!("implementing citizen {sub_id:?}")))?
          .as_str()
          .to_string(),
      };
      let generic_count = citizen_arity(hinputs, interner, &sub_id).ok_or_else(|| {
        ValenError::UnsupportedCallbackType(format!("no instance of {rust_name} implementing {trait_name}"))
      })?;
      projections.push(Projection {
        rust_name,
        trait_path: rust_spelling_of(oracle, &edge.super_interface, trait_name)?,
        generic_count,
        edge,
      });
    }
  }
  projections.sort_by(|a, b| (&a.rust_name, &a.trait_path).cmp(&(&b.rust_name, &b.trait_path)));

  let mut out = String::new();
  let mut emitted_structs: Vec<&str> = Vec::new();
  for projection in &projections {
    let generic_names: Vec<String> = (0..projection.generic_count).map(|i| format!("T{i}")).collect();
    let generics_clause =
      if generic_names.is_empty() { String::new() } else { format!("<{}>", generic_names.join(", ")) };
    if !emitted_structs.contains(&projection.rust_name.as_str()) {
      emitted_structs.push(projection.rust_name.as_str());
      out.push_str(&format!(
        "pub struct {name}{generics_clause}(__ValeOpaque<{hash}>, \
         ::std::marker::PhantomData<({params})>);\n\n",
        name = projection.rust_name,
        hash = typeid(&projection.rust_name),
        params = generic_names.join(", "),
      ));
    }
    out.push_str(&format!(
      "impl{generics_clause} {} for {}{generics_clause} {{\n",
      projection.trait_path, projection.rust_name
    ));
    let mut methods: Vec<(String, String)> = Vec::new();
    for (abstract_id, override_) in projection.edge.abstract_func_to_override_func.iter() {
      // Per-parameter `&mut` flags come from the abstract method's postparsed effects; the override's
      // typed `KindT` dropped borrow mutability (@BCHATZ).
      let mut_flags = abstract_method_mut_flags(coutputs, interner, abstract_id);
      methods.push(render_override(
        &override_.override_prototype,
        &mut_flags,
        rust_crates,
        oracle,
        src_digest,
      )?);
    }
    methods.sort_by(|a, b| a.0.cmp(&b.0));
    for (_name, rendered) in methods {
      out.push_str(&rendered);
    }
    out.push_str("}\n\n");
  }
  Ok(out)
}

/// The generic arity of a projected citizen, read off its instance in `hinputs.structs` (the edge's
/// sub-citizen may be the arg-less template). For an anonymous substruct that is the interface's
/// generics plus one functor per method. Matched on the interned template id, never on a human name
/// (@ATAFLBZ).
fn citizen_arity<'s, 't>(
  hinputs: &HinputsT<'s, 't>,
  interner: &TypingInterner<'s, 't>,
  sub_id: &IdT<'s, 't>,
) -> Option<usize> {
  let sub_template = Compiler::get_super_template(interner, sub_id);
  for s in hinputs.structs.iter() {
    let id = &s.instantiated_citizen.id;
    if Compiler::get_super_template(interner, id) != sub_template {
      continue;
    }
    return match id.local_name {
      INameT::AnonymousSubstruct(asn) => Some(asn.template_args.len()),
      INameT::Struct(sn) => Some(sn.template_args.len()),
      _ => None,
    };
  }
  None
}

/// The human name of the citizen an id denotes (struct or interface, template or instance form).
fn citizen_human_name<'s, 't>(id: &IdT<'s, 't>) -> Option<StrI<'s>> {
  match id.local_name {
    INameT::InterfaceTemplate(t) => Some(t.human_namee),
    INameT::Interface(inm) => Some(inm.template.human_namee),
    INameT::StructTemplate(t) => Some(t.human_name),
    INameT::Struct(sn) => match sn.template {
      IStructTemplateNameT::StructTemplate(t) => Some(t.human_name),
      _ => None,
    },
    INameT::AnonymousSubstructTemplate(t) => {
      let IInterfaceTemplateNameT::InterfaceTemplate(iface) = t.interface;
      Some(iface.human_namee)
    }
    _ => None,
  }
}

/// How a Rust item is written in the final file, asked of the oracle: its canonical visible path from
/// the crate root, e.g. `::nobiliav::MainLoopCallback`. The final file has no `use` lines, so every
/// Rust item it names is spelled this way. It is not built from the id's coordinate: that is the
/// item's *defining* path (its identity), which can pass through a private module and would then be
/// E0603 here.
fn rust_spelling_of<'s, 't>(
  oracle: &dyn RustOracle<'s, 't>,
  id: &IdT<'s, 't>,
  name: StrI<'s>,
) -> Result<String, ValenError> {
  oracle
    .rust_spelling(id.package_coord, name)
    .ok_or_else(|| ValenError::UnsupportedCallbackType(format!("no Rust spelling for {id:?}")))
}

/// Render one concrete override as a Rust trait-impl method whose body Valen fills, returning
/// `(method name, rendered method)` — the name so the caller can sort. Param 0 is the receiver; each
/// remaining `KindT` renders to its Rust type, named positionally (`_p1`…) since the typed AST carries
/// no source names. `mut_flags[i]` says whether parameter `i` is `&mut`: a borrow's `KindT` is the same
/// shared `BorrowRef` either way, so a `&mut` param re-renders its inner behind `&mut`.
fn render_override<'s, 't>(
  proto: &PrototypeT<'s, 't>,
  mut_flags: &[bool],
  rust_crates: &[StrI<'s>],
  oracle: &dyn RustOracle<'s, 't>,
  src_digest: u64,
) -> Result<(String, String), ValenError> {
  let fname = IFunctionNameT::try_from(proto.id.local_name)
    .map_err(|_| ValenError::UnsupportedCallbackType(format!("override name {:?}", proto.id.local_name)))?;
  let name = fname.template().human_name().as_str().to_string();
  let mut rendered: Vec<String> = Vec::new();
  for (i, kind) in proto.param_types().iter().enumerate() {
    let is_mut = mut_flags.get(i).copied().unwrap_or(false);
    if i == 0 {
      // The receiver: `&mut self` when the trait method churns it, else `&self`.
      rendered.push(if is_mut { "&mut self".to_string() } else { "&self".to_string() });
    } else if is_mut {
      match kind {
        KindT::BorrowRef(b) => {
          rendered.push(format!("_p{i}: &mut {}", render_rust_kind(b.inner, rust_crates, oracle)?))
        }
        other => {
          return Err(ValenError::UnsupportedCallbackType(format!(
            "&mut on non-borrow parameter {i}: {other:?}"
          )))
        }
      }
    } else {
      rendered.push(format!("_p{i}: {}", render_rust_kind(*kind, rust_crates, oracle)?));
    }
  }
  let ret = match proto.return_type {
    KindT::Void(_) => String::new(),
    other => format!(" -> {}", render_rust_kind(other, rust_crates, oracle)?),
  };
  let out = format!(
    "    #[vale::emit_consumer_body(digest = \"{src_digest:016x}\")]\n    \
     fn {name}({}){ret} {{\n        unreachable!()\n    }}\n",
    rendered.join(", ")
  );
  Ok((name, out))
}

/// Per-parameter `&mut` flags for an override, read off the corresponding abstract method's postparsed
/// `FunctionS`. Parameter `i` (including parameter 0, the receiver) is `&mut` iff its `tyype` is a
/// `BorrowRef` whose region group the method's `mut(g)` effects mark mutable — mutability the synthesizer
/// records on the effect clause + region, never on the typed `KindT` (@BCHATZ). An empty result (no
/// postparse found) renders every borrow shared.
fn abstract_method_mut_flags<'s, 't>(
  coutputs: &CompilerOutputs<'s, 't>,
  interner: &TypingInterner<'s, 't>,
  abstract_id: &IdT<'s, 't>,
) -> Vec<bool> {
  let template_id = Compiler::get_super_template(interner, abstract_id);
  match coutputs.peek_postparsed_function(template_id) {
    Some(func) => func.params.iter().map(|p| param_tyype_is_mut(&p.tyype, func.effects)).collect(),
    None => Vec::new(),
  }
}

/// Whether a parameter's `tyype` is a `&mut` borrow: a `BorrowRef` whose region group is marked `mut(g)`
/// by the method's effect clause. The synthesizer gives each borrow parameter one group and pushes
/// `EffectS::Mut(group)` for the `&mut` ones; that same group is the param's region, so they match
/// structurally (`GroupS: PartialEq`), with no human-name keying.
fn param_tyype_is_mut<'s>(tyype: &ITypeST<'s>, effects: &[EffectS<'s>]) -> bool {
  let ITypeST::BorrowRef(b) = tyype else {
    return false;
  };
  let RegionS::Group(group) = b.region else {
    return false;
  };
  effects.iter().any(|e| matches!(e, EffectS::Mut(g) if **g == *group))
}

/// Lower a typed `KindT` to its Rust rendering for a projected override signature: a shared borrow, the
/// scalars a boundary crosses, and a non-generic imported Rust citizen by its visible path. Anything
/// else is not reachable through a supported callback boundary yet.
fn render_rust_kind<'s, 't>(
  kind: KindT<'s, 't>,
  rust_crates: &[StrI<'s>],
  oracle: &dyn RustOracle<'s, 't>,
) -> Result<String, ValenError> {
  match kind {
    KindT::BorrowRef(b) => Ok(format!("&{}", render_rust_kind(b.inner, rust_crates, oracle)?)),
    KindT::Int(i) if i.bits == 32 => Ok("i32".to_string()),
    KindT::Int(i) if i.bits == 64 => Ok("i64".to_string()),
    KindT::Bool(_) => Ok("bool".to_string()),
    KindT::USize(_) => Ok("usize".to_string()),
    KindT::Struct(s) if rust_crates.contains(&s.id.package_coord.module) => rust_citizen_path(oracle, &s.id),
    KindT::Interface(i) if rust_crates.contains(&i.id.package_coord.module) => {
      rust_citizen_path(oracle, &i.id)
    }
    other => Err(ValenError::UnsupportedCallbackType(format!("{other:?}"))),
  }
}

/// A non-generic Rust citizen's visible path. A generic one is an error, since this renderer doesn't
/// lower type arguments yet.
fn rust_citizen_path<'s, 't>(oracle: &dyn RustOracle<'s, 't>, id: &IdT<'s, 't>) -> Result<String, ValenError> {
  let args_are_empty = match id.local_name {
    INameT::Struct(sn) => sn.template_args.is_empty(),
    INameT::Interface(inm) => inm.template_args.is_empty(),
    _ => true,
  };
  let name = citizen_human_name(id).filter(|_| args_are_empty);
  match name {
    Some(name) => rust_spelling_of(oracle, id, name),
    None => Err(ValenError::UnsupportedCallbackType(format!("{id:?}"))),
  }
}
