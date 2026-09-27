#ifndef VALE_RUST_INTEROP_H_
#define VALE_RUST_INTEROP_H_

#include <string>

#include "../globalstate.h"
#include "../metal/ast.h"

void emitInboundCallbackWrapper(
    GlobalState* globalState,
    Program* program,
    const std::string& symbol,
    const std::string& valeName);

#endif
