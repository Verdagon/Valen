// Rust->Vale callback wrappers (the reverse interop direction).

#include <iostream>

#include "../function/expressions/shared/shared.h"
#include "../translatetype.h"
#include "../function/function.h"
#include "../function/boundary.h"
#include <region/common/migration.h>

#include "rust_interop.h"

void emitInboundCallbackWrapper(
    GlobalState* globalState,
    Program* program,
    const std::string& symbol,
    const std::string& valeName) {
  Prototype* prototypeM = nullptr;
  for (auto& [coord, package] : program->packages) {
    (void)coord;
    auto iter = package->functions.find(valeName);
    if (iter != package->functions.end()) {
      prototypeM = iter->second->prototype;
      break;
    }
  }
  assert(prototypeM != nullptr && "inbound callback body not found in program");

  auto sig = buildBoundarySignature(globalState, prototypeM);
  bool usingReturnOutParam = sig.usesReturnOutParam;
  LLVMTypeRef wrapperReturnLT = sig.returnLT;
  LLVMTypeRef wrapperFunctionTypeL =
      LLVMFunctionType(sig.returnLT, sig.paramTypesL.data(), sig.paramTypesL.size(), 0);

  LLVMValueRef wrapperL = LLVMAddFunction(globalState->mod, symbol.c_str(), wrapperFunctionTypeL);
  LLVMSetLinkage(wrapperL, LLVMExternalLinkage);

  LLVMBasicBlockRef block = LLVMAppendBasicBlockInContext(globalState->context, wrapperL, "entry");
  LLVMBuilderRef builder = LLVMCreateBuilderInContext(globalState->context);
  LLVMPositionBuilderAtEnd(builder, block);
  LLVMBuilderRef localsBuilder = builder;

  FunctionState functionState(symbol, wrapperL, wrapperReturnLT, localsBuilder);
  BlockState initialBlockState(globalState->addressNumberer, nullptr, std::nullopt);

  const ExternAbi* abi = lookupExternAbi(globalState, prototypeM);
  assert(abi != nullptr && "inbound callback wrapper needs the callback's extern ABI");
  assert(abi->args.size() == prototypeM->params.size());
  std::vector<Ref> argsToBody;
  unsigned cParamIndex = usingReturnOutParam ? 1u : 0u;
  for (int logicalParamIndex = 0; logicalParamIndex < (int)prototypeM->params.size(); logicalParamIndex++) {
    auto valeParamRefMT = prototypeM->params[logicalParamIndex];
    const Coercion& c = abi->args[logicalParamIndex];
    if (c.kind == CoercionKind::Pair) {
      // Two integer register params -> reassemble the Vale struct. Store the two scalars into an
      // {iN,iM} slot at their offsets, then reinterpret the slot as the Vale struct and load it by
      // value (the inverse of buildCallOrSideCall's Cast+StructKind spill).
      auto structKind = peel_all_references(valeParamRefMT);
      auto valeStructLT = globalState->getRegion(structKind)->translateType(structKind);
      auto lo = LLVMGetParam(wrapperL, cParamIndex);
      auto hi = LLVMGetParam(wrapperL, cParamIndex + 1);
      LLVMTypeRef pairElems[2] = {LLVMTypeOf(lo), LLVMTypeOf(hi)};
      auto pairLT = LLVMStructTypeInContext(globalState->context, pairElems, 2, /*packed=*/0);
      auto slot = makeBackendLocal(&functionState, builder, pairLT, "pairInSlot", LLVMGetUndef(pairLT));
      LLVMBuildStore(builder, lo, LLVMBuildStructGEP2(builder, pairLT, slot, 0, "pf0"));
      LLVMBuildStore(builder, hi, LLVMBuildStructGEP2(builder, pairLT, slot, 1, "pf1"));
      auto valeStructLE = LLVMBuildLoad2(
          builder, valeStructLT,
          LLVMBuildBitCast(builder, slot, LLVMPointerType(valeStructLT, 0), "pairAsStruct"),
          "pairStruct");
      argsToBody.push_back(toRef(globalState->getRegion(structKind), valeParamRefMT, valeStructLE));
      cParamIndex += 2;
    } else if (c.kind == CoercionKind::Ignore) {
      // A zero-sized arg is ignored. Consume none, conjure an undef for the value.
      auto structKind = peel_all_references(valeParamRefMT);
      auto valeStructLE = LLVMGetUndef(globalState->getRegion(structKind)->translateType(structKind));
      argsToBody.push_back(toRef(globalState->getRegion(structKind), valeParamRefMT, valeStructLE));
    } else {
      auto cArgLE = LLVMGetParam(wrapperL, cParamIndex);
      argsToBody.push_back(
          receiveHostObjectIntoVale(globalState, &functionState, builder, valeParamRefMT, cArgLE));
      cParamIndex += 1;
    }
  }

  auto valeReturnRefOrVoid =
      buildCallV(globalState, &functionState, builder, prototypeM, argsToBody);

  if (prototypeM->returnType == globalState->metalCache->voidType
      || abi->ret.kind == CoercionKind::Ignore) {
    // Vale void, or a zero-sized struct return. Either way, ret void.
    LLVMBuildRetVoid(builder);
  } else {
    auto hostReturnRefLE =
        sendValeObjectIntoHost(
            globalState, &functionState, builder, prototypeM->returnType, valeReturnRefOrVoid);
    if (usingReturnOutParam) {
      LLVMBuildStore(builder, hostReturnRefLE, LLVMGetParam(wrapperL, 0));
      LLVMBuildRetVoid(builder);
    } else if (abi->ret.kind == CoercionKind::Pair) {
      auto slot = makeBackendLocal(
          &functionState, builder, LLVMTypeOf(hostReturnRefLE), "retPairSlot", hostReturnRefLE);
      auto aggLE = LLVMBuildLoad2(
          builder, wrapperReturnLT,
          LLVMBuildBitCast(builder, slot, LLVMPointerType(wrapperReturnLT, 0), "retPairAsAgg"),
          "retPairAgg");
      LLVMBuildRet(builder, aggLE);
    } else {
      LLVMBuildRet(builder, hostReturnRefLE);
    }
  }

  LLVMDisposeBuilder(builder);
}
