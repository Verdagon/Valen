#ifndef BOUNDARY_H_
#define BOUNDARY_H_

#include <vector>

#include "../globalstate.h"
#include "boundary.h"

struct BoundarySignature {
  LLVMTypeRef returnLT;
  std::vector<LLVMTypeRef> paramTypesL;
  bool usesReturnOutParam;
};

bool translatesToCVoid(GlobalState* globalState, ValueKind* returnMT);
bool returnNeedsOutParam(GlobalState* globalState, Kind* returnRefMT);
LLVMTypeRef translateExternReturnType(GlobalState* globalState, Kind* returnRefMT);
BoundarySignature buildBoundarySignature(GlobalState* globalState, Prototype* prototypeM);

const ExternAbi* lookupExternAbi(GlobalState* globalState, Prototype* prototypeM);
const std::vector<bool>* lookupParamNoalias(GlobalState* globalState, Prototype* prototypeM);

Ref receiveHostObjectIntoVale(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMBuilderRef builder,
    Kind* valeRefMT,
    LLVMValueRef hostRefLE);

LLVMValueRef sendValeObjectIntoHost(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMBuilderRef builder,
    Kind* valeRefMT,
    Ref valeRef);

#endif
