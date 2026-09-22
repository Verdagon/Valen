#include <stdint.h>

#include "vtest/Widget.h"
#include "vtest/Widget_n.h"
#include "vtest/Widget_alias.h"
#include "vtest/Widget_dealias.h"

ValeInt vtest_testAliasDealias(vtest_Widget w) {
  vtest_Widget_alias(w);
  vtest_Widget_dealias(w);
  return vtest_Widget_n(w);
}
