use super::util::{assert_borrow_error_renders, assert_compiles_clean};

#[test]
fn test_borrow_into_moved_local_rejected() {
  assert_borrow_error_renders(
    r#"
struct Holder { n int; }
func consume<g'>(a &Holder in g, b Holder) { }
exported func main() int {
  h = Holder(1);
  consume(&h, ^h);
  return 0;
}
"#,
    r#"At test:0.vale:6:12:
  consume(&h, ^h);
           ^
Argument 0 borrows into h, but argument 1 moves it, so the borrow would dangle.
"#,
  );
}

#[test]
fn test_field_borrow_into_moved_local_rejected() {
  assert_borrow_error_renders(
    r#"
struct Ship { fuel int; }
struct Holder { ship Ship; }
func consume2<g'>(a &Ship in g, b Holder) { }
exported func main() int {
  h = Holder(Ship(1));
  consume2(&h.ship, ^h);
  return 0;
}
"#,
    r#"At test:0.vale:7:13:
  consume2(&h.ship, ^h);
            ^^^^^^
Argument 0 borrows into h, but argument 1 moves it, so the borrow would dangle.
"#,
  );
}

#[test]
fn test_borrow_into_other_local_with_move_is_clean() {
  assert_compiles_clean(r#"
struct Holder { n int; }
func consume<g'>(a &Holder in g, b Holder) { }
exported func main() int {
  h = Holder(1);
  y = Holder(2);
  consume(&y, ^h);
  return 0;
}
"#);
}
