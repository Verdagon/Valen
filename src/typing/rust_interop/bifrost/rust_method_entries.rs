use crate::StrI;
use crate::typing::compiler::Compiler;
use crate::typing::names::names::*;
use crate::postparsing::ast::LocationInDenizen;
use crate::postparsing::names::*;
use crate::typing::rust_interop::bifrost::importer::GENERATED_RANGE_OFFSET;
use crate::typing::rust_interop::bifrost::importer::resolved_name_of;
use crate::utils::range::CodeLocationS;
use crate::typing::env::i_env_entry::*;

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
  let oracle = compiler.oracles.rust.unwrap(); // unimplemented
  let interner = compiler.typing_interner;
  let owner_name =
      resolved_name_of(struct_template_id.package_coord, struct_template_id.local_name);

  let type_item = oracle.resolve(None, &owner_name).unwrap();

  let mut entries: Vec<(INameT<'s, 't>, IEnvEntryT<'s, 't>)> = Vec::new();
  for (human_name, method_item) in oracle.methods(type_item) {
    let local_name = new_extern_function_name(compiler, human_name);
    let method_id = struct_template_id.add_step(interner, local_name);
    entries.push(
      (local_name, IEnvEntryT::Function(FunctionEnvEntry { template_id: method_id })));
  }
  entries
}

pub fn new_extern_function_name<'s, 'ctx, 't>(
  compiler: &Compiler<'s, 'ctx, 't>,
  human_name: StrI<'s>,
) -> INameT<'s, 't>
where
    's: 't,
{
  let loc = CodeLocationS::internal(compiler.scout_arena, GENERATED_RANGE_OFFSET);
  let name_s = IFunctionDeclarationNameS::FunctionName(FunctionNameS {
    imprecise_name: compiler.scout_arena.intern_code_name(human_name),
    code_location: loc,
    lid: LocationInDenizen { path: &[] },
  });
  match compiler.translate_generic_function_name(name_s) {
    IFunctionTemplateNameT::FunctionTemplate(r) => INameT::FunctionTemplate(r),
    _ => unimplemented!(),
  }
}
