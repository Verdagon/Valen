#include "debugging.h"

#include <cstring>
#include <string>
#include <vector>

#include <utils/definefunction.h>
#include "expressions/shared/shared.h"
#include "../translatetype.h"
#include "function.h"
#include "expression.h"
#include "boundary.h"
#include <region/common/migration.h>
#include <utils/counters.h>
#include <llvm-c/DebugInfo.h>
#include <llvm-c/Target.h>
#include "metal/instructions.h"
#include "metal/types.h"
#include "metal/ast.h"

static LLVMMetadataRef getOrCreateDIFile(GlobalState* globalState, const std::string& path) {
  auto it = globalState->diFileCache.find(path);
  if (it != globalState->diFileCache.end()) {
    return it->second;
  }
  auto sp = globalState->program->sourcePaths.find(path);
  const std::string& resolved = (sp != globalState->program->sourcePaths.end()) ? sp->second : path;
  auto slash = resolved.find_last_of('/');
  std::string dir = (slash == std::string::npos) ? "." : resolved.substr(0, slash);
  std::string base = (slash == std::string::npos) ? resolved : resolved.substr(slash + 1);
  auto file = LLVMDIBuilderCreateFile(
      globalState->dibuilder, base.c_str(), base.size(), dir.c_str(), dir.size());
  globalState->diFileCache.emplace(path, file);
  return file;
}

static LLVMMetadataRef getOrCreateCompileUnit(
    GlobalState* globalState, LLVMMetadataRef anchorFile) {
  if (globalState->compileUnit) {
    return globalState->compileUnit;
  }
  LLVMMetadataRef cuFile = anchorFile;
  // VCOORD: This is suspicious, source paths shouldnt be empty.
  if (!globalState->program->sourcePaths.empty()) {
    const std::string* chosen = nullptr;
    for (auto& entry : globalState->program->sourcePaths) {
      if (chosen == nullptr || entry.first < *chosen) {
        chosen = &entry.first;
      }
    }
    cuFile = getOrCreateDIFile(globalState, *chosen);
  }
  globalState->compileUnit = LLVMDIBuilderCreateCompileUnit(
      globalState->dibuilder, LLVMDWARFSourceLanguageC, cuFile, "Vale compiler",
      13, 0, "", 0, 0, "", 0, LLVMDWARFEmissionFull, 0, 0, 0, "", 0, "", 0);
  return globalState->compileUnit;
}

void attachDISubprogram(
    GlobalState* globalState,
    LLVMValueRef functionLF,
    const std::string& linkageName,
    Function* functionM) {
  if (!globalState->opt->debug) {
    return;
  }
  if (functionM->sourceLocation == nullptr ||
      functionM->sourceLocation->filePath.empty()) {
    return;
  }

  auto file = getOrCreateDIFile(globalState, functionM->sourceLocation->filePath);
  getOrCreateCompileUnit(globalState, file);
  int32_t line = functionM->sourceLocation->line;
  auto subroutineType = LLVMDIBuilderCreateSubroutineType(
      globalState->dibuilder, file, nullptr, 0, LLVMDIFlagZero);
  auto subprogram = LLVMDIBuilderCreateFunction(
      globalState->dibuilder,
      /*scope*/ file,
      linkageName.c_str(), linkageName.size(),
      linkageName.c_str(), linkageName.size(),
      file, line, subroutineType,
      /*is_local_to_unit*/ true,
      /*is_definition*/ true,
      /*scope_line*/ line,
      LLVMDIFlagZero,
      /*is_optimized*/ false);
  LLVMSetSubprogram(functionLF, subprogram);
}

static constexpr unsigned DW_ATE_ADDRESS = 0x01;
static constexpr unsigned DW_ATE_BOOLEAN = 0x02;
static constexpr unsigned DW_ATE_FLOAT = 0x04;
static constexpr unsigned DW_ATE_SIGNED = 0x05;
static constexpr unsigned DW_TAG_STRUCTURE_TYPE = 0x13;

LLVMMetadataRef makeOpaqueRefDIType(GlobalState* globalState) {
  return LLVMDIBuilderCreateBasicType(
      globalState->dibuilder, "ref", 3, globalState->ptrSize, DW_ATE_ADDRESS, LLVMDIFlagZero);
}

static LLVMMetadataRef getOrCreateDIStructType(GlobalState* globalState, StructKind* kind) {
  auto structM = globalState->lookupStruct(kind);
  const std::string& fullName = kind->fullName->name;
  auto innerLT = LLVMGetTypeByName2(globalState->context, fullName.c_str());
  if (innerLT == nullptr) {
    auto opaque = makeOpaqueRefDIType(globalState);
    globalState->diTypeCache.emplace(kind, opaque);
    return opaque;
  }

  uint64_t sizeBits = LLVMSizeOfTypeInBits(globalState->dataLayout, innerLT);
  auto fwd = LLVMDIBuilderCreateReplaceableCompositeType(
      globalState->dibuilder, DW_TAG_STRUCTURE_TYPE,
      fullName.c_str(), fullName.size(),
      /*Scope*/ globalState->compileUnit ? globalState->compileUnit : nullptr,
      /*File*/ nullptr, /*Line*/ 0, /*RuntimeLang*/ 0,
      sizeBits, /*AlignInBits*/ 0, LLVMDIFlagZero,
      fullName.c_str(), fullName.size());
  globalState->diTypeCache[kind] = fwd;

  std::vector<LLVMMetadataRef> diMembers;
  for (size_t i = 0; i < structM->members.size(); i++) {
    auto sm = structM->members[i];
    auto memberDIType = getOrCreateDIType(globalState, sm->type);
    auto memberLT = LLVMStructGetTypeAtIndex(innerLT, i);
    uint64_t memberSizeBits = 8 * LLVMABISizeOfType(globalState->dataLayout, memberLT);
    uint64_t memberOffsetBits = 8 * LLVMOffsetOfElement(globalState->dataLayout, innerLT, i);
    diMembers.push_back(LLVMDIBuilderCreateMemberType(
        globalState->dibuilder, /*Scope*/ fwd, sm->name.c_str(), sm->name.size(),
        /*File*/ nullptr, /*LineNo*/ 0,
        memberSizeBits, /*AlignInBits*/ 0, memberOffsetBits,
        LLVMDIFlagZero, memberDIType));
  }

  auto real = LLVMDIBuilderCreateStructType(
      globalState->dibuilder,
      /*Scope*/ globalState->compileUnit ? globalState->compileUnit : nullptr,
      fullName.c_str(), fullName.size(),
      /*File*/ nullptr, /*Line*/ 0,
      sizeBits, /*AlignInBits*/ 0, LLVMDIFlagZero,
      /*DerivedFrom*/ nullptr,
      diMembers.data(), diMembers.size(),
      /*RuntimeLang*/ 0, /*VTableHolder*/ nullptr,
      fullName.c_str(), fullName.size());
  LLVMMetadataReplaceAllUsesWith(fwd, real);
  globalState->diTypeCache[kind] = real;
  return real;
}

static LLVMMetadataRef getOrCreateDIArrayType(GlobalState* globalState, StaticSizedArrayT* kind) {
  auto def = globalState->program->getStaticSizedArray(kind);
  auto innerLT = globalState->getRegion(kind)->translateType(kind);
  uint64_t sizeBits = LLVMSizeOfTypeInBits(globalState->dataLayout, innerLT);
  auto elemDI = getOrCreateDIType(globalState, def->elementType);
  LLVMMetadataRef subrange =
      LLVMDIBuilderGetOrCreateSubrange(globalState->dibuilder, /*LowerBound*/ 0, def->size);
  auto arrayDI = LLVMDIBuilderCreateArrayType(
      globalState->dibuilder, sizeBits, /*AlignInBits*/ 0, elemDI, &subrange, 1);
  globalState->diTypeCache.emplace(kind, arrayDI);
  return arrayDI;
}

LLVMMetadataRef getOrCreateDIType(GlobalState* globalState, Kind* kind) {
  auto it = globalState->diTypeCache.find(kind);
  if (it != globalState->diTypeCache.end()) {
    return it->second;
  }
  if (auto structKind = dynamic_cast<StructKind*>(kind)) {
    return getOrCreateDIStructType(globalState, structKind);
  }
  if (auto ssaMT = dynamic_cast<StaticSizedArrayT*>(kind)) {
    return getOrCreateDIArrayType(globalState, ssaMT);
  }
  LLVMMetadataRef di = nullptr;
  if (auto intK = dynamic_cast<Int*>(kind)) {
    auto name = std::string("i") + std::to_string(intK->bits);
    di = LLVMDIBuilderCreateBasicType(
        globalState->dibuilder, name.c_str(), name.size(),
        intK->bits, DW_ATE_SIGNED, LLVMDIFlagZero);
  } else if (dynamic_cast<Bool*>(kind)) {
    di = LLVMDIBuilderCreateBasicType(
        globalState->dibuilder, "bool", 4, 8, DW_ATE_BOOLEAN, LLVMDIFlagZero);
  } else if (dynamic_cast<Float*>(kind)) {
    di = LLVMDIBuilderCreateBasicType(
        globalState->dibuilder, "f64", 3, 64, DW_ATE_FLOAT, LLVMDIFlagZero);
  } else {
    di = makeOpaqueRefDIType(globalState);
  }
  globalState->diTypeCache.emplace(kind, di);
  return di;
}

LLVMMetadataRef getOrCreateDIPointerType(GlobalState* globalState, Kind* localType) {
  if (auto borrowRef = dynamic_cast<BorrowRef*>(localType)) {
    if (auto structKind = dynamic_cast<StructKind*>(borrowRef->inner)) {
      auto pointee = getOrCreateDIStructType(globalState, structKind);
      return LLVMDIBuilderCreatePointerType(
          globalState->dibuilder, pointee, globalState->ptrSize,
          /*AlignInBits*/ 0, /*AddressSpace*/ 0, "", 0);
    }
  }
  return makeOpaqueRefDIType(globalState);
}

void emitLocalVariableDebugInfo(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMBuilderRef builder,
    Local* local,
    LLVMValueRef localAddr) {
  if (!globalState->opt->debug || local->sourceLocation == nullptr || local->name.empty()) {
    return;
  }
  auto subprogram = LLVMGetSubprogram(functionState->containingFuncL);
  if (!subprogram) {
    return;
  }
  int32_t line = local->sourceLocation->line;
  auto scopeFile = LLVMDIScopeGetFile(subprogram);
  auto storageLT = globalState->getRegion(local->type)->translateType(local->type);
  LLVMMetadataRef diType =
      (LLVMGetTypeKind(storageLT) == LLVMPointerTypeKind)
          ? getOrCreateDIPointerType(globalState, local->type)
          : getOrCreateDIType(globalState, local->type);
  const std::string& name = local->name;
  auto diVar = LLVMDIBuilderCreateAutoVariable(
      globalState->dibuilder, subprogram, name.c_str(), name.size(),
      scopeFile, line, diType,
      /*AlwaysPreserve*/ true, LLVMDIFlagZero, /*AlignInBits*/ 0);
  auto diExpr = LLVMDIBuilderCreateExpression(globalState->dibuilder, nullptr, 0);
  auto diLoc = LLVMDIBuilderCreateDebugLocation(
      globalState->context, line, 1, subprogram, /*inlinedAt*/ nullptr);
  LLVMDIBuilderInsertDeclareRecordAtEnd(
      globalState->dibuilder, localAddr, diVar, diExpr, diLoc,
      LLVMGetInsertBlock(builder));
}

void initDebugInfo(GlobalState* globalState) {
  if (!globalState->opt->debug) {
    return;
  }
  globalState->dibuilder = LLVMCreateDIBuilder(globalState->mod);
  auto i32Ty = LLVMInt32TypeInContext(globalState->context);
  LLVMAddModuleFlag(globalState->mod, LLVMModuleFlagBehaviorWarning,
      "Dwarf Version", strlen("Dwarf Version"),
      LLVMValueAsMetadata(LLVMConstInt(i32Ty, 4, 0)));
  LLVMAddModuleFlag(globalState->mod, LLVMModuleFlagBehaviorWarning,
      "Debug Info Version", strlen("Debug Info Version"),
      LLVMValueAsMetadata(LLVMConstInt(i32Ty, 3 /* LLVM DEBUG_METADATA_VERSION */, 0)));
}

void finalizeDebugInfo(GlobalState* globalState) {
  if (globalState->dibuilder) {
    LLVMDIBuilderFinalize(globalState->dibuilder);
  }
}
