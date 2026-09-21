
#include "function/expressions/expressions.h"
#include "externs.h"
#include "globalstate.h"
#include "utils/definefunction.h"

Externs::Externs(LLVMModuleRef mod, LLVMContextRef context, int ptrSizeBits) {
  auto emptyLT = LLVMStructTypeInContext(context, nullptr, 0, false);
  auto emptyPtrLT = LLVMPointerType(emptyLT, 0);
  auto voidLT = LLVMVoidTypeInContext(context);
  auto int1LT = LLVMInt1TypeInContext(context);
  auto int8LT = LLVMInt8TypeInContext(context);
  auto int32LT = LLVMInt32TypeInContext(context);
  auto int32PtrLT = LLVMPointerType(int32LT, 0);
  auto int64LT = LLVMInt64TypeInContext(context);
  auto voidPtrLT = LLVMPointerType(int8LT, 0);
  auto int8PtrLT = LLVMPointerType(int8LT, 0);
  auto sizeTLT = LLVMIntTypeInContext(context, ptrSizeBits);
  auto cIntLT = int32LT;

  censusContains = addExtern(mod, "__vcensusContains", int64LT, {voidPtrLT});
  censusAdd = addExtern(mod, "__vcensusAdd", voidLT, {voidPtrLT});
  censusRemove = addExtern(mod, "__vcensusRemove", voidLT, {voidPtrLT});
  malloc = addExtern(mod, "malloc", int8PtrLT, {sizeTLT});
  free = addExtern(mod, "free", voidLT, {int8PtrLT});
  exit = addExtern(mod, "exit", voidLT, {cIntLT});
  perror = addExtern(mod, "perror", voidLT, {int8PtrLT});
  assert = addExtern(mod, "__vassert", voidLT, {int1LT, int8PtrLT});
  assertI64Eq = addExtern(mod, "__vassertI64Eq", voidLT, {int64LT, int64LT, int8PtrLT});
  printCStr = addExtern(mod, "__vprintCStr", voidLT, {int8PtrLT});
  printCStrToStderr = addExtern(mod, "__vprintCStrToStderr", voidLT, {int8PtrLT});
  getch = addExtern(mod, "getchar", cIntLT, {});
  printInt = addExtern(mod, "__vprintI64", voidLT, {int64LT});
  printIntToStderr = addExtern(mod, "__vprintI64ToStderr", voidLT, {int64LT});
  strlen = addExtern(mod, "strlen", sizeTLT, {int8PtrLT});
  strncpy = addExtern(mod, "strncpy", int8PtrLT, {int8PtrLT, int8PtrLT, sizeTLT});
  strncmp = addExtern(mod, "strncmp", cIntLT, {int8PtrLT, int8PtrLT, sizeTLT});
  memcpy = addExtern(mod, "memcpy", int8PtrLT, {int8PtrLT, int8PtrLT, sizeTLT});
  memset = addExtern(mod, "memset", int8PtrLT, {int8PtrLT, cIntLT, sizeTLT});

  fopen = addExtern(mod, "fopen", int8PtrLT, {int8PtrLT, int8PtrLT});
  fclose = addExtern(mod, "fclose", cIntLT, {int8PtrLT});
  fread = addExtern(mod, "fread", sizeTLT, {int8PtrLT, sizeTLT, sizeTLT, int8PtrLT});
  fwrite = addExtern(mod, "fwrite", sizeTLT, {int8PtrLT, sizeTLT, sizeTLT, int8PtrLT});

  valeRtI64ToAsciiLF   = addExtern(mod, "__vale_rt_i64_to_ascii",   int32LT, {int64LT, int8PtrLT, int32LT});
  valeRtFloatToAsciiLF = addExtern(mod, "__vale_rt_float_to_ascii", int32LT, {LLVMDoubleTypeInContext(context), int8PtrLT, int32LT});
  valeRtBytesFindLF    = addExtern(mod, "__vale_rt_bytes_find",     int32LT, {int8PtrLT, int32LT, int8PtrLT, int32LT});
  valeRtWriteStdoutLF  = addExtern(mod, "__vale_rt_write_stdout",   voidLT,  {int8PtrLT, int32LT});
  valeRtGetMainArgLenLF = addExtern(mod, "__vale_rt_get_main_arg_len", int32LT,   {int64LT});
  valeRtGetMainArgPtrLF = addExtern(mod, "__vale_rt_get_main_arg_ptr", int8PtrLT, {int64LT});

//  initTwinPages = addExtern(mod, "__vale_initTwinPages", int8PtrLT, {});
}



