#include <stdint.h>

#include "vtest/Thing.h"
#include "vtest/makeHelloThing.h"
#include "vtest/runExtCommand.h"

vtest_ThingRef vtest_runExtCommand() {
  return vtest_makeHelloThing(37);
}
