#include <stdint.h>
#include <string.h>
#include <assert.h>

#include "vtest/getAStr.h"
#include "vtest/makeRepeatingHello.h"
#include "vtest/str_alias.h"
#include "vtest/str_len.h"
#include "vtest/str_char_at.h"
#include "vtest/str_dealias.h"

// We use incrementIntFile to get some side effects to test replayability, see AASETR.
int64_t incrementIntFile(const char* filename);

vtest_str vtest_runExtCommand() {
  int runNumber = incrementIntFile("myfile.bin");

  vtest_str str = vtest_getAStr();   // owned 1

  assert(vtest_str_len(vtest_str_alias(str)) == 6);
  char buf[7];
  for (int i = 0; i < 6; i++) {
    buf[i] = vtest_str_char_at(vtest_str_alias(str), i);
  }
  buf[6] = 0;
  int diff = strncmp(buf, "hello!", 6);
  assert(diff == 0);

  vtest_str_dealias(str);   // discharge the original owned count

  return vtest_makeRepeatingHello(runNumber);
}
