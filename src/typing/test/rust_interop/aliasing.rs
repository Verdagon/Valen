use crate::collect_only_tnode;
use crate::collect_where_tnode;
use crate::interner::StrI;
use crate::typing::ast::ast::PrototypeT;
use crate::typing::ast::expressions::FunctionCallTE;
use crate::typing::names::names::{FunctionNameT, FunctionTemplateNameT, INameT, IdT};
use crate::utils::code_hierarchy::PackageCoordinate;
use crate::typing::test::rust_interop::drive_helpers::{drive_and_run, typecheck_without_builtins};
use crate::typing::test::traverse::NodeRefT;

#[test]
fn multiple_mutable_aliases_to_one_rust_object_are_legal() {
  let outcome = typecheck_without_builtins("main", r#"
import mycrate.Slot;
exported func main() i64 {
  slot = Slot.new();
  ref_a = &slot;
  ref_b = &slot;
  ref_a.mutate(42i64);
  ref_b.mutate(73i64);
  return ref_a.get();
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    assert!(!collect_where_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("mutate"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    ).is_empty());
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("get"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

#[test]
fn rustc_driven_bin_multiple_mutable_aliases_returns_73() {
  let run = drive_and_run("main", r#"
import mycrate.Slot;
exported func main() i64 {
  slot = Slot.new();
  ref_a = &slot;
  ref_b = &slot;
  ref_a.mutate(42i64);
  ref_b.mutate(73i64);
  return ref_a.get();
}
"#);
  assert_eq!(
    run.process_exit,
    Some(73),
    "did not exit 73 rustc_exit={} process_exit={:?} firings {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
