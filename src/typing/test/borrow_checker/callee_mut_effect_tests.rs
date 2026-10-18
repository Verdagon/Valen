use super::util::assert_borrow_check_gives_error;

#[test]
fn test_callee_mut_self_param_effect_invalidates_inner_ref_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
#!DeriveStructDrop
struct Entity { hp int; buffs []int; }
func advance(self &Entity mut) mut(self) { }
func observe<T>(x &T) { }
exported func step(entity &Entity mut) {
  buff = &entity.buffs[0];
  entity.advance();
  observe(buff);
}
"#,
    r#"At test:0.vale:11:11:
  observe(buff);
          ^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:9:
  entity.advance();
        ^^^^^^^^^^
"#,
  );
}

#[test]
fn test_callee_mut_self_member_effect_invalidates_inner_ref_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
#!DeriveStructDrop
struct Inner { buffs []int; }
#!DeriveStructDrop
struct Outer { inner Inner; }
func churn(self &Outer mut) mut(self.inner) { }
func observe<T>(x &T) { }
exported func step(o &Outer mut) {
  buff = &o.inner.buffs[0];
  o.churn();
  observe(buff);
}
"#,
    r#"At test:0.vale:13:11:
  observe(buff);
          ^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:12:4:
  o.churn();
   ^^^^^^^^
"#,
  );
}

#[test]
fn test_callee_mut_self_elements_effect_invalidates_element_ref_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
#!DeriveStructDrop
struct Bag { items []int; }
func churn(self &Bag mut) mut(self.items[]) { }
func observe<T>(x &T) { }
exported func step(b &Bag mut) {
  elem = &b.items[0];
  b.churn();
  observe(elem);
}
"#,
    r#"At test:0.vale:11:11:
  observe(elem);
          ^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:4:
  b.churn();
   ^^^^^^^^
"#,
  );
}

#[test]
fn test_callee_mut_self_ellipsis_effect_invalidates_inner_ref_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
#!DeriveStructDrop
struct Entity { hp int; buffs []int; }
func churn(self &Entity mut) mut(self...) { }
func observe<T>(x &T) { }
exported func step(entity &Entity mut) {
  buff = &entity.buffs[0];
  entity.churn();
  observe(buff);
}
"#,
    r#"At test:0.vale:11:11:
  observe(buff);
          ^^^^
Used a borrow after invalidated.
Invalidated at test:0.vale:10:9:
  entity.churn();
        ^^^^^^^^
"#,
  );
}
