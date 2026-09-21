
#ifndef valeopts_h
#define valeopts_h

#include <string>
#include <stdint.h>
#include <stddef.h>

#include "backend_options_ffi.h"

enum class ValeOptimizationLevel {
    O0,
    O1,
    O2,
    O2i,
    O3
};


struct ValeOptions {
    std::string outputDir;

    std::string triple;
    std::string cpu;

    ValeOptimizationLevel optLevel = ValeOptimizationLevel::O2i;
    bool pic = false;
    bool verify = false;
    bool print_asm = false;
    bool print_llvmir = false;
    bool census = false;
    bool flares = false;
    bool includeBoundsChecks = true;
    bool useAtomicRc = false;
    bool printMemOverhead = false;
    bool debug = false;
    // Suppress every aliasing optimization hint (`!alias.scope`/`!noalias` metadata,
    // parameter `noalias` attribute), keeping `nounwind`. Test-only.
    bool suppress_alias_metadata = false;
};

// Copy fields out of the FFI POD into a ValeOptions. Returns 1 on success,
// 0 or negative on malformed input.
int loadFromFfi(ValeOptions *opt, const BackendCompileOptionsFFI *ffi);

#endif
