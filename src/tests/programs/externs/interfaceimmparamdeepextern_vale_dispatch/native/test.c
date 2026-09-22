#include <stdint.h>

#include "vtest/IShip.h"
#include "vtest/IShip_getFuel.h"

ValeInt vtest_cGetShipFuel(vtest_IShip s) {
  return vtest_IShip_getFuel(s);
}
