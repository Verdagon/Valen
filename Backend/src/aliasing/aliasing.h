#ifndef VALE_ALIASING_ALIASING_H_
#define VALE_ALIASING_ALIASING_H_

#include <cstdint>
#include <vector>

#include <llvm-c/Core.h>

class GlobalState;
class FunctionState;

// VCOORD: revisit

std::vector<uint32_t> noaliasComplement(const std::vector<uint32_t>& touched, uint32_t groupCount);

void attachAccessAliasScope(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMValueRef inst,
    const std::vector<uint32_t>& groupIndices,
    uint32_t groupCount);

void attachAliasScope(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMValueRef inst,
    const std::vector<uint32_t>& aliasScopeIds,
    const std::vector<uint32_t>& noaliasIds);

void attachNoalias(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMValueRef inst,
    const std::vector<uint32_t>& noaliasIds);

// /VCOORD

#endif
