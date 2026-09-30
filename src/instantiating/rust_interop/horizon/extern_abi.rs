use rustc_abi::{BackendRepr, Primitive, RegKind};
use rustc_middle::ty::{self, Instance, Ty, TyCtxt};
use rustc_target::callconv::{ArgAbi, PassMode};

use crate::backend_ffi::metal_lowerer::{ExternAbi, PassModeR};

/// Ask rustc (`tcx.fn_abi_of_instance`) how a Rust leaf's — or a Rust→Vale callback's — return and
/// each argument cross the boundary, mapping each `PassMode` to the `PassModeR` the backend obeys.
/// The query is role-agnostic: the same answer says how Vale must call a Rust leaf and how Rust hands
/// a callback its receiver and args.
pub(super) fn compute_extern_abi<'tcx>(tcx: TyCtxt<'tcx>, instance: Instance<'tcx>) -> ExternAbi {
  let typing_env = ty::TypingEnv::fully_monomorphized();
  let fn_abi = tcx
    .fn_abi_of_instance(typing_env.as_query_input((instance, ty::List::empty())))
    .unwrap_or_else(|e| panic!("rust interop: rustc could not compute the ABI of {instance:?}: {e:?}"));
  let ret = translate_pass_mode(tcx, &fn_abi.ret);
  let mut args: Vec<PassModeR> = fn_abi.args.iter().map(|a| translate_pass_mode(tcx, a)).collect();
  // Per @TCHAPZ, a `#[track_caller]` fn's `fn_abi` has a hidden trailing `&Location` arg that its
  // `fn_sig`, and so the Vale prototype, lacks. Detect it via `requires_caller_location` and mark that
  // trailing pass mode `LocationPtr`, so the backend declares a ptr param and passes null for it.
  if instance.def.requires_caller_location(tcx) {
    match args.last_mut() {
      Some(last) => *last = PassModeR::LocationPtr,
      None => panic!("requires_caller_location is true but fn_abi has no args"),
    }
  }
  ExternAbi { ret, args }
}

fn translate_pass_mode<'tcx>(tcx: TyCtxt<'tcx>, arg: &ArgAbi<'tcx, Ty<'tcx>>) -> PassModeR {
  match &arg.mode {
    // A unit return `()`, e.g. the `__vale_drop` shim's return, or a zero-sized value.
    PassMode::Ignore => PassModeR::Ignore,
    // Per @EACBIPZ, a large aggregate crosses as an indirect pointer to a caller-made copy, not byval.
    // Only `on_stack: false` (AArch64 AAPCS) is handled; `on_stack: true` (x86 byval) is not yet.
    PassMode::Indirect { on_stack: false, .. } => PassModeR::Indirect,
    // A value small enough for a register.
    PassMode::Direct(_) => {
      // A reference (`&self`, `&mut self`, `*mut T`) is a pointer-scalar and crosses as a real pointer.
      // A small integer-classed aggregate (`Counter`, `Glyph`) crosses as its register integer.
      if let BackendRepr::Scalar(scalar) = arg.layout.backend_repr {
        if matches!(scalar.primitive(), Primitive::Pointer(_)) {
          return PassModeR::DirectPtr;
        }
      }
      PassModeR::DirectInt(arg.layout.size.bits() as u32)
    }
    // A small struct rustc `Cast`s to registers. Only the single-integer case is handled: no leading
    // prefix registers, and one integer unit covering the whole value — an 8-byte struct crossing as
    // a bare `i64`. Multi-piece (`[2 x i64]`), float, or prefixed casts are not yet.
    PassMode::Cast { cast, pad_i32: false } => {
      if cast.prefix.iter().any(|p| p.is_some())
        || cast.rest_offset.is_some()
        || cast.rest.unit.kind != RegKind::Integer
        || cast.rest.total != cast.rest.unit.size
      {
        panic!("unsupported Cast {cast:?} for interop extern (only a single integer unit is handled)");
      }
      PassModeR::Cast(cast.rest.unit.size.bits() as u32)
    }
    // A small struct rustc passes as two register scalars (`ScalarPair`), e.g. `{i32, i32}`. Each
    // component crosses as its own integer; the struct is reassembled from the two on the far side.
    // Only integer components are handled — a pointer component (a fat pointer / slice) is not yet.
    PassMode::Pair(_, _) => match arg.layout.backend_repr {
      BackendRepr::ScalarPair(s0, s1) => {
        if matches!(s0.primitive(), Primitive::Pointer(_))
          || matches!(s1.primitive(), Primitive::Pointer(_))
        {
          panic!("unsupported Pair with a pointer component for interop extern (fat pointer not handled)");
        }
        PassModeR::Pair(s0.size(&tcx).bits() as u32, s1.size(&tcx).bits() as u32)
      }
      other => panic!("PassMode::Pair without a ScalarPair backend_repr: {other:?}"),
    },
    other => panic!(
      "unsupported PassMode {other:?} for interop extern (on-stack byval / multi-piece Cast / HFA not yet handled)"
    ),
  }
}
