#include <array>
#include <globalstate.h>
#include <function/function.h>
#include <function/expressions/expressions.h>
#include <region/common/common.h>
#include "ffihandlestructs.h"

FfiHandleStructs::FfiHandleStructs(LLVMContextRef context) {
  auto int64LT = LLVMInt64TypeInContext(context);
  concreteHandleStructLT = LLVMStructCreateNamed(context, "__ConcreteHandle");
  std::array<LLVMTypeRef, 1> concreteMembersLT{int64LT};
  LLVMStructSetBody(concreteHandleStructLT, concreteMembersLT.data(), concreteMembersLT.size(), false);
  interfaceHandleStructLT = LLVMStructCreateNamed(context, "__InterfaceHandle");
  std::array<LLVMTypeRef, 2> interfaceMembersLT{int64LT, int64LT};
  LLVMStructSetBody(interfaceHandleStructLT, interfaceMembersLT.data(), interfaceMembersLT.size(), false);
}

FfiHandleExplodedMembers FfiHandleStructs::explodeForRegularConcrete(
    GlobalState* globalState, FunctionState* functionState, LLVMBuilderRef builder, LLVMValueRef handleLE) {
  assert(LLVMTypeOf(handleLE) == concreteHandleStructLT);
  auto objPtrI64LE = LLVMBuildExtractValue(builder, handleLE, 0, "objPtrI64");
  return FfiHandleExplodedMembers{objPtrI64LE, nullptr};
}

FfiHandleExplodedMembers FfiHandleStructs::explodeForRegularInterface(
    GlobalState* globalState, FunctionState* functionState, LLVMBuilderRef builder, LLVMValueRef handleLE) {
  assert(LLVMTypeOf(handleLE) == interfaceHandleStructLT);
  auto objPtrI64LE = LLVMBuildExtractValue(builder, handleLE, 0, "objPtrI64");
  auto typeInfoPtrI64LE = LLVMBuildExtractValue(builder, handleLE, 1, "typeInfoPtrI64");
  return FfiHandleExplodedMembers{objPtrI64LE, typeInfoPtrI64LE};
}

LLVMValueRef FfiHandleStructs::implodeForRegularConcrete(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMBuilderRef builder,
    LLVMValueRef objPtrI64LE) {
  auto int64LT = LLVMInt64TypeInContext(globalState->context);
  assert(LLVMTypeOf(objPtrI64LE) == int64LT);
  auto handleLE = LLVMGetUndef(concreteHandleStructLT);
  handleLE = LLVMBuildInsertValue(builder, handleLE, objPtrI64LE, 0, "handle");
  return handleLE;
}

LLVMValueRef FfiHandleStructs::implodeForRegularInterface(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMBuilderRef builder,
    LLVMValueRef typeInfoPtrI64LE,
    LLVMValueRef objPtrI64LE) {
  auto int64LT = LLVMInt64TypeInContext(globalState->context);
  assert(LLVMTypeOf(objPtrI64LE) == int64LT);
  assert(LLVMTypeOf(typeInfoPtrI64LE) == int64LT);
  auto handleLE = LLVMGetUndef(interfaceHandleStructLT);
  handleLE = LLVMBuildInsertValue(builder, handleLE, objPtrI64LE, 0, "handle"); // field 0 = obj
  handleLE = LLVMBuildInsertValue(builder, handleLE, typeInfoPtrI64LE, 1, "handle"); // field 1 = typeinfo
  return handleLE;
}
