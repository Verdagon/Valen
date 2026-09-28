use crate::collect_only_tnode;
use crate::interner::StrI;
use crate::typing::ast::ast::PrototypeT;
use crate::typing::ast::expressions::FunctionCallTE;
use crate::typing::names::names::{FunctionNameT, FunctionTemplateNameT, INameT, IdT};
use crate::utils::code_hierarchy::PackageCoordinate;
use crate::typing::test::rust_interop::drive_helpers::typecheck;
use crate::typing::test::traverse::NodeRefT;

#[test]
fn a_rust_value_bound_to_a_local_gets_a_scope_end_drop() {
  let outcome = typecheck("main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  c = make_counter();
  return 3;
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("drop"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

#[test]
fn a_rust_value_returned_and_discarded_gets_dropped() {
  let outcome = typecheck("main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  make_counter();
  return 4;
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("drop"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}
