
#ifndef backend_options_ffi_h
#define backend_options_ffi_h

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BACKEND_OPT_LEVEL_O0  0
#define BACKEND_OPT_LEVEL_O1  1
#define BACKEND_OPT_LEVEL_O2  2
#define BACKEND_OPT_LEVEL_O2i 3
#define BACKEND_OPT_LEVEL_O3  4

typedef struct BackendCompileOptionsFFI {
  // NUL-terminated UTF-8 strings owned by the caller. Empty ("") means
  // "use the LLVM/default value" for triple and cpu.
  const char* output_dir;
  const char* triple;
  const char* cpu;

  int32_t opt_level;
  uint8_t pic;
  uint8_t verify;
  uint8_t print_asm;
  uint8_t print_llvmir;
  uint8_t census;
  uint8_t flares;
  uint8_t include_bounds_checks;
  uint8_t use_atomic_rc;
  uint8_t print_mem_overhead;
  uint8_t debug;
  uint8_t suppress_alias_metadata;
} BackendCompileOptionsFFI;

// Compile mode selector for BackendInputsFFI.mode.
#define BACKEND_MODE_STANDALONE 0

typedef struct SourceFilePathFFI {
  const char* basename;
  const char* abspath;
} SourceFilePathFFI;

typedef struct BackendInputsFFI {
  void* cache;                     // MetalCacheHandle*
  void* program;                   // ProgramHandle*
  BackendCompileOptionsFFI options;
  int32_t mode;                    // BACKEND_MODE_*
  const SourceFilePathFFI* source_paths;
  size_t num_source_paths;
} BackendInputsFFI;

#ifdef __cplusplus
} // extern "C"
#endif

#endif
