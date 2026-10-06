// Fence: ensures that Valen vectorizes a loop that writes one buffer of a
// level while it reads another, with no check that the two buffers overlap.
//
// Disclosed workaround: in either Rust arm, `diffuse` could be split into a
// version for two different buffers and an in-place version for one. In the
// GhostCell arm that would let the two buffers have separate brands. It changes
// nothing in `scale`, the loop this test measures: both Rust arms already
// vectorize it behind one overlap check, with or without the split. Splitting a
// function into an aliasing and a non-aliasing version is ruled inadmissible
// for every arm.
//
// Both Rust arms vectorize this loop today. Each first checks, once per call,
// that the two buffers do not overlap. The absent check is Valen's only
// expected difference here. The GhostCell arm's header records a layout under
// which GhostCell does not vectorize at all, and why that layout is not its
// best.
//
// The program:
//   Level { heat Vec<Sample>, light Vec<Sample>, params Params }
//   scale(level &Level in g, n i64) mut(level.heat...)
//   for each i: heat[i].v = heat[i].v + light[i].v * params.gain
//   diffuse(dst &Vec<Sample> in g, src &Vec<Sample> in g, n i64) mut(g)
//   for each i: dst[i].v = dst[i].v + src[i].v / 8
//   `diffuse` is called as diffuse(heat, heat), where heat spreads, and as
//   diffuse(heat, light), where lit tiles warm up. It reads no neighbours, so
//   the in-place call is correct without a second buffer. It is why the
//   GhostCell arm keeps the samples of both buffers in cells of one brand.
//
// Mechanism: M2. In `scale`, `level.heat` and `level.light` are sibling fields,
// so their elements are in different scopes. Each store to a heat element
// carries the heat scope, and each load of a light element is marked `!noalias`
// against it. LLVM then knows the stores cannot change what the loop reads, and
// needs no runtime overlap check before vectorizing. Inside `diffuse` the two
// buffers share one group, but only for that call.
//
// Arms:
// - Plain Rust: rust/two_buffers_one_owner_vectorize_plain.rs
// - GhostCell:  rust/two_buffers_one_owner_vectorize.rs
//
// Assumptions behind the expected IR:
// - `at` is inlined into `scale`.
// - `#inline(never)` keeps `scale` a function, so `level` stays a `noalias`
//   parameter. The buffer pointers, the lengths and `gain` are reached through
//   `level` itself, and parameter `noalias` is what lets them be loaded once.
// - LLVM folds the bounds checks that `at` makes into the trip count, as it
//   does for both Rust arms today.
//
// Expected IR of `scale`, unoptimized:
// - Per element, the loop loads both buffer pointers and both lengths, checks
//   both bounds, loads `gain`, loads the light element and the heat element, and
//   stores the heat element.
//
// Expected IR of `scale`, optimized:
// - Both buffer pointers, both lengths and `gain` are loaded once, before the
//   loop.
// - The vectorized loop body has vector operations on the heat and light
//   elements.
// - There is no `vector.memcheck` block: no runtime check that the heat buffer
//   and the light buffer overlap.
// - A scalar loop handles the remaining elements and keeps both bounds checks.

use crate::typing::test::rust_interop::drive_helpers::drive_and_run;

#[test]
fn two_buffers_one_owner_vectorize() {
  let run = drive_and_run("horizon/main", r#"
import mycrate.at;
import std.vec.Vec;
import std.alloc.Global;
struct Sample { v int; }
struct Params { gain int; }
struct Level { heat Vec<Sample, Global>; light Vec<Sample, Global>; params Params; }
func diffuse<g'>(dst &Vec<Sample, Global> in g, src &Vec<Sample, Global> in g, n i64) mut(g) {
  i = 0i64;
  while i < __copy_prim(n) {
    d = at(dst, __copy_prim(i));
    set d.v = __copy_prim(d.v) + __copy_prim(at(src, __copy_prim(i)).v) / 8;
    set i = i + 1i64;
  }
}
#inline(never)
func scale<g'>(level &Level in g, n i64) mut(level.heat...) {
  i = 0i64;
  while i < __copy_prim(n) {
    h = at(&level.heat, __copy_prim(i));
    set h.v = __copy_prim(h.v) + __copy_prim(at(&level.light, __copy_prim(i)).v) * __copy_prim(level.params.gain);
    set i = i + 1i64;
  }
}
exported func main() int {
  level = Level(Vec.new<Sample>(), Vec.new<Sample>(), Params(2));
  level.heat.push(Sample(16));
  level.heat.push(Sample(32));
  level.heat.push(Sample(8));
  level.heat.push(Sample(24));
  level.light.push(Sample(8));
  level.light.push(Sample(16));
  level.light.push(Sample(0));
  level.light.push(Sample(8));
  diffuse(&level.heat, &level.heat, 4i64);
  diffuse(&level.heat, &level.light, 4i64);
  scale(&level, 4i64);
  return __copy_prim(at(&level.heat, 0i64).v) + __copy_prim(at(&level.heat, 1i64).v) + __copy_prim(at(&level.heat, 2i64).v) + __copy_prim(at(&level.heat, 3i64).v);
}
"#);
  assert_eq!(
    run.process_exit,
    Some(158),
    "rustc_exit={}, process_exit={:?}, firings: {:?}",
    run.rustc_exit, run.process_exit, run.firings
  );
}
