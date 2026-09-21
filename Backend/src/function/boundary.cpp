#include <vector>

#include "../globalstate.h"
#include "expressions/expressions.h"
#include "boundary.h"
#include "../region/iregion.h"

bool translatesToCVoid(GlobalState* globalState, ValueKind* returnMT) {
  return returnMT == globalState->metalCache->neverType
      || returnMT == globalState->metalCache->voidType;
}

static LLVMTypeRef hostBoundaryType(GlobalState* globalState, Kind* valeRefMT) {
  auto valeKind = peel_all_references(valeRefMT);
  auto valueLT = globalState->getRegion(valeKind)->getExternalType(valeKind);
  auto voidLT = LLVMVoidTypeInContext(globalState->context);
  if (dynamic_cast<BorrowRef*>(valeRefMT)) {
    return LLVMPointerType(valueLT, 0);
  } else if (dynamic_cast<OwnRef*>(valeRefMT)) {
    return LLVMPointerType(valueLT, 0);
  } else if (dynamic_cast<ShareRef*>(valeRefMT)) {
    return LLVMPointerType(valueLT, 0);
  } else if (dynamic_cast<WeakRef*>(valeRefMT)) {
    return LLVMPointerType(valueLT, 0);
  } else if (dynamic_cast<Void*>(valeRefMT)) {
    return voidLT;
  } else if (dynamic_cast<Never*>(valeRefMT)) {
    return voidLT;
  } else if (dynamic_cast<Bool*>(valeRefMT)) {
    return valueLT;
  } else if (dynamic_cast<Int*>(valeRefMT)) {
    return valueLT;
  } else if (dynamic_cast<Float*>(valeRefMT)) {
    return valueLT;
  } else if (dynamic_cast<USize*>(valeRefMT)) {
    return valueLT;
  } else if (dynamic_cast<Str*>(valeRefMT)) {
    return valueLT;
  } else if (dynamic_cast<StructKind*>(valeRefMT)) {
    return valueLT;
  } else if (dynamic_cast<InterfaceKind*>(valeRefMT)) {
    return valueLT;
  } else if (dynamic_cast<StaticSizedArrayT*>(valeRefMT)) {
    return valueLT;
  } else if (dynamic_cast<RuntimeSizedArrayT*>(valeRefMT)) {
    return valueLT;
  } else {
    { assert(false); throw 1337; }
  }
}

bool returnNeedsOutParam(GlobalState* globalState, Kind* returnRefMT) {
  return LLVMGetTypeKind(hostBoundaryType(globalState, returnRefMT)) == LLVMStructTypeKind;
}

LLVMTypeRef translateExternReturnType(GlobalState* globalState, Kind* returnRefMT) {
  if (returnNeedsOutParam(globalState, returnRefMT)) {
    return LLVMVoidTypeInContext(globalState->context);
  }
  return hostBoundaryType(globalState, returnRefMT);
}

// VCOORD: revisit this
const ExternAbi* lookupExternAbi(GlobalState* globalState, Prototype* prototypeM) {
  auto pkgIter = globalState->program->packages.find(prototypeM->name->packageCoord);
  if (pkgIter == globalState->program->packages.end()) {
    return nullptr;
  }
  auto& externAbis = pkgIter->second->externAbis;
  auto iter = externAbis.find(prototypeM->name->name);
  return iter == externAbis.end() ? nullptr : &iter->second;
}

const std::vector<bool>* lookupParamNoalias(GlobalState* globalState, Prototype* prototypeM) {
  auto pkgIter = globalState->program->packages.find(prototypeM->name->packageCoord);
  if (pkgIter == globalState->program->packages.end()) {
    return nullptr;
  }
  auto& paramNoaliasByName = pkgIter->second->paramNoaliasByName;
  auto iter = paramNoaliasByName.find(prototypeM->name->name);
  return iter == paramNoaliasByName.end() ? nullptr : &iter->second;
}

// VCOORD: revisit this
BoundarySignature buildBoundarySignature(GlobalState* globalState, Prototype* prototypeM) {
  if (const ExternAbi* abi = lookupExternAbi(globalState, prototypeM)) {
    auto voidLT = LLVMVoidTypeInContext(globalState->context);
    auto ptrLT = LLVMPointerType(LLVMInt8TypeInContext(globalState->context), 0);
    bool usesReturnOutParam = abi->ret.kind == CoercionKind::Indirect;
    std::vector<LLVMTypeRef> paramTypesL;
    if (usesReturnOutParam) {
      paramTypesL.push_back(ptrLT);  // first parameter
    }
    for (const Coercion& c : abi->args) {
      switch (c.kind) {
        case CoercionKind::Ignore: break;  // zero-sized: not passed at all
        // DirectInt and Cast both cross as a single integer of directIntBits bits.
        case CoercionKind::DirectInt:
        case CoercionKind::Cast:
          paramTypesL.push_back(LLVMIntTypeInContext(globalState->context, c.directIntBits)); break;
        case CoercionKind::DirectPtr:
        case CoercionKind::Indirect:
        case CoercionKind::LocationPtr: // implicit `#[track_caller]` (see @TCHAPZ).
          paramTypesL.push_back(ptrLT);
          break;
        // A ScalarPair struct crosses as two separate integer register params.
        case CoercionKind::Pair:
          paramTypesL.push_back(LLVMIntTypeInContext(globalState->context, c.directIntBits));
          paramTypesL.push_back(LLVMIntTypeInContext(globalState->context, c.directIntBits2));
          break;
      }
    }
    bool hasLocationArg = !abi->args.empty() && abi->args.back().kind == CoercionKind::LocationPtr;
    assert(abi->args.size() - (hasLocationArg ? 1u : 0u) == prototypeM->params.size());
    LLVMTypeRef returnLT;
    switch (abi->ret.kind) {
      case CoercionKind::Ignore:
      case CoercionKind::Indirect: returnLT = voidLT; break;  // Indirect returns through the out-pointer
      // DirectInt and Cast both return as a single integer of directIntBits bits.
      case CoercionKind::DirectInt:
      case CoercionKind::Cast:
        returnLT = LLVMIntTypeInContext(globalState->context, abi->ret.directIntBits); break;
      case CoercionKind::DirectPtr: returnLT = ptrLT; break;
      // A ScalarPair struct returns in two registers as an {iN, iM} aggregate.
      case CoercionKind::Pair: {
        LLVMTypeRef elems[2] = {
            LLVMIntTypeInContext(globalState->context, abi->ret.directIntBits),
            LLVMIntTypeInContext(globalState->context, abi->ret.directIntBits2),
        };
        returnLT = LLVMStructTypeInContext(globalState->context, elems, 2, /*packed=*/0);
        break;
      }
      default: { assert(false); throw 1337; }
    }
    return BoundarySignature{returnLT, std::move(paramTypesL), usesReturnOutParam};
  }

  bool usesReturnOutParam = returnNeedsOutParam(globalState, prototypeM->returnType);
  std::vector<LLVMTypeRef> paramTypesL;
  if (usesReturnOutParam) {
    paramTypesL.push_back(
        LLVMPointerType(hostBoundaryType(globalState, prototypeM->returnType), 0));
  }
  for (auto valeParamRefMT : prototypeM->params) {
    paramTypesL.push_back(hostBoundaryType(globalState, valeParamRefMT));
  }
  return BoundarySignature{
      translateExternReturnType(globalState, prototypeM->returnType),
      std::move(paramTypesL),
      usesReturnOutParam};
}

Ref receiveHostObjectIntoVale(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMBuilderRef builder,
    Kind* valeRefMT,
    LLVMValueRef hostRefLE) {
  auto valeRefValueType = peel_all_references(valeRefMT);
  if (dynamic_cast<Void*>(valeRefMT)) {
    return toRef(globalState->getRegion(valeRefValueType), valeRefMT, makeVoid(globalState));
  }
  if (dynamic_cast<Bool*>(valeRefMT)) {
    auto asI1LE =
        LLVMBuildTrunc(builder, hostRefLE, LLVMInt1TypeInContext(globalState->context), "boolAsI1");
    return toRef(globalState->getRegion(valeRefValueType), valeRefMT, asI1LE);
  }
  return toRef(globalState->getRegion(valeRefValueType), valeRefMT, hostRefLE);
}

LLVMValueRef sendValeObjectIntoHost(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMBuilderRef builder,
    Kind* valeRefMT,
    Ref valeRef) {
  auto valeRefValueType = peel_all_references(valeRefMT);
  auto valeArgLE =
      globalState->getRegion(valeRefValueType)
          ->checkValidReference(FL(), functionState, builder, true, valeRefMT, valeRef);
  if (dynamic_cast<Bool*>(valeRefMT)) {
    return LLVMBuildZExt(builder, valeArgLE, LLVMInt8TypeInContext(globalState->context), "boolAsI8");
  }
  return valeArgLE;
}
