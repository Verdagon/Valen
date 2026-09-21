#ifndef FUNCTION_DEBUGGING_H
#define FUNCTION_DEBUGGING_H

#include <string>
#include <llvm-c/Core.h>
#include <llvm-c/DebugInfo.h>

class GlobalState;
class Kind;
class Function;
class Local;
class FunctionState;


void initDebugInfo(GlobalState* globalState);
void finalizeDebugInfo(GlobalState* globalState);

void attachDISubprogram(
    GlobalState* globalState,
    LLVMValueRef functionLF,
    const std::string& linkageName,
    Function* functionM);

LLVMMetadataRef getOrCreateDIType(GlobalState* globalState, Kind* kind);

LLVMMetadataRef makeOpaqueRefDIType(GlobalState* globalState);

LLVMMetadataRef getOrCreateDIPointerType(GlobalState* globalState, Kind* localType);

void emitLocalVariableDebugInfo(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMBuilderRef builder,
    Local* local,
    LLVMValueRef localAddr);

class ScopedDebugLoc {
  LLVMBuilderRef builder;
  LLVMMetadataRef saved;
 public:
  explicit ScopedDebugLoc(LLVMBuilderRef b)
      : builder(b), saved(LLVMGetCurrentDebugLocation2(b)) {}
  ~ScopedDebugLoc() {
    if (saved) {
      LLVMSetCurrentDebugLocation2(builder, saved);
    }
  }
  ScopedDebugLoc(const ScopedDebugLoc&) = delete;
  ScopedDebugLoc& operator=(const ScopedDebugLoc&) = delete;
};

#endif
