use super::util::assert_borrow_check_passes;

#[test]
fn borrow_check_skips_func_with_not_annotation() {
  assert_borrow_check_passes(&[], r#"
#!BorrowCheck
func call_gen<E, G, g'>(gen &G in g) E
where func(&G, int)E {
  return gen(7);
}
"#);
}
