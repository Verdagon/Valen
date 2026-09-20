#include <stdint.h>

#include "vtest/noopBarrier.h"

// An opaque no-op. Its body is invisible to the Vale module's optimizer (separate object, no LTO), so the
// call stays opaque: LLVM cannot prove it doesn't touch `a.fuel` on its own, and only the `!noalias`
// metadata lets the two reads across it coalesce.
extern void vtest_noopBarrier() {
}
