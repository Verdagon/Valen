#include <stdint.h>

#include "vtest/IShip.h"
#include "vtest/IShip_getFuel.h"

// We use incrementIntFile to get some side effects to test replayability, see AASETR.
int64_t incrementIntFile(const char* filename);

ValeInt vtest_cGetShipFuel(vtest_IShip s) {
  int runNumber = incrementIntFile("myfile.bin");
  return vtest_IShip_getFuel(s) * runNumber;
}
