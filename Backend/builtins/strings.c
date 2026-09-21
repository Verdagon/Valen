
#include <stdint.h>
#include <string.h>
#include <stdio.h>

// Format an int64 into `buffer` as decimal ASCII. Returns bytes written
// (excluding null terminator).
int32_t __vale_rt_i64_to_ascii(int64_t n, char* buffer, int32_t bufferSize) {
  int written = snprintf(buffer, bufferSize, "%lld", (long long)n);
  return (int32_t)written;
}

// Format a double into `buffer` as ASCII. Returns bytes written.
int32_t __vale_rt_float_to_ascii(double f, char* buffer, int32_t bufferSize) {
  int written = snprintf(buffer, bufferSize, "%lf", f);
  return (int32_t)written;
}

// Find `needle` in `haystack`. Returns byte offset within haystack, or -1.
int32_t __vale_rt_bytes_find(
    const char* haystack, int32_t haystackLen,
    const char* needle, int32_t needleLen) {
  if (needleLen == 0) return 0;
  if (needleLen > haystackLen) return -1;
  for (int32_t i = 0; i <= haystackLen - needleLen; i++) {
    if (memcmp(haystack + i, needle, (size_t)needleLen) == 0) {
      return i;
    }
  }
  return -1;
}

// Write `len` bytes from `bytes` to stdout.
void __vale_rt_write_stdout(const char* bytes, int32_t len) {
  fwrite(bytes, 1, (size_t)len, stdout);
}

