use rustc_abi::{BackendRepr, Primitive};
use rustc_middle::ty::{self, Instance, Ty, TyCtxt};
use rustc_target::callconv::{ArgAbi, PassMode};

use crate::backend_ffi::metal_lowerer::{PassModeR, ExternAbi};

// Asks rustc what the ABI is for the given function.
pub(super) fn compute_extern_abi<'tcx>(tcx: TyCtxt<'tcx>, instance: Instance<'tcx>) -> ExternAbi {
  let typing_env = ty::TypingEnv::fully_monomorphized();
  let fn_abi = tcx
    .fn_abi_of_instance(typing_env.as_query_input((instance, ty::List::empty())))
    .unwrap_or_else(|e| panic!("rust interop: rustc could not compute the ABI of {instance:?}: {e:?}"));
  let ret = translate_pass_mode(&fn_abi.ret);
  let mut args: Vec<PassModeR> = fn_abi.args.iter().map(|a| translate_pass_mode(a)).collect();
  if instance.def.requires_caller_location(tcx) {
    match args.last_mut() {
      Some(last) => *last = PassModeR::LocationPtr,
      None => panic!("requires_caller_location is true but fn_abi has no args"),
    }
  }
  ExternAbi { ret, args }
}

fn translate_pass_mode<'tcx>(arg: &ArgAbi<'tcx, Ty<'tcx>>) -> PassModeR {
  match &arg.mode {
    PassMode::Ignore => PassModeR::Ignore,
    PassMode::Direct(_) => {
      if let BackendRepr::Scalar(scalar) = arg.layout.backend_repr {
        if matches!(scalar.primitive(), Primitive::Pointer(_)) {
          return PassModeR::DirectPtr;
        }
      }
      PassModeR::DirectInt(arg.layout.size.bits() as u32)
    }
    PassMode::Pair(_, _) => match arg.layout.backend_repr {
      BackendRepr::ScalarPair(s0, s1) => {
        let bits0 =
            match s0.primitive() {
              Primitive::Int(int, _) => int.size().bits() as u32,
              primitive => panic!("only pairs of ints supported, got {:?}", primitive),
            };
        let bits1 =
            match s1.primitive() {
              Primitive::Int(int, _) => int.size().bits() as u32,
              primitive => panic!("only pairs of ints supported, got {:?}", primitive),
            };
        PassModeR::Pair(bits0, bits1)
      }
      backend_repr => unimplemented!(),
    },
    _ => unimplemented!(),
  }
}
