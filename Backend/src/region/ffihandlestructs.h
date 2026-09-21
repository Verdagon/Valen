#ifndef REGION_FFIHANDLESTRUCTS_H_
#define REGION_FFIHANDLESTRUCTS_H_

#include <llvm-c/Core.h>

class GlobalState;
class FunctionState;

// The exploded components of an FFI handle. Concrete handles fill only
// objPtrI64LE (typeInfoPtrI64LE stays null); interface handles fill both.
struct FfiHandleExplodedMembers {
  LLVMValueRef objPtrI64LE;
  LLVMValueRef typeInfoPtrI64LE;

  FfiHandleExplodedMembers() = delete;

  FfiHandleExplodedMembers(
      LLVMValueRef objPtrI64LE_,
      LLVMValueRef typeInfoPtrI64LE_) :
      objPtrI64LE(objPtrI64LE_),
      typeInfoPtrI64LE(typeInfoPtrI64LE_) {}
};

struct FfiHandleStructs {
  explicit FfiHandleStructs(LLVMContextRef context);

  FfiHandleExplodedMembers explodeForRegularConcrete(
      GlobalState* globalState, FunctionState* functionState, LLVMBuilderRef builder, LLVMValueRef handleLE);

  FfiHandleExplodedMembers explodeForRegularInterface(
      GlobalState* globalState, FunctionState* functionState, LLVMBuilderRef builder, LLVMValueRef handleLE);

  LLVMValueRef implodeForRegularConcrete(
      GlobalState* globalState,
      FunctionState* functionState,
      LLVMBuilderRef builder,
      LLVMValueRef objPtrI64LE);
  LLVMValueRef implodeForRegularInterface(
      GlobalState* globalState,
      FunctionState* functionState,
      LLVMBuilderRef builder,
      LLVMValueRef typeInfoPtrI64LE,
      LLVMValueRef objPtrI64LE);

  [[nodiscard]] LLVMTypeRef getConcreteHandleStructLT() const { return concreteHandleStructLT; }
  [[nodiscard]] LLVMTypeRef getInterfaceHandleStructLT() const { return interfaceHandleStructLT; }

private:
  LLVMTypeRef concreteHandleStructLT;
  LLVMTypeRef interfaceHandleStructLT;
};

#endif
