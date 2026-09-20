#include <stdint.h>

#include "vtest/noopBarrier.h"

// An opaque no-op. Its body is invisible to the Vale module's optimizer (separate object, no LTO), so the
// call stays opaque: LLVM cannot prove it doesn't read `a.fuel` on its own, and only the `!noalias`
// metadata lets the redundant `set a.fuel = 2` be dead-store-eliminated across it.
extern void vtest_noopBarrier() {
}
