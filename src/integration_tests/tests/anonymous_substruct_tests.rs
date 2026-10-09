#![allow(unused_imports, dead_code, unused_variables, unreachable_code)]
use crate::integration_tests::tests::run_compilation::test_without_borrow_check;
use crate::keywords::Keywords;
use crate::parse_arena::ParseArena;
use crate::scout_arena::ScoutArena;
use crate::testvm::von::IVonData;
use crate::testvm::von::VonInt;
use crate::testvm::von::VonStr;
use crate::typing::typing_interner::TypingInterner;

#[test]
fn open_interface_constructor() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test_without_borrow_check(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        r"
interface Bipedal {
  func hop(virtual s &Bipedal) int;
}

func hopscotch(s &Bipedal) int {
  s.hop();
  return s.hop();
}

exported func main() int {
   x = Bipedal({ 3 });
  // x is an unnamed substruct which implements Bipedal.

  return hopscotch(&x);
}
",
    );
    match compile.eval_for_kind_primitive_args(Vec::new()).unwrap() {
        IVonData::Int(VonInt { value: 3 }) => {}
        other => panic!("Expected VonInt(3), got {:?}", other),
    }
}

#[test]
fn open_interface_constructor_multiple_methods() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test_without_borrow_check(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        r"
interface Bipedal {
  func hop(virtual s &Bipedal) int;
  func skip(virtual s &Bipedal) int;
}

struct Human {  }
func hop(s &Human) int { return 7; }
func skip(s &Human) int { return 9; }
impl Bipedal for Human;

func hopscotch(s &Bipedal) int {
  s.hop();
  s.skip();
  return s.hop();
}

exported func main() int {
   x = Bipedal({ 3 }, { 5 });
  // x is an unnamed substruct which implements Bipedal.

  return hopscotch(&x);
}
",
    );
    match compile.eval_for_kind_primitive_args(Vec::new()).unwrap() {
        IVonData::Int(VonInt { value: 3 }) => {}
        other => panic!("Expected VonInt(3), got {:?}", other),
    }
}

#[ignore]
#[test]
fn lambda_is_compatible_anonymous_substruct() {
    let compilation_bump = bumpalo::Bump::new();
    let parse_bump = bumpalo::Bump::new();
    let scout_bump = bumpalo::Bump::new();
    let typing_bump = bumpalo::Bump::new();
    let instantiating_bump = bumpalo::Bump::new();
    let parse_arena = ParseArena::new(&parse_bump);
    let scout_arena = ScoutArena::new(&scout_bump);
    let keywords = Keywords::new_for_scout(&scout_arena);
    let parser_keywords = Keywords::new_for_parse(&parse_arena);
    let typing_interner = TypingInterner::new(&typing_bump);
    let mut compile = test_without_borrow_check(
        &compilation_bump,
        &typing_interner, &scout_arena, &keywords, &parser_keywords, &parse_arena,
        &instantiating_bump,
        r"
import castutils.*;

interface AFunction2<R, P1, P2> {
  func __call(virtual this &AFunction2<R, P1, P2>, a P1, b P2) R;
}
exported func main() str {
  func = AFunction2<str, int, bool>((i, b) => { str(i) + str(b) });
  return func(42, true);
}
",
    );
    match compile.eval_for_kind_primitive_args(Vec::new()).unwrap() {
        IVonData::Str(VonStr { value }) if value == "42true" => {}
        other => panic!("Expected VonStr(\"42true\"), got {:?}", other),
    }
}
