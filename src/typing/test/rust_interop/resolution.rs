use crate::collect_only_tnode;
use crate::collect_where_tnode;
use crate::interner::StrI;
use crate::typing::ast::ast::PrototypeT;
use crate::typing::ast::expressions::FunctionCallTE;
use crate::typing::names::names::{
  FunctionNameT, FunctionTemplateNameT, INameT, IStructTemplateNameT, IdT, StructNameT,
  StructTemplateNameT,
};
use crate::typing::types::types::{KindT, StructTT};
use crate::utils::code_hierarchy::PackageCoordinate;
use crate::typing::test::rust_interop::drive_helpers::typecheck;
use crate::typing::test::traverse::NodeRefT;

#[test]
fn calls_a_rust_function_returning_unit() {
  let outcome = typecheck("main", r#"
import mycrate.do_nothing;
exported func main() int {
  do_nothing();
  return 8;
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
                        template: FunctionTemplateNameT { human_name: StrI("do_nothing"), .. }, ..
                    }), ..
                }, ..
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}

#[test]
fn vale_source_can_name_a_rust_type() {
  let outcome = typecheck("main", r#"
import mycrate.make_counter;
import mycrate.Counter;
exported func main() int {
  return value_of(make_counter());
}
func value_of(c Counter) int {
  return 11;
}
"#, |_| ());
  outcome.expect_compiled();
}

#[test]
fn a_program_using_no_rust_items_compiles_with_an_oracle_present() {
  let outcome = typecheck("main", r#"
import mycrate.add_two_numbers;
import mycrate.Counter;
import mycrate.make_counter;
exported func main() int {
  return 17;
}
"#, |hinputs| {
    let main = hinputs.lookup_function_by_str("main");
    assert!(collect_where_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("mycrate"), .. }, ..
                }, ..
            }, ..
        }) => Some(())
    ).is_empty());
  });
  outcome.expect_compiled();
}

#[test]
fn two_crates_exporting_the_same_short_name_stay_distinct() {
  let outcome = typecheck("two_crates", r#"
import mycrate.Widget;
import othercrate.Widget;
import mycrate.make_widget;
import othercrate.make_other_widget;
exported func main() int {
  a = make_widget();
  b = make_other_widget();
  return 5;
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
                        template: FunctionTemplateNameT { human_name: StrI("make_widget"), .. }, ..
                    }), ..
                },
                return_type: KindT::Struct(StructTT {
                    id: IdT {
                        package_coord: PackageCoordinate { module: StrI("mycrate"), .. },
                        local_name: INameT::Struct(StructNameT {
                            template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                human_name: StrI("Widget"), ..
                            }), ..
                        }), ..
                    }, ..
                }),
            }, ..
        }) => Some(())
    );
    collect_only_tnode!(
        NodeRefT::FunctionDefinition(main),
        NodeRefT::FunctionCall(FunctionCallTE {
            callable: PrototypeT {
                id: IdT {
                    package_coord: PackageCoordinate { module: StrI("othercrate"), .. },
                    local_name: INameT::Function(FunctionNameT {
                        template: FunctionTemplateNameT { human_name: StrI("make_other_widget"), .. }, ..
                    }), ..
                },
                return_type: KindT::Struct(StructTT {
                    id: IdT {
                        package_coord: PackageCoordinate { module: StrI("othercrate"), .. },
                        local_name: INameT::Struct(StructNameT {
                            template: IStructTemplateNameT::StructTemplate(StructTemplateNameT {
                                human_name: StrI("Widget"), ..
                            }), ..
                        }), ..
                    }, ..
                }),
            }, ..
        }) => Some(())
    );
  });
  outcome.expect_compiled();
}
