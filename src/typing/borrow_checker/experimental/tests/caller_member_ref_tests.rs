use super::util::{assert_borrow_check_gives_error, assert_borrow_check_passes};

// Ensures a caller keeps its reference to an element of a Valen-native array across a call that
// declares `mut` on the element's group and only writes a field. A member write destroys nothing,
// so a reference to the churned group itself survives. The same program over an imported Rust
// `Vec` is rejected, because an accessor's borrow covers everything under the container; see
// src/typing/test/rust_interop/horizon/speed/valen/caller_ref_across_mutating_callee.rs.
#[test]
fn caller_keeps_member_ref_across_mut_callee() {
  assert_borrow_check_passes(&["arrays", "arith", "drop", "implicit_clone"], r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
struct Entity { hp int; }
struct Level { entities []Entity; }
func observe<T>(x &T) { }
func strike<g'>(e &Entity in g) mut(g) {
  set e.hp = 1;
}
func siege<g'>(level &Level in g) mut(level.entities[]) {
  e = &level.entities[0];
  strike(e);
  observe(e);
  strike(e);
  observe(e);
}
exported func main() int {
  level = Level(Array<Entity>(2));
  siege(&level);
  return 0;
}
"#);
}

// Ensures a borrow local that died at a churn is usable again after `set` re-derives it, in
// straight-line code. The design says a stale reference may be re-derived immediately after the
// mutation.
#[test]
fn reassigned_borrow_is_usable_after_churn() {
  assert_borrow_check_passes(&["arrays", "arith", "drop", "implicit_clone"], r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  e = &arr[0];
  churn(&arr);
  set e = &arr[0];
  observe(e);
  churn(&arr);
  set e = &arr[0];
  observe(e);
  return 0;
}
"#);
}

// Ensures the same holds around a loop's back edge: the borrow is re-derived after the churn in
// each iteration, so it is live at the top of the next one.
#[test]
fn reassigned_borrow_is_usable_after_loop_back_edge() {
  assert_borrow_check_passes(&["arrays", "arith", "drop", "implicit_clone"], r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  e = &arr[0];
  while (false) {
    observe(e);
    churn(&arr);
    set e = &arr[0];
  }
  return 0;
}
"#);
}

// Ensures the same for a borrow returned by an accessor (`in r...`), handed to a callee that
// declares `mut` on its group, in straight-line code. This is the shape of
// caller_ref_across_mutating_callee over an imported Rust `Vec`.
#[test]
fn reassigned_accessor_borrow_is_usable_after_mut_callee() {
  assert_borrow_check_passes(&["arrays", "arith", "drop", "implicit_clone"], r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func strike<r'>(x &int in r) mut(r) { }
func peek<r'>(a &[]int in r) &int in r... { return &a[0]; }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  e = peek(&arr);
  strike(e);
  set e = peek(&arr);
  observe(e);
  strike(e);
  set e = peek(&arr);
  observe(e);
  return 0;
}
"#);
}

// Ensures the same around a loop's back edge.
#[test]
fn reassigned_accessor_borrow_is_usable_after_loop_back_edge() {
  assert_borrow_check_passes(&["arrays", "arith", "drop", "implicit_clone"], r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func strike<r'>(x &int in r) mut(r) { }
func peek<r'>(a &[]int in r) &int in r... { return &a[0]; }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  e = peek(&arr);
  while (false) {
    strike(e);
    set e = peek(&arr);
    observe(e);
  }
  return 0;
}
"#);
}

// A re-derivation does NOT make the borrow live across the back edge if it is churned again AFTER
// the `set` (before the back edge): on the next iteration the top use reads the churned value.
#[test]
fn loop_borrow_churned_after_reset_is_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  e = &arr[0];
  while (false) {
    observe(e);
    set e = &arr[0];
    churn(&arr);
  }
  return 0;
}
"#,
    r#"At test:0.vale:10:13:
    observe(e);
            ^
Used a borrow after invalidated.
Invalidated at test:0.vale:12:5:
    churn(&arr);
    ^^^^^
"#,
  );
}

// A borrow spared by the back edge (re-derived in the body) must still be checked against the code
// *before* the loop: iteration 1 reads the pre-loop value, which a pre-loop churn has invalidated.
#[test]
fn loop_borrow_stale_from_before_loop_is_rejected() {
  assert_borrow_check_gives_error(
    &["arrays", "arith", "drop", "implicit_clone"],
    r#"
import v.builtins.arrays.*;
import v.builtins.drop.*;
func churn<r'>(a &[]int in r) mut(r) { }
func observe<T>(x &T) { }
exported func main() int {
  arr = Array<int>(3);
  e = &arr[0];
  churn(&arr);
  while (false) {
    observe(e);
    churn(&arr);
    set e = &arr[0];
  }
  return 0;
}
"#,
    r#"At test:0.vale:11:13:
    observe(e);
            ^
Used a borrow after invalidated.
Invalidated at test:0.vale:9:3:
  churn(&arr);
  ^^^^^
"#,
  );
}
