// Declares imported Rust items as ordinary Vale denizens.
//
// This is the piece that lets everything downstream stop knowing about Rust. A Rust type gets a
// synthesized `StructS` (or `InterfaceS` for an enum or trait) under its crate's package, and an
// outer environment holding its methods — the same sequence `struct_compiler::precompile_struct`
// runs for a Vale struct. After that, method resolution finds Rust methods through the ordinary
// param-environment path, and drop resolves through ordinary overload lookup, with no
// Rust-specific branch in either.
//
// The three entry points are the ones the parent `rust_interop/mod.rs` switches between bifrost and
// horizon, so their signatures must stay exactly bifrost's:
//   - `declare_rust_imports`, called once from `Compiler::evaluate`, is a pure producer: it hands
//     back every postparsed denizen and namespace store the imports need, and core inserts them.
//   - `create_postparsed_function`, called lazily on a postparsed-cache miss, synthesizes one Rust
//     function or method's `FunctionS`.
//   - `rust_method_entries`, called by `struct_compiler` for every citizen, lists a Rust type's
//     lazy method entries for its outer environment.

use crate::postparsing::ast::{InterfaceS, ProgramS};
use crate::typing::compiler::Compiler;
use crate::typing::compiler_error_reporter::{CouldNotPostparseReason, ICompileErrorT};
use crate::typing::env::environment::{
  ImportedItemKind, ResolvedName, TemplatasStoreBuilder,
};
use crate::typing::env::i_env_entry::{
  FunctionEnvEntry, IEnvEntryT, InterfaceEnvEntry, StructEnvEntry,
};
use crate::typing::macros::macros::GeneratedAhtDenizen;
use crate::typing::names::names::*;
use crate::typing::rust_interop::RustImportDeclarations;
use crate::typing::rust_interop::bifrost::oracle::{FuncSignatureR, RustItemId};
use crate::typing::rust_interop::bifrost::rust_method_entries::new_extern_function_name;
use crate::typing::rust_interop::horizon::declarations::{
  synthesize_extern_function, synthesize_extern_interface, synthesize_extern_struct,
  synthesize_extern_trait,
};
use crate::interner::StrI;
use crate::postparsing::ast::FunctionS;
use crate::utils::code_hierarchy::{FileCoordinateMap, PackageCoordinate};
use crate::utils::fx::IndexMap;

/// The package the anonymous-substruct macro's generated denizens live in, for an imported trait.
///
/// The macro names its substruct, constructor, forwarders and impl by nesting them under the
/// interface's id. An imported trait's id is under its crate's package, and those generated
/// denizens are Vale code rather than Rust items, so they get a package of their own that is not
/// a Rust crate — otherwise `in_rust_crate` would claim them and the importer would try to resolve
/// them through the oracle.
pub const RUST_TRAIT_ANON_MODULE: &str = "rust_trait_anon";

/// The postparsed node a synthesized Rust type hands back: a `StructS` for a struct, an `InterfaceS`
/// for an enum or trait. A function seeds nothing — it synthesizes lazily on first call.
enum RustImportSeed<'s, 't> {
  Struct(&'t IdT<'s, 't>, &'s crate::postparsing::ast::StructS<'s>),
  /// A synthesized interface (a Rust trait or enum), plus whether the anonymous-substruct macro can
  /// project it — true iff every abstract method's params and return are expressible in value position.
  /// The import loop fires the macro only when this is true, so a trait whose signatures the macro
  /// can't yet build a `where func` bound for still imports and works through a hand-written
  /// forwarder — the auto-substruct is simply withheld.
  Interface(&'t IdT<'s, 't>, &'s InterfaceS<'s>, bool),
}

/// Declare every Rust import in the program, plus every type reached only as a `Deref` target.
///
/// Each import resolves through the oracle to a canonical name; its env entry goes into a store for
/// the item's crate package, and its postparsed seed into the returned declarations. An imported
/// trait also contributes its abstract methods and, the first time it is seen, the
/// anonymous-substruct macro's generated denizens under `RUST_TRAIT_ANON_MODULE`, so a lambda can
/// be handed to it (`Callback({ 7 })`).
///
/// An import that resolves to nothing is `UnresolvableRustImport`.
pub fn declare_rust_imports<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  file_to_program_s: &FileCoordinateMap<'s, ProgramS<'s>>,
) -> Result<RustImportDeclarations<'s, 't>, ICompileErrorT<'s, 't>>
where
  's: 't,
{
  let interner = compiler.typing_interner;
  let mut declarations = RustImportDeclarations {
    functions: Vec::new(),
    structs: Vec::new(),
    interfaces: Vec::new(),
    impls: Vec::new(),
    namespaces: Vec::new(),
  };
  // Keyed so the same item imported from two files is declared once, and the anon macro runs on
  // its first sighting only.
  let mut seen_interfaces: IndexMap<&'t IdT<'s, 't>, ()> = IndexMap::default();
  let mut per_crate: IndexMap<
    &'s PackageCoordinate<'s>,
    Vec<(INameT<'s, 't>, IEnvEntryT<'s, 't>)>,
  > = IndexMap::default();
  let mut anon_denizen_entries: Vec<(&'t IdT<'s, 't>, IEnvEntryT<'s, 't>)> = Vec::new();

  let mut names: Vec<ResolvedName<'s>> = Vec::new();
  for program in file_to_program_s.file_coord_to_contents.values() {
    for import in program.imports {
      if !compiler.rust_crates.contains(&import.module_name) {
        continue;
      }
      let oracle = compiler
        .oracles
        .rust
        .expect("an import names a Rust crate, but no Rust oracle was provided");
      let name = match oracle.resolve_import(import) {
        Some(name) => name,
        None => {
          let mut path = format!("{}.", import.module_name.0);
          path.extend(import.package_names.iter().map(|s| format!("{}.", s.0)));
          path.push_str(import.importee_name.0);
          return Err(ICompileErrorT::UnresolvableRustImport {
            range: interner.alloc_slice_from_vec(vec![import.range]),
            path,
          });
        }
      };
      if !names.contains(&name) {
        names.push(name);
      }
    }
  }
  // A type reached only as the shared `Deref` target of an imported type is declared as though it
  // were imported, so a `deref`-reached method resolves on it and `deref`'s `&Target` return names
  // it.
  if let Some(oracle) = compiler.oracles.rust {
    for name in oracle.deref_target_imports() {
      if !names.contains(&name) {
        names.push(name);
      }
    }
  }

  for name in names {
    let (local_name, entry, seed) = declare_rust_import(compiler, name);
    match seed {
      Some(RustImportSeed::Struct(id, s)) => declarations.structs.push((id, s)),
      Some(RustImportSeed::Interface(id, i, anon_eligible)) => {
        let first_sighting = seen_interfaces.insert(id, ()).is_none();
        declarations.interfaces.push((id, i));
        for internal_method in i.internal_methods.iter() {
          let (_, method_template_id) = compiler.internal_method_template_id(id, internal_method);
          declarations.functions.push((method_template_id, internal_method));
        }
        if first_sighting && anon_eligible {
          let anon_pkg_coord = compiler
            .scout_arena
            .intern_package_coordinate(compiler.scout_arena.intern_str(RUST_TRAIT_ANON_MODULE), &[]);
          let anon_pkg_id = interner.intern_id(IdValT {
            package_coord: anon_pkg_coord,
            init_steps: &[],
            local_name: INameT::PackageTopLevel(
              interner.intern_package_top_level_name(PackageTopLevelNameT {}),
            ),
          });
          let native_iface_id = anon_pkg_id.add_step(interner, id.local_name);
          for aht_denizen in
            compiler.get_interface_sibling_entries_anonymous_substruct(*native_iface_id, i)
          {
            let denizen_id = aht_denizen.template_id();
            match aht_denizen {
              GeneratedAhtDenizen::Function(fid, f) => declarations.functions.push((fid, f)),
              GeneratedAhtDenizen::Struct(sid, s) => declarations.structs.push((sid, s)),
              GeneratedAhtDenizen::Impl(iid, im) => declarations.impls.push((iid, im)),
            }
            anon_denizen_entries.push((denizen_id, aht_denizen.env_entry()));
          }
        }
      }
      None => {}
    }
    per_crate.entry(name.package_coord).or_default().push((local_name, entry));
  }

  for (coord, entries) in per_crate {
    let package_id = interner.intern_id(IdValT {
      package_coord: coord,
      init_steps: &[],
      local_name: INameT::PackageTopLevel(
        interner.intern_package_top_level_name(PackageTopLevelNameT {}),
      ),
    });
    let mut store = TemplatasStoreBuilder::new(package_id);
    store.add_entries(compiler.scout_arena, entries);
    declarations.namespaces.push((package_id, store.build_in(interner)));
  }

  // The anon macro's denizens are grouped into one store per namespace they nest in, the same way
  // `Compiler::evaluate` groups a Vale package's denizens.
  let pkg_top_level =
    INameT::PackageTopLevel(interner.intern_package_top_level_name(PackageTopLevelNameT {}));
  let mut anon_ns_to_entries: IndexMap<
    &'t IdT<'s, 't>,
    Vec<(INameT<'s, 't>, IEnvEntryT<'s, 't>)>,
  > = IndexMap::default();
  for (name, env_entry) in &anon_denizen_entries {
    let package_id = interner.intern_id(IdValT {
      package_coord: name.package_coord,
      init_steps: name.init_steps,
      local_name: pkg_top_level,
    });
    anon_ns_to_entries.entry(package_id).or_default().push((name.local_name, *env_entry));
  }
  for (package_id, entries) in anon_ns_to_entries {
    let mut store = TemplatasStoreBuilder::new(package_id);
    store.add_entries(compiler.scout_arena, entries);
    declarations.namespaces.push((package_id, store.build_in(interner)));
  }

  Ok(declarations)
}

/// Turn one resolved Rust item into its top-level env entry for its crate's package.
///
/// A **type** becomes an ordinary struct declaration: an eager opaque `StructS` (returned as a seed)
/// plus a `StructEnvEntry`. `IEnvEntryT::Struct` rather than a finished `ITemplataT::Kind` is what
/// makes generic Rust types work — the indexing phase converts it into
/// `ITemplataT::StructDefinition`, the one arm `solve_call_rule` can apply type arguments to. A
/// **free function** becomes an id-only lazy `FunctionEnvEntry`; `create_postparsed_function` builds
/// it on first call. A type's methods and drop are NOT produced here — they are lazy entries in the
/// type's outer environment, added by `rust_method_entries` when `precompile_struct` builds it.
fn declare_rust_import<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  name: ResolvedName<'s>,
) -> (INameT<'s, 't>, IEnvEntryT<'s, 't>, Option<RustImportSeed<'s, 't>>)
where
  's: 't,
{
  let interner = compiler.typing_interner;
  let oracle = compiler.oracles.rust.expect("declare_rust_import called without a rust oracle");
  let item = oracle
    .resolve(None, &name)
    .unwrap_or_else(|| panic!("vfail: a resolved rust import does not resolve: {:?}", name));
  let package_coord = name.package_coord;
  // Every Rust denizen is a top-level denizen of its crate's package, so its template id is that
  // package id plus the denizen's local name. The same id is both the env entry's `template_id` and
  // the postparsed-cache key, so a later lookup can't drift from the seed.
  let package_id = interner.intern_id(IdValT {
    package_coord,
    init_steps: &[],
    local_name: INameT::PackageTopLevel(
      interner.intern_package_top_level_name(PackageTopLevelNameT {}),
    ),
  });
  let human_name = name.importee_name;

  match name.kind {
    ImportedItemKind::Type => {
      let template_name = interner.intern_struct_template_name(StructTemplateNameT { human_name });
      let struct_local_name = INameT::StructTemplate(template_name);
      let struct_s = synthesize_extern_struct(
        compiler,
        package_coord,
        human_name,
        oracle.type_generic_params(item, interner),
      );
      let struct_template_id = package_id.add_step(interner, struct_local_name);
      (
        struct_local_name,
        IEnvEntryT::Struct(StructEnvEntry { template_id: struct_template_id, tyype: struct_s.tyype }),
        Some(RustImportSeed::Struct(struct_template_id, struct_s)),
      )
    }
    ImportedItemKind::Function => {
      let function_local_name = new_extern_function_name(compiler, human_name);
      let function_template_id = package_id.add_step(interner, function_local_name);
      (
        function_local_name,
        IEnvEntryT::Function(FunctionEnvEntry { template_id: function_template_id }),
        None,
      )
    }
    // A Rust enum imports as an opaque sealed interface; a Rust trait as an interface a struct can
    // implement. Both are the interface analog of the `Type` arm; they differ only in whether the
    // interface carries abstract methods — an enum has none, a trait projects its methods so an
    // `impl` can override them.
    ImportedItemKind::Enum | ImportedItemKind::Trait => {
      let template_name =
        interner.intern_interface_template_name(InterfaceTemplateNameT { human_namee: human_name });
      let interface_local_name = INameT::InterfaceTemplate(template_name);
      let (interface_s, anon_eligible) = if name.kind == ImportedItemKind::Trait {
        // The trait's abstract methods, read structurally, become the interface's internal methods,
        // so an `impl Callback for MyCb` overrides each one by ordinary matching. A trait method
        // whose signature declines is dropped from the interface; an override for it then fails to
        // resolve at the impl.
        let method_sigs: Vec<(StrI<'s>, FuncSignatureR<'s, 't>)> = oracle
          .methods(item)
          .into_iter()
          .filter_map(|(method_name, method_item)| {
            let sig = oracle.fn_sig(method_item, interner).ok()?;
            Some((method_name, sig))
          })
          .collect();
        synthesize_extern_trait(compiler, package_coord, human_name, &method_sigs)
      } else {
        // An enum synthesizes a Sealed interface with no abstract methods; the anon macro bails on a
        // Sealed interface anyway, so it is never anon-eligible.
        (
          synthesize_extern_interface(
            compiler,
            package_coord,
            human_name,
            oracle.type_generic_params(item, interner),
          ),
          false,
        )
      };
      let interface_template_id = package_id.add_step(interner, interface_local_name);
      (
        interface_local_name,
        IEnvEntryT::Interface(InterfaceEnvEntry {
          template_id: interface_template_id,
          tyype: interface_s.tyype,
        }),
        Some(RustImportSeed::Interface(interface_template_id, interface_s, anon_eligible)),
      )
    }
  }
}

/// The canonical `ResolvedName` one step of a Rust id carries, or `None` if the step is neither a
/// struct, interface nor function template.
///
/// An interface step names an **enum**. An imported trait is an interface too, but its methods are
/// the interface's internal methods rather than lazy entries in its outer environment, so nothing
/// resolves through a trait step: resolving one as an enum finds nothing, which is what keeps
/// `rust_method_entries` from listing a trait's methods a second time.
fn resolved_name_of<'s, 't>(
  package_coord: &'s PackageCoordinate<'s>,
  local_name: INameT<'s, 't>,
) -> Option<ResolvedName<'s>>
where
  's: 't,
{
  let (importee_name, kind) = match local_name {
    INameT::StructTemplate(t) => (t.human_name, ImportedItemKind::Type),
    INameT::InterfaceTemplate(t) => (t.human_namee, ImportedItemKind::Enum),
    INameT::FunctionTemplate(t) => (t.human_name, ImportedItemKind::Function),
    _ => return None,
  };
  Some(ResolvedName { package_coord, importee_name, kind })
}

/// The oracle item a Rust id names, found by walking the id's steps: each step resolves inside the
/// item the previous one resolved to, starting from the crate root. A free function is a one-step
/// path; a method or drop is two steps, its owner type then itself. `None` if any step names
/// nothing the oracle has.
fn resolve_id<'s, 't>(compiler: &Compiler<'s, '_, 't>, id: &'t IdT<'s, 't>) -> Option<RustItemId>
where
  's: 't,
{
  let oracle = compiler.oracles.rust?;
  let mut current: Option<RustItemId> = None;
  for step in id.steps() {
    let step_name = resolved_name_of(id.package_coord, step)?;
    current = Some(oracle.resolve(current, &step_name)?);
  }
  current
}

/// Build a lazily-registered Rust function's `FunctionS` on its first lookup, called by
/// `Compiler::illuminate_function` on a postparsed-cache miss. Recovers the oracle item by walking
/// the id's steps, queries its signature, and synthesizes the declaration. A method and a drop go
/// the same way as a free function: the oracle answers `fn_sig` for all three.
///
/// Outer `None` when the id is not a Rust function the oracle knows — a genuine bug the caller's
/// vfail surfaces. `Some(Err(reason))` when the signature declines: a called function whose type
/// Vale cannot name, which the caller turns into a `CouldNotPostparseFunction` compile error.
pub fn create_postparsed_function<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  template_id: &'t IdT<'s, 't>,
) -> Option<Result<&'s FunctionS<'s>, CouldNotPostparseReason>>
where
  's: 't,
{
  if !compiler.in_rust_crate(template_id) {
    return None;
  }
  let oracle = compiler.oracles.rust?;
  let human_name = match template_id.local_name {
    INameT::FunctionTemplate(r) => r.human_name,
    _ => return None,
  };
  let item = resolve_id(compiler, template_id)?;
  let sig = match oracle.fn_sig(item, compiler.typing_interner) {
    Ok(sig) => sig,
    Err(reason) => return Some(Err(reason)),
  };
  let function_s =
    synthesize_extern_function(compiler, template_id.package_coord, human_name, &sig)?;
  Some(Ok(function_s))
}

/// The id-only method entries that belong in a Rust type's outer environment (Vale's home for a type's
/// methods and associated functions), drop included. Chained into `precompile_struct`'s outer store for
/// every citizen; empty for a Vale citizen. Each is lazy — no `fn_sig`, no synthesis — like a
/// lazily-imported free function, and synthesizes on first call through `create_postparsed_function`.
/// Its template id nests under the type's (`struct_template_id.add_step(method_name)`), the shape a
/// Vale internal method uses; the citizen-compile loop skips these Rust-backed entries so they are not
/// force-compiled.
pub fn rust_method_entries<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  struct_template_id: &'t IdT<'s, 't>,
) -> Vec<(INameT<'s, 't>, IEnvEntryT<'s, 't>)>
where
  's: 't,
{
  if !compiler.in_rust_crate(struct_template_id) {
    return Vec::new();
  }
  let Some(oracle) = compiler.oracles.rust else { return Vec::new() };
  let Some(type_item) = resolve_id(compiler, struct_template_id) else { return Vec::new() };
  let interner = compiler.typing_interner;
  oracle
    .methods(type_item)
    .into_iter()
    .map(|(method_name, _method_item)| {
      let local_name = new_extern_function_name(compiler, method_name);
      let method_id = struct_template_id.add_step(interner, local_name);
      (local_name, IEnvEntryT::Function(FunctionEnvEntry { template_id: method_id }))
    })
    .collect()
}
