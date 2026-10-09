#include <iostream>
#include "../../region/common/controlblock.h"
#include "shared/string.h"

#include "shared/shared.h"
#include "../../region/common/heap.h"

Ref translateConstantStr(
    AreaAndFileAndLine from,
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMBuilderRef builder,
    ConstantStr* constantStr) {
  auto strRef =
      buildConstantVStr(globalState, functionState, builder, constantStr->value);
  // Dont need to alias here, see SRCAO
  return strRef;
}

Ref translateConstantRustStr(
    AreaAndFileAndLine from,
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMBuilderRef builder,
    ConstantRustStr* constantRustStr) {
  auto i8PtrLT = LLVMPointerType(LLVMInt8TypeInContext(globalState->context), 0);
  auto i64LT = LLVMInt64TypeInContext(globalState->context);
  auto charsRawLE = globalState->getOrMakeStringConstant(constantRustStr->value);
  auto charsLE = LLVMBuildBitCast(builder, charsRawLE, i8PtrLT, "rustStrCharPtr");
  auto lenLE = LLVMConstInt(i64LT, constantRustStr->value.length(), 0);
  LLVMTypeRef pairElems[2] = { i8PtrLT, i64LT };
  auto pairLT = LLVMStructTypeInContext(globalState->context, pairElems, 2, 0);
  auto pairLE = LLVMGetUndef(pairLT);
  pairLE = LLVMBuildInsertValue(builder, pairLE, charsLE, 0, "rustStrPtr");
  pairLE = LLVMBuildInsertValue(builder, pairLE, lenLE, 1, "rustStrLen");
  auto kind = constantRustStr->result;
  auto blobLT = globalState->getRegion(kind)->translateType(kind);
  auto blobLE = bitcastViaBackendLocal(functionState, builder, blobLT, "rustStrBlob", pairLE);
  return toRef(globalState->getRegion(kind), kind, blobLE);
}
