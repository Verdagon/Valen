//! End-to-end: the backend emits LLVM `noalias` on a borrow parameter the borrow checker proved is
//! the sole reference into its group, and omits it where two parameters share a group. Asserts against
//! the pre-optimization dump (`build.ll`) so a redundant-attribute pass can't hide the result.

use crate::end_to_end_tests::{compile_inline, compile_program, programs_dir};
use std::fs;

/// The `define` line for the function whose mangled LLVM name contains `needle`.
fn define_line<'a>(ll: &'a str, needle: &str) -> &'a str {
  ll.lines()
    .find(|l| l.contains("define") && l.contains(needle))
    .unwrap_or_else(|| panic!("no `define` line for `{needle}` in:\n{ll}"))
}

/// The whole `define … { … }` body of the function whose mangled LLVM name contains `needle`, from its
/// `define` line through the closing `}`.
fn function_body(ll: &str, needle: &str) -> String {
  let lines: Vec<&str> = ll.lines().collect();
  let start = lines
    .iter()
    .position(|l| l.contains("define") && l.contains(needle))
    .unwrap_or_else(|| panic!("no `define` line for `{needle}` in:\n{ll}"));
  let rel_end = lines[start..]
    .iter()
    .position(|l| l.trim() == "}")
    .unwrap_or_else(|| panic!("no closing `}}` for `{needle}` after its define in:\n{ll}"));
  lines[start..=start + rel_end].join("\n")
}

/// The `declare` line (a bodiless extern) whose mangled LLVM name contains `needle`.
fn declare_line<'a>(ll: &'a str, needle: &str) -> &'a str {
  ll.lines()
    .find(|l| l.contains("declare") && l.contains(needle))
    .unwrap_or_else(|| panic!("no `declare` line for `{needle}` in:\n{ll}"))
}

/// The `attributes #N = { … }` group referenced by the `#N` token on `line`. Function-level attributes
/// (`nounwind`, `willreturn`, …) live in these groups, not inline on the declare/define line — so a
/// `line.contains("nounwind")` check would wrongly fail.
fn attr_group<'a>(ll: &'a str, line: &str) -> &'a str {
  let tag = line
    .split_whitespace()
    .find(|t| t.starts_with('#'))
    .unwrap_or_else(|| panic!("no `#N` attribute-group token on line:\n{line}"));
  let prefix = format!("attributes {tag} = {{");
  ll.lines()
    .find(|l| l.starts_with(&prefix))
    .unwrap_or_else(|| panic!("no `attributes {tag} = {{ … }}` group in:\n{ll}"))
}

#[test]
fn sole_borrow_param_gets_noalias_same_group_does_not() {
  let cp = compile_inline(
    r#"
struct Spaceship { fuel int; }
func getFuel(a &Spaceship) int {
  return __copy_prim(a.fuel);
}
func pairSum<g'>(a &Spaceship in g, b &Spaceship in g) int {
  return __copy_prim(a.fuel);
}
exported func main() int {
  ship = Spaceship(42);
  other = Spaceship(7);
  return (&ship).getFuel() + pairSum(&ship, &other);
}
"#,
    |opts| opts.print_llvmir = true,
  );
  let ll = fs::read_to_string(cp.cwd.join("build.ll")).expect("read build.ll");

  // The sole-borrow parameter is the only way to reach its group, so it is restrict.
  assert!(
    define_line(&ll, "getFuel").contains("noalias"),
    "getFuel's borrow param should be noalias:\n{}",
    define_line(&ll, "getFuel"),
  );
  // Two parameters in the same group may alias each other, so neither is noalias.
  assert!(
    !define_line(&ll, "pairSum").contains("noalias"),
    "pairSum's same-group params must not be noalias:\n{}",
    define_line(&ll, "pairSum"),
  );
}

/// Block-scoped restrict (Phase B): `a` and `b` share group `g`, so neither is whole-function `noalias`.
/// But in the trailing span only `a` reaches into `g` (across the opaque `nothing()`), so `a` is the
/// sole reference there — the checker records those loads as accesses into `g`, and the backend tags
/// them with `!alias.scope`.
#[test]
fn sole_reference_region_loads_get_alias_scope_metadata() {
  let cp = compile_inline(
    r#"
struct Ship { fuel int; }
func nothing() { }
func do_things<g'>(a &Ship in g, b &Ship in g) int {
  p = __copy_prim(a.fuel);
  q = __copy_prim(b.fuel);
  x = __copy_prim(a.fuel);
  nothing();
  y = __copy_prim(a.fuel);
  return p + q + x + y;
}
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  return do_things(&s1, &s2);
}
"#,
    |opts| opts.print_llvmir = true,
  );
  let ll = fs::read_to_string(cp.cwd.join("build.ll")).expect("read build.ll");

  // The tail span's loads of `a.fuel` belong to `g`'s alias scope.
  let body = function_body(&ll, "do_things");
  assert!(
    body.contains("!alias.scope"),
    "do_things' loads into `g` should carry !alias.scope:\n{body}",
  );
}

/// The opaque `nothing()` call takes no arguments, so it can reach no group and is proven not to touch
/// `g` — it carries `!noalias {g}`, which is what lets LLVM keep `a`'s accesses coalescable across it.
#[test]
fn sole_reference_region_call_gets_noalias() {
  let cp = compile_inline(
    r#"
struct Ship { fuel int; }
func nothing() { }
func do_things<g'>(a &Ship in g, b &Ship in g) int {
  p = __copy_prim(a.fuel);
  q = __copy_prim(b.fuel);
  x = __copy_prim(a.fuel);
  nothing();
  y = __copy_prim(a.fuel);
  return p + q + x + y;
}
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  return do_things(&s1, &s2);
}
"#,
    |opts| opts.print_llvmir = true,
  );
  let ll = fs::read_to_string(cp.cwd.join("build.ll")).expect("read build.ll");

  // The `nothing()` call reaches no group, so it is tagged !noalias and LLVM keeps `a`'s loads coalescable.
  let body = function_body(&ll, "do_things");
  let call_line = body
    .lines()
    .find(|l| l.contains("call") && l.contains("@nothing"))
    .unwrap_or_else(|| panic!("no `nothing()` call line in do_things:\n{body}"));
  assert!(
    call_line.contains("!noalias"),
    "the nothing() call should carry !noalias:\n{call_line}",
  );
}

/// Block-scoped restrict for stores (Phase B): `a` and `b` share group `g`, but in the trailing span only
/// `a` reaches into `g` across the opaque `nothing()`. The two `set a.fuel = …` there are recorded as
/// accesses into `g`, so the backend tags their LLVM `store` with `!alias.scope` (its group) and
/// `!noalias` (the disjoint groups) — the write-side mirror of the load tagging above.
#[test]
fn sole_reference_region_stores_get_alias_scope_metadata() {
  let cp = compile_inline(
    r#"
struct Ship { fuel int; }
func nothing() { }
func do_things<g'>(a &Ship in g, b &Ship in g) {
  set b.fuel = 1;
  set a.fuel = 2;
  nothing();
  set a.fuel = 3;
}
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  do_things(&s1, &s2);
  return 0;
}
"#,
    |opts| opts.print_llvmir = true,
  );
  let ll = fs::read_to_string(cp.cwd.join("build.ll")).expect("read build.ll");

  // The tail span's stores through `a` belong to `g`'s alias scope. (`g` is the only group here, so the
  // disjoint set is empty and the stores carry no `!noalias` — the `!noalias` rides the `nothing()` call,
  // asserted separately above.)
  let body = function_body(&ll, "do_things");
  assert!(
    body.lines().any(|l| l.contains("store") && l.contains("!alias.scope")),
    "a store into `g` should carry !alias.scope:\n{body}",
  );
}

/// The `load i32` count in `do_things`'s optimized body, compiling `restrictcoalesce` with the aliasing
/// hints on or suppressed.
fn coalesce_load_count(suppress: bool) -> usize {
  let dir = programs_dir().join("programs/externs/restrictcoalesce");
  let cp = compile_program(&dir, &[], |opts| {
    opts.print_llvmir = true;
    opts.suppress_alias_metadata = suppress;
  });
  let ll = fs::read_to_string(cp.cwd.join("build.opt.ll")).expect("read build.opt.ll");
  function_body(&ll, "do_things").lines().filter(|l| l.contains("load i32")).count()
}

/// Load-bearing proof (Phase B), as an on/off diff: with `!alias.scope`/`!noalias` emitted, the optimizer
/// coalesces the two `a.fuel` reads across the opaque C-extern `noopBarrier()`; with the metadata
/// suppressed the barrier blocks the fold. Comparing the two builds of the *same* program isolates
/// exactly the metadata's effect — any unrelated codegen appears in both counts and cancels — so this is
/// immune to the drift a fixed absolute count would suffer.
#[test]
fn region_reads_coalesce_across_opaque_extern_call() {
  let with = coalesce_load_count(false);
  let without = coalesce_load_count(true);
  assert!(
    with < without,
    "the alias metadata should coalesce at least one a.fuel load: with metadata={with}, suppressed={without}",
  );
}

/// The `store i32` count in `do_things`'s optimized body, compiling `restrictdse` with the aliasing hints
/// on or suppressed.
fn dse_store_count(suppress: bool) -> usize {
  let dir = programs_dir().join("programs/externs/restrictdse");
  let cp = compile_program(&dir, &[], |opts| {
    opts.print_llvmir = true;
    opts.suppress_alias_metadata = suppress;
  });
  let ll = fs::read_to_string(cp.cwd.join("build.opt.ll")).expect("read build.opt.ll");
  function_body(&ll, "do_things").lines().filter(|l| l.contains("store i32")).count()
}

/// Load-bearing proof for stores (Phase B), as an on/off diff: with `!alias.scope` on the stores through
/// `a` and `!noalias` on the opaque `noopBarrier()`, LLVM proves the barrier can't read `a.fuel` and
/// dead-store-eliminates the redundant `set a.fuel = 2`; with the metadata suppressed the barrier blocks
/// DSE. Comparing the two builds isolates exactly the metadata's effect, immune to unrelated drift.
#[test]
fn region_store_dead_store_eliminated_across_opaque_extern_call() {
  let with = dse_store_count(false);
  let without = dse_store_count(true);
  assert!(
    with < without,
    "the alias metadata should dead-store-eliminate at least one a.fuel store: with metadata={with}, suppressed={without}",
  );
}

/// Split a function body (`define … {` through `}`) into basic blocks. A block runs from one label line
/// (an unindented `name:`) up to just before the next.
fn basic_blocks(body: &str) -> Vec<String> {
  let mut blocks: Vec<String> = Vec::new();
  let mut cur = String::new();
  for line in body.lines() {
    let is_label = !line.starts_with(char::is_whitespace)
      && line.split(';').next().unwrap_or("").trim_end().ends_with(':');
    if is_label && !cur.is_empty() {
      blocks.push(cur.clone());
      cur.clear();
    }
    cur.push_str(line);
    cur.push('\n');
  }
  if !cur.is_empty() {
    blocks.push(cur);
  }
  blocks
}

/// Asserts the restrict metadata let LLVM hoist a field's load out of a loop that contains an opaque call:
/// the field is read once before the loop and carried in a register, so the loop body block holding the
/// `noopBarrier()` call has no `load i32` left in it. Without the metadata the call would be assumed to
/// maybe-touch the field, forcing a reload after it every iteration — so this assertion breaks the moment
/// the emission is removed (ITBLUX).
fn assert_field_read_hoisted_across_loop_call(body: &str) {
  let loop_blocks: Vec<String> =
    basic_blocks(body).into_iter().filter(|b| b.contains("@vale_abi_vtest_noopBarrier")).collect();
  assert!(!loop_blocks.is_empty(), "expected a loop block containing a noopBarrier call:\n{body}");
  for b in &loop_blocks {
    assert!(
      !b.contains("load i32"),
      "the field read should be hoisted out of the loop — no reload after the opaque call:\n{b}",
    );
  }
}

/// Register-promotion for a sole reference (Phase A, exactly Rust's `&mut`): in `bump(s &Ship)`, `s` is the
/// only reference into its group, so LLVM keeps `s.fuel` in a register across the loop's opaque
/// `noopBarrier()` calls — the read is hoisted out and never reloaded.
#[test]
fn sole_reference_loop_hoists_field_read_across_opaque_calls() {
  let dir = programs_dir().join("programs/externs/restrictloopsole");
  let cp = compile_program(&dir, &[], |opts| opts.print_llvmir = true);
  let ll = fs::read_to_string(cp.cwd.join("build.opt.ll")).expect("read build.opt.ll");
  assert_field_read_hoisted_across_loop_call(&function_body(&ll, "@vale_abi_vtest_bump("));
}

/// Block-scoped restrict — the case Rust's `&mut` cannot express. In `bump_inline`, `a` and `b` share
/// group `g` (two overlapping `&mut` would be rejected by Rust), so *neither* gets whole-function noalias.
/// Yet in each loop only one of them is used, so the checker proves it sole there and the backend tags that
/// loop's accesses/call. LLVM then hoists `a.fuel`'s read across the first loop's calls and `b.fuel`'s
/// across the second's — register-promotion driven purely by the block-scoped metadata, since there is no
/// param-level noalias to lean on.
#[test]
fn block_scoped_restrict_hoists_reads_in_two_aliasing_loops() {
  let dir = programs_dir().join("programs/externs/restrictloopblock");
  let cp = compile_program(&dir, &[], |opts| opts.print_llvmir = true);
  let ll = fs::read_to_string(cp.cwd.join("build.opt.ll")).expect("read build.opt.ll");
  assert_field_read_hoisted_across_loop_call(&function_body(&ll, "@vale_abi_vtest_bump_inline"));
}

/// Extern declarations inherit Vale's no-unwind execution model (`panic=abort`): the backend stamps
/// `nounwind` on every extern's LLVM declaration, which LLVM cannot infer for an opaque (bodiless) callee.
/// This is what lets the optimizer treat an extern call as guaranteed to transfer control to its successor,
/// unblocking the dead-store elimination the `restrictdse` test above depends on.
#[test]
fn extern_declaration_is_nounwind() {
  let dir = programs_dir().join("programs/externs/restrictdse");
  let cp = compile_program(&dir, &[], |opts| opts.print_llvmir = true);
  let ll = fs::read_to_string(cp.cwd.join("build.ll")).expect("read build.ll");

  // Assert on the pre-optimization dump so an opt pass can't be what introduces or hides the attribute.
  let decl = declare_line(&ll, "@vale_abi_vtest_noopBarrier");
  assert!(
    attr_group(&ll, decl).contains("nounwind"),
    "the noopBarrier extern declaration should carry nounwind (Vale is panic=abort):\n{decl}",
  );
}

/// A read-only callee handed a reference into `g` can still READ `g`, and reading is invisible in a Valen
/// signature (only `mut` is declared). So a call is `!noalias {g}` only when its arguments cannot reach
/// `g` — reaching `g` (even to read it) keeps the call out of the complement. Here `reads(a)` is handed
/// `a`, which reaches `g`, so its call must carry no `!noalias`; otherwise LLVM could delete a write the
/// callee observes.
#[test]
fn read_only_call_reaching_group_is_not_noalias() {
  let cp = compile_inline(
    r#"
struct Ship { fuel int; }
func reads(s &Ship) int { return __copy_prim(s.fuel); }
func do_things<g'>(a &Ship in g, b &Ship in g) mut(g) {
  set b.fuel = 1;
  set a.fuel = 2;
  reads(a);
  set a.fuel = 3;
}
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  do_things(&s1, &s2);
  return 0;
}
"#,
    |opts| opts.print_llvmir = true,
  );
  let ll = fs::read_to_string(cp.cwd.join("build.ll")).expect("read build.ll");

  let body = function_body(&ll, "do_things");
  let call_line = body
    .lines()
    .find(|l| l.contains("call") && l.contains("reads"))
    .unwrap_or_else(|| panic!("no `reads(a)` call line in do_things:\n{body}"));
  assert!(
    !call_line.contains("!noalias"),
    "a call that can reach `g` (it reads `a`) must not be !noalias'd:\n{call_line}",
  );
}

/// `!alias.scope` is inert on its own, so it is emitted uniformly on every access — including a lone
/// borrow parameter that is the whole-function sole reference into its group, which the param-level
/// `noalias` attribute also covers. `solo`'s stores through `a` still carry `!alias.scope`.
#[test]
fn single_reference_function_store_gets_alias_scope() {
  let cp = compile_inline(
    r#"
struct Ship { fuel int; }
func nothing() { }
func solo<g'>(a &Ship in g) mut(g) {
  set a.fuel = 1;
  nothing();
  set a.fuel = 2;
}
exported func main() int {
  s = Ship(1);
  solo(&s);
  return 0;
}
"#,
    |opts| opts.print_llvmir = true,
  );
  let ll = fs::read_to_string(cp.cwd.join("build.ll")).expect("read build.ll");

  let body = function_body(&ll, "solo");
  assert!(
    body.lines().any(|l| l.contains("store") && l.contains("!alias.scope")),
    "a single-reference function's store should carry !alias.scope:\n{body}",
  );
}

/// Two references in disjoint groups (`a in g`, `c in h`) never alias, so each access carries `!noalias`
/// naming the other group's scope. Neither group has a second reference, yet both accesses are still
/// tagged and the cross-group `!noalias` is emitted by complement.
#[test]
fn disjoint_group_access_carries_cross_group_noalias() {
  let cp = compile_inline(
    r#"
struct Ship { fuel int; }
func nothing() { }
func do_things<g', h'>(a &Ship in g, c &Ship in h) mut(g) mut(h) {
  set a.fuel = 1;
  set c.fuel = 2;
  nothing();
}
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  do_things(&s1, &s2);
  return 0;
}
"#,
    |opts| opts.print_llvmir = true,
  );
  let ll = fs::read_to_string(cp.cwd.join("build.ll")).expect("read build.ll");

  let body = function_body(&ll, "do_things");
  assert!(
    body.lines().any(|l| l.contains("store") && l.contains("!alias.scope") && l.contains("!noalias")),
    "a store into one of two disjoint groups should carry !alias.scope + cross-group !noalias:\n{body}",
  );
}

/// The `suppress_alias_metadata` test lever drops all aliasing optimization hints — the
/// `!alias.scope`/`!noalias` metadata AND the parameter-level `noalias` attribute — while keeping
/// `nounwind`, so a load-bearing test can compile the same program with and without the hints and
/// compare the optimizer's output. Here we prove the lever itself works: the baseline emits `noalias`,
/// and with the flag every trace of it is gone but `nounwind` remains.
#[test]
fn suppress_alias_metadata_drops_all_alias_hints_but_keeps_nounwind() {
  let dir = programs_dir().join("programs/externs/restrictloopsole");

  let cp_on = compile_program(&dir, &[], |opts| opts.print_llvmir = true);
  let ll_on = fs::read_to_string(cp_on.cwd.join("build.ll")).expect("read build.ll");
  assert!(ll_on.contains("noalias"), "baseline should emit some noalias hint:\n{ll_on}");

  let cp_off = compile_program(&dir, &[], |opts| {
    opts.print_llvmir = true;
    opts.suppress_alias_metadata = true;
  });
  let ll_off = fs::read_to_string(cp_off.cwd.join("build.ll")).expect("read build.ll");
  assert!(!ll_off.contains("!alias.scope"), "suppress should drop !alias.scope:\n{ll_off}");
  assert!(!ll_off.contains("noalias"), "suppress should drop the param `noalias` and `!noalias`:\n{ll_off}");
  assert!(ll_off.contains("nounwind"), "nounwind must remain so the execution model stays sound:\n{ll_off}");
}

/// A read-only call handed an in-group reference is NOT `!noalias`'d against that group — reading is
/// invisible in the signature (only `mut` is declared), so the call reaches its argument's group and the
/// backend must not exclude it, or it could delete a write the callee observes. Contrast a no-argument
/// call, which reaches nothing and so is `!noalias`'d against the group. (Asserted at e2e because the
/// unified carrier is untagged — a unit test can't tell a call's reach from an access into the same
/// group.)
#[test]
fn read_only_call_reaching_its_argument_group_is_not_noaliased() {
  let cp = compile_inline(
    r#"
struct Ship { fuel int; }
func peek(s &Ship) int { return __copy_prim(s.fuel); }
func barrier() { }
func do_things<g'>(a &Ship in g, b &Ship in g) mut(g) {
  set a.fuel = 1;
  set b.fuel = 2;
  peek(a);
  barrier();
  set a.fuel = 3;
}
exported func main() int {
  s1 = Ship(1);
  s2 = Ship(2);
  do_things(&s1, &s2);
  return 0;
}
"#,
    |opts| opts.print_llvmir = true,
  );
  let ll = fs::read_to_string(cp.cwd.join("build.ll")).expect("read build.ll");
  let body = function_body(&ll, "do_things");

  let peek_call = body
    .lines()
    .find(|l| l.contains("call") && l.contains("peek"))
    .unwrap_or_else(|| panic!("no peek call in:\n{body}"));
  assert!(
    !peek_call.contains("!noalias"),
    "a read-only call reaching its argument's group must NOT be !noalias'd against it:\n{peek_call}",
  );

  let barrier_call = body
    .lines()
    .find(|l| l.contains("call") && l.contains("barrier"))
    .unwrap_or_else(|| panic!("no barrier call in:\n{body}"));
  assert!(
    barrier_call.contains("!noalias"),
    "a no-argument call reaches no group, so it must be !noalias'd against the group:\n{barrier_call}",
  );
}
