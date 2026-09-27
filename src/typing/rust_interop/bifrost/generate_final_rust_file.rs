use crate::typing::rust_interop::bifrost::error::ValenError;
use crate::typing::hinputs_t::HinputsT;
use crate::StrI;

const VALE_OPAQUE_DECL: &str = "pub struct __ValeOpaque<const T: u64>(::core::cell::UnsafeCell<()>, \
   ::std::marker::PhantomData<*mut ()>, ::std::marker::PhantomPinned);\n";

pub fn generate_final_rust_file_source<'s, 't>(
  hinputs: &HinputsT<'s, 't>,
  rust_crates: &[StrI<'s>],
  src_digest: u64,
) -> Result<String, ValenError> {
  for sub_to_edge in hinputs.interface_template_to_sub_citizen_to_edge.values() {
    for edge in sub_to_edge.values() {
      if rust_crates.contains(&edge.super_interface.package_coord.module) {
        unimplemented!();
      }
    }
  }

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
