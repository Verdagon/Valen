#![allow(unused_imports, dead_code, unused_variables, unreachable_code)]
use bumpalo::Bump;
use crate::builtins::builtins::empty_v_builtins_stub;
use crate::code_source::{CodeSource, Source};
use crate::interner::StrI;
use crate::keywords::Keywords;
use crate::parse_arena::ParseArena;
use crate::scout_arena::ScoutArena;
use crate::tests::tests::new_test_code_map;
use crate::tests::tests::new_test_package_source;
use crate::typing::compiler_error_reporter::ICompileErrorT;
use crate::typing::infer_compiler::IConclusionResolveError;
use crate::typing::infer_compiler::IResolvingError;
use crate::typing::names::names::{FunctionNameT, FunctionTemplateNameT, INameT, IdT};
use crate::typing::overload_resolver::IFindFunctionFailureReason;
use crate::typing::test::compiler_test_compilation::compiler_test_compilation;
use crate::typing::test::compiler_test_compilation::compiler_test_compilation_without_borrow_check;
use crate::typing::test::humanize_helper::{assert_humanized_eq, humanize_compile_error};
use crate::typing::types::types::KindT;
use crate::typing::typing_interner::TypingInterner;

#[test]
#[ignore]
fn lambda_body_type_matches_anonymous_substruct_return_type() {
  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let typing_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code = r"
interface AFunction1<P> {
  func __call(virtual this &AFunction1<P>, a P) int;
}
exported func main() {
  arr = AFunction1<int>((_) => { 4 });
}
";
  let code_source = CodeSource::new(vec![new_test_code_map(&parse_arena, code)]);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  let _coutputs = compile.expect_compiler_outputs();
}

#[test]
#[ignore]
fn minimal_anonymous_substruct_construction() {
  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let typing_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code = r"
interface AFn { func __call(virtual this &AFn); }
exported func main() { AFn(() => { }); }
";
  let code_source = CodeSource::new(vec![new_test_code_map(&parse_arena, code)]);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  let _coutputs = compile.expect_compiler_outputs();
}

#[test]
fn basic_ifunction1_anonymous_subclass() {
  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let typing_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code = r"
import ifunction.ifunction1.*;

exported func main() int {
  f = IFunction1<int, int>({^_});
  return (^f)(7);
}
";
  let code_source = CodeSource::new(vec![
    new_test_code_map(&parse_arena, code),
    new_test_package_source(&parse_arena, "ifunction.ifunction1"),
  ]);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation_without_borrow_check(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  let _coutputs = compile.expect_compiler_outputs();
}

#[test]
fn native_anon_substruct_with_multi_param_abstract_method_compiles() {
  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let typing_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code = r"
interface TwoArg {
  func apply(virtual self &TwoArg, a int, b int) int;
}

exported func main() int {
  f = TwoArg((a, b) => { 5 });
  return f.apply(3, 4);
}
";
  let code_source = CodeSource::new(vec![new_test_code_map(&parse_arena, code)]);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation_without_borrow_check(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  let _coutputs = compile.expect_compiler_outputs();
}

#[test]
fn native_anon_substruct_with_concrete_citizen_borrow_param_compiles() {
  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let typing_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code = r"
#!DeriveStructDrop
struct W { }

interface IH {
  func handle(virtual self &IH, w &W) void;
}

exported func main() {
  h = IH((w) => { });
}
";
  let code_source = CodeSource::new(vec![
    Source::builtin_module(&parse_arena, &parser_keywords, "drop"),
    new_test_code_map(&parse_arena, code),
    Source::Fn(empty_v_builtins_stub),
  ]);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation_without_borrow_check(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  let _coutputs = compile.expect_compiler_outputs();
}

#[test]
fn regular_open_interface_and_struct_no_anonymous_substruct() {
  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let typing_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code = r"
#!DeriveAnonymousSubstruct
interface Opt { }

struct Some { x int; }
impl Opt for Some;
";
  let code_source = CodeSource::new(vec![new_test_code_map(&parse_arena, code)]);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  let coutputs = compile.expect_compiler_outputs();
  let drop_func_names: Vec<_> = coutputs
    .functions
    .iter()
    .map(|f| f.header.id)
    .filter_map(|f| match f {
      id @ IdT {
        local_name:
          INameT::Function(FunctionNameT {
            template: FunctionTemplateNameT { human_name: StrI("drop"), .. },
            ..
          }),
        ..
      } => Some(id),
      _ => None,
    })
    .collect();
  assert_eq!(drop_func_names.len(), 2);
}

#[test]
fn basic_interface_anonymous_subclass() {
  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let typing_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code = r"
interface Bork {
  func bork(virtual self &Bork) int;
}

exported func main() int {
  f = Bork({ 7 });
  return f.bork();
}
";
  let code_source = CodeSource::new(vec![new_test_code_map(&parse_arena, code)]);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation_without_borrow_check(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  compile.expect_compiler_outputs();
}

#[test]
fn integer_is_compatible_with_interface_anonymous_substruct() {
  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let typing_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code = r#"
import v.builtins.drop.*;
interface AFunction2<R, P1> {
  func doCall(virtual this &AFunction2<R, P1>, a P1) R;
}
func __call(x6 &int, x42 int)str { "hi" }
exported func main() str {
  func = AFunction2<str, int>(6);
  return func.doCall(42);
}
"#;
  let code_source = CodeSource::new(vec![
    Source::builtin_module(&parse_arena, &parser_keywords, "drop"),
    new_test_code_map(&parse_arena, code),
    Source::Fn(empty_v_builtins_stub),
  ]);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation_without_borrow_check(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  compile.expect_compiler_outputs();
}

#[test]
fn lambda_is_compatible_with_interface_anonymous_substruct() {
  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let typing_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code = r"
import v.builtins.str.*;

interface AFunction2<R, P1> {
  func __call(virtual this &AFunction2<R, P1>, a P1) R;
}
exported func main() str {
  func = AFunction2<str, int>((i) => { str(i) });
  return func(42);
}
";
  let code_source = CodeSource::new(vec![
    Source::builtin_module(&parse_arena, &parser_keywords, "str"),
    Source::builtin_module(&parse_arena, &parser_keywords, "drop"),
    Source::builtin_module(&parse_arena, &parser_keywords, "implicit_clone"),
    new_test_code_map(&parse_arena, code),
    Source::Fn(empty_v_builtins_stub),
  ]);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation_without_borrow_check(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  compile.expect_compiler_outputs();
}

#[test]
#[ignore]
fn lambda_body_type_mismatches_anonymous_substruct_return_type() {
  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let typing_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code = r"
interface AFunction1<P> {
  func __call(virtual this &AFunction1<P>, a P) int;
}
exported func main() {
  arr = AFunction1<int>((_) => { true });
}
";
  let code_source = CodeSource::new(vec![new_test_code_map(&parse_arena, code)]);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );

  let err = compile
    .get_compiler_outputs()
    .err()
    .unwrap_or_else(|| panic!("expected Err(CouldntFindFunctionToCallT), got Ok"));
  match &err {
    ICompileErrorT::CouldntFindFunctionToCallT { fff, .. } => {
      let rejection_reasons: Vec<&IFindFunctionFailureReason<'_, '_>> =
        fff.rejected_callee_to_reason.iter().map(|p| &p.1).collect();
      match rejection_reasons.as_slice() {
                [IFindFunctionFailureReason::FindFunctionResolveFailure {
                    reason: IResolvingError::ResolvingResolveConclusionError(boxed),
                }] => {
                    match boxed.as_ref() {
                        IConclusionResolveError::ReturnTypeConflictInConclusionResolve {
                            expected_return_type: KindT::Int(_),
                            actual: actual_prototype,
                            ..
                        } => {
                            match actual_prototype.return_type {
                                KindT::Bool(_) => {}
                                other => panic!("expected Bool, got {:?}", other),
                            }
                        }
                        other => panic!("expected ReturnTypeConflictInConclusionResolve(_, Int, _), got {:?}", other),
                    }
                }
                other => panic!("expected Vec[FindFunctionResolveFailure(ResolvingResolveConclusionError(...))], got {:?}", other),
            }
    }
    other => panic!("expected CouldntFindFunctionToCallT, got Err({:?})", other),
  }
  assert_humanized_eq(
    &humanize_compile_error(&mut compile, err),
    r#"At test:0.vale:5:1:
exported func main() {
At test:0.vale:6:9:
  arr = AFunction1<int>((_) => { true });
Couldn't find a suitable function AFunction1(main.λC:test:0.vale:6:25<>). Rejected candidates:

Candidate 1 (of 1): test:0.vale:2:1:
CodeLocationS { file: FileCoordinate { package_coord: PackageCoordinate { module: "test", packages: [] }, filepath: "0.vale" }, offset: 1 }
Found function: main.λC:test:0.vale:6:25.λF:test:0.vale:6:25<i32>(&main.λC:test:0.vale:6:25<>, i32) which returns bool but expected return type of i32


"#,
  );
}

#[test]
fn zero_method_anonymous_substruct() {
  let parse_bump = Bump::new();
  let scout_bump = Bump::new();
  let typing_bump = Bump::new();
  let parse_arena = ParseArena::new(&parse_bump);
  let scout_arena = ScoutArena::new(&scout_bump);
  let keywords = Keywords::new_for_scout(&scout_arena);
  let parser_keywords = Keywords::new_for_parse(&parse_arena);
  let code = r#"
interface MyInterface {}
exported func main() {
  x = MyInterface();
}
"#;
  let code_source = CodeSource::new(vec![new_test_code_map(&parse_arena, code)]);
  let typing_interner = TypingInterner::new(&typing_bump);
  let mut compile = compiler_test_compilation_without_borrow_check(
    &typing_interner,
    &scout_arena,
    &keywords,
    &parser_keywords,
    &parse_arena,
    &code_source,
  );
  compile.expect_compiler_outputs();
}
