#error "vtest.unused/native/broken.c was compiled"
// If the Frontend-driven walker visits this file, clang fails immediately.
// The walker must skip this dir because vtest.unused isn't reached by the
// program at vtest/test.vale (which neither imports nor calls anything here).
