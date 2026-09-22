#![allow(non_snake_case)]

use crate::end_to_end_tests::{
    assert_compile_and_run_dbg, assert_inline_compile_and_run, cmd, expect, programs_dir, reject,
};

fn p(rel: &str) -> std::path::PathBuf {
    programs_dir().join(rel)
}

#[test]
fn generic_lambda_forwarder_runs() {
    assert_inline_compile_and_run(
        r#"
#!DeriveInterfaceDrop
sealed interface Bork {
  func bork(virtual self &Bork) int;
}

#!DeriveStructDrop
struct BorkForwarder<Lam>
where func drop(Lam)void, func __call(&Lam)int {
  lam Lam;
}

impl<Lam> Bork for BorkForwarder<Lam>;

func bork<Lam>(self &BorkForwarder<Lam>) int {
  return (&self.lam)();
}

exported func main() int {
  f = BorkForwarder({ 7 });
  z = (&f).bork();
  [_] = ^f;
  return ^z;
}
"#,
        7,
    );
}

#[test]
fn mutswaplocals() {
    assert_compile_and_run_dbg(&p("programs/mutswaplocals.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: mutswaplocals-before' -f mutswaplocals.vale"),
        cmd("run"),
        expect("frame variable -P 1 a", &["fuel = 1"]),
        expect("frame variable -P 1 b", &["fuel = 2"]),
        cmd("br s -p 'lldb breakpoint: mutswaplocals-after' -f mutswaplocals.vale"),
        cmd("continue"),
        expect("frame variable -P 1 a", &["fuel = 2"]),
        expect("frame variable -P 1 b", &["fuel = 1"]),
    ]);
}

#[test]
fn restackify() {
    assert_compile_and_run_dbg(&p("programs/restackify.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: restackify-before' -f restackify.vale"),
        cmd("br s -p 'lldb breakpoint: restackify-after' -f restackify.vale"),
        cmd("run"),
        expect("frame variable -P 1 ship", &["fuel = 35"]),
        cmd("continue"),
        expect("frame variable -P 1 ship", &["fuel = 42"]),
    ]);
}

#[test]
fn destructure_restackify() {
    assert_compile_and_run_dbg(&p("programs/destructure_restackify.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: destructure-restackify-ready' -f destructure_restackify.vale"),
        cmd("run"),
        expect("frame variable fuel", &["fuel = 42"]),
        expect("frame variable -P 1 ship", &["fuel = 42"]),
    ]);
}

#[test]
fn loop_restackify() {
    assert_compile_and_run_dbg(&p("programs/loop_restackify.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: loop-restackify-iter' -f loop_restackify.vale"),
        cmd("run"),
        expect("frame variable i", &["i = 0"]),
        expect("frame variable -P 1 ship", &["fuel = 27"]),
        cmd("continue"),
        expect("frame variable i", &["i = 1"]),
        expect("frame variable -P 1 ship", &["fuel = 32"]),
        cmd("continue"),
        expect("frame variable i", &["i = 2"]),
        expect("frame variable -P 1 ship", &["fuel = 37"]),
    ]);
}

#[test]
fn mutlocal() {
    assert_compile_and_run_dbg(&p("programs/mutlocal.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: mutlocal-before' -f mutlocal.vale"),
        cmd("run"),
        expect("frame variable x", &["x = 73"]),
        cmd("br s -p 'lldb breakpoint: mutlocal-after' -f mutlocal.vale"),
        cmd("continue"),
        expect("frame variable x", &["x = 42"]),
    ]);
}

#[test]
fn constraintRef() {
    assert_compile_and_run_dbg(&p("programs/constraintRef.vale"), 8, &[
        cmd("br s -p 'lldb breakpoint: constraintRef-ready' -f constraintRef.vale"),
        cmd("run"),
        expect("frame variable -P 1 carrier", &["hp = 400", "interceptors = 8"]),
    ]);
}

#[test]
fn unstackifyret() {
    assert_compile_and_run_dbg(&p("programs/unstackifyret.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: unstackifyret-set' -f unstackifyret.vale"),
        cmd("run"),
        expect("frame variable playerRow", &["playerRow = 4"]),
    ]);
}

#[test]
fn unreachablemoot() {
    assert_compile_and_run_dbg(&p("programs/unreachablemoot.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: unreachablemoot-live' -f unreachablemoot.vale"),
        cmd("br s -p 'lldb breakpoint: unreachablemoot-dead' -f unreachablemoot.vale"),
        expect("run", &["stop reason = breakpoint 1"]),
        reject("continue", &["exited with status = 42"], &["stop reason = breakpoint 2"]),
    ]);
}

#[test]
fn panic() {
    assert_compile_and_run_dbg(&p("programs/panic.vale"), 1, &[
        cmd("br s -p 'lldb breakpoint: panic-site' -f panic.vale"),
        cmd("br s -p 'lldb breakpoint: panic-after' -f panic.vale"),
        expect("run", &["stop reason = breakpoint 1"]),
        reject("continue", &[], &["stop reason = breakpoint 2"]),
    ]);
}

#[test]
fn panicnot() {
    assert_compile_and_run_dbg(&p("programs/panicnot.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: panicnot-return' -f panicnot.vale"),
        cmd("br s -p 'lldb breakpoint: panicnot-dead' -f panicnot.vale"),
        expect("run", &["stop reason = breakpoint 1"]),
        reject("continue", &["exited with status = 42"], &["stop reason = breakpoint 2"]),
    ]);
}

#[test]
fn nestedblocks() {
    assert_compile_and_run_dbg(&p("programs/nestedblocks.vale"), 42, &[
        cmd("br s -p 'lldb breakpoint: nestedblocks-inner' -f nestedblocks.vale"),
        cmd("run"),
        expect("frame variable originalIndex", &["originalIndex = 9"]),
        expect("frame variable i", &["i = 1"]),
        expect("frame variable neighborIndex", &["neighborIndex = 10"]),
    ]);
}
