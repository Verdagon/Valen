#include "aliasing.h"

#include <algorithm>

#include "../globalstate.h"
#include "../function/function.h"

// VCOORD: revisit

std::vector<uint32_t> noaliasComplement(const std::vector<uint32_t>& touched, uint32_t groupCount) {
  std::vector<uint32_t> out;
  for (uint32_t i = 0; i < groupCount; i++) {
    if (std::find(touched.begin(), touched.end(), i) == touched.end()) {
      out.push_back(i);
    }
  }
  return out;
}

static LLVMMetadataRef makeSelfRefMDNode(LLVMContextRef ctx, LLVMMetadataRef* extraOps, size_t extraCount) {
  LLVMMetadataRef tmp = LLVMTemporaryMDNode(ctx, nullptr, 0);
  std::vector<LLVMMetadataRef> ops;
  ops.push_back(tmp);
  for (size_t i = 0; i < extraCount; i++) {
    ops.push_back(extraOps[i]);
  }
  LLVMMetadataRef node = LLVMMDNodeInContext2(ctx, ops.data(), ops.size());
  LLVMMetadataReplaceAllUsesWith(tmp, node);
  return node;
}

// The alias scope for `id` in this function, created lazily (with its domain) and cached.
static LLVMMetadataRef aliasScopeFor(GlobalState* globalState, FunctionState* functionState, uint32_t id) {
  auto ctx = globalState->context;
  if (functionState->aliasDomain == nullptr) {
    functionState->aliasDomain = makeSelfRefMDNode(ctx, nullptr, 0);
  }
  auto it = functionState->aliasScopes.find(id);
  if (it != functionState->aliasScopes.end()) {
    return it->second;
  }
  LLVMMetadataRef domain = functionState->aliasDomain;
  LLVMMetadataRef scope = makeSelfRefMDNode(ctx, &domain, 1);
  functionState->aliasScopes[id] = scope;
  return scope;
}

void attachNoalias(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMValueRef inst,
    const std::vector<uint32_t>& noaliasIds) {
  if (globalState->opt->suppress_alias_metadata) {
    return;
  }
  if (noaliasIds.empty()) {
    return;
  }
  auto ctx = globalState->context;
  std::vector<LLVMMetadataRef> noaliasNodes;
  for (uint32_t id : noaliasIds) {
    noaliasNodes.push_back(aliasScopeFor(globalState, functionState, id));
  }
  LLVMMetadataRef noaliasList = LLVMMDNodeInContext2(ctx, noaliasNodes.data(), noaliasNodes.size());
  LLVMSetMetadata(
      inst,
      LLVMGetMDKindIDInContext(ctx, "noalias", 7),
      LLVMMetadataAsValue(ctx, noaliasList));
}

void attachAliasScope(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMValueRef inst,
    const std::vector<uint32_t>& aliasScopeIds,
    const std::vector<uint32_t>& noaliasIds) {
  if (globalState->opt->suppress_alias_metadata) {
    return;
  }
  auto ctx = globalState->context;
  if (!aliasScopeIds.empty()) {
    std::vector<LLVMMetadataRef> scopeNodes;
    for (uint32_t id : aliasScopeIds) {
      scopeNodes.push_back(aliasScopeFor(globalState, functionState, id));
    }
    LLVMMetadataRef scopeList = LLVMMDNodeInContext2(ctx, scopeNodes.data(), scopeNodes.size());
    LLVMSetMetadata(
        inst,
        LLVMGetMDKindIDInContext(ctx, "alias.scope", 11),
        LLVMMetadataAsValue(ctx, scopeList));
  }
  attachNoalias(globalState, functionState, inst, noaliasIds);
}

void attachAccessAliasScope(
    GlobalState* globalState,
    FunctionState* functionState,
    LLVMValueRef inst,
    const std::vector<uint32_t>& groupIndices,
    uint32_t groupCount) {
  // This access is in `groupIndices`; every *other* group is disjoint and cannot alias it.
  attachAliasScope(globalState, functionState, inst, groupIndices, noaliasComplement(groupIndices, groupCount));
}

// /VCOORD