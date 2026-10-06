use super::util::assert_borrow_check_passes;

// A guard that holds a borrow of a transaction and rolls it back when dropped. The caller keeps
// using the transaction directly while the guard lives; in Rust the guard's `&mut Txn` would force
// every use through the guard.
#[test]
fn test_caller_uses_transaction_while_rollback_guard_lives() {
  assert_borrow_check_passes(&[], r#"
struct Txn { state int; }
func exec<t'>(txn &Txn in t, op int) mut(t) { set txn.state = op; }
func rollback<t'>(txn &Txn in t) mut(t) { set txn.state = 0; }
#!DeriveStructDrop
struct Rollbacker<t'> { txn &Txn in t; }
func drop<t'>(r Rollbacker<t>) mut(t) {
  [txn] = ^r;
  txn.rollback();
}
func do_work<t'>(txn &Txn in t) mut(t) {
  r = Rollbacker(txn);
  txn.exec(1);
  txn.exec(2);
}
exported func main() int {
  txn = Txn(5);
  do_work(&txn);
  return txn.state;
}
"#);
}
