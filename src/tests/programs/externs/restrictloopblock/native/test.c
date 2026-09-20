#include <stdint.h>

#include "vtest/noopBarrier.h"

// An opaque no-op. Its body is invisible to the Vale module's optimizer (separate object, no LTO), so the
// call stays opaque: LLVM cannot prove on its own that it doesn't touch the Ship, and only the block-scoped
// !alias.scope/!noalias metadata + nounwind let each field stay in a register across the loop's calls.
extern void vtest_noopBarrier() {
}
