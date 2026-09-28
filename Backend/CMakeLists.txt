# Backend TODO:
#  * Remove the outgoing extern wrappers that we only have for C ABI interop.
#    I _think_ we can ask LLVM for the right information, if we really twist
#    its arm. Last resort we can copy the code from clang that does it.
#  * Remove the incoming FFI wrappers, and conform our ABI so that we don't
#    need them. They're just more complexity and probably reduce inlining
#    quality.
#  * Reduce GlobalState, it's grown way too big. Also a lot of the things in
#    there could probably be put into the Program struct.
#  * Redo all of the extern handling, make it more general, so that we don't
#    have two separate ABI areas.
#  * Optimize more like other compilers, so that benchmark comparisons can
#    be accurate.
#  * Figure out a better way to emit all the !alias.scope and !noalias
#    information, spidey sense says there's a better way. It feels like a
#    smuggled-in side channel (which, TBF, it is, since borrow checking
#    isn't always enabled), but maybe it can be some optionals on the
#    instructions/calls or something.
#  * Figure out this cross-language debugging nonsense, make sure we sync up
#    with the debug information from other compilers.

cmake_minimum_required(VERSION 3.5)
project(backend)

set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -g -std=c++17 -fvisibility=hidden")


if (MSVC)
	# Set the stack size to 4mb instead of the default 1mb for visual studio
	# because for some reason 1mb is too small for the backend on Windows.
	# Also, debug information overflows the stack even on non-Windows machines.
	set(CMAKE_EXE_LINKER_FLAGS "${CMAKE_EXE_LINKER_FLAGS} /STACK:4194304")
endif (MSVC)

# No version pin: build.rs points LLVM_DIR at the Vale rustc fork's shared libLLVM
# (currently 21.1.8-rust-dev, whose `-rust-dev` version suffix defeats a numeric
# `find_package(LLVM <N>)` match anyway). The toolchain controls the version.
find_package(LLVM REQUIRED CONFIG)
add_definitions(${LLVM_DEFINITIONS})
include_directories(
	${LLVM_INCLUDE_DIRS}
	"${CMAKE_SOURCE_DIR}/src/"
)

message(STATUS "LLVM VERSION : ${LLVM_PACKAGE_VERSION}")
message(STATUS "Using LLVMConfig.cmake in: ${LLVM_DIR}")

# See either:
#   llvm-config --libfiles --link-static
#   llvm-config --components | tr " " "\n"
# for what we could use here.
# We'll keep it to a minimum for now, because different machines
# seem to support different components.
set(LLVM_LINK_COMPONENTS
		Core
		Support
		IRReader
		Passes
		aarch64asmparser aarch64codegen aarch64desc aarch64disassembler aarch64info
		x86asmparser x86codegen x86desc x86disassembler x86info
		webassemblyasmparser webassemblycodegen webassemblydesc webassemblydisassembler webassemblyinfo
)

llvm_map_components_to_libnames(llvm_libs ${LLVM_LINK_COMPONENTS})

set(BACKEND_LIB_SRCS
		src/vale.cpp
		src/globalstate.cpp
		src/metal/ast.cpp
		src/metal/metal_cache_ffi.cpp
		src/metal/types.cpp
		src/translatetype.cpp
		src/valeopts.cpp
		src/mainFunction.cpp
		src/externs.cpp

		src/utils/counters.cpp
		src/utils/structlt.cpp

		src/function/function.cpp
		src/function/debugging.cpp
		src/function/boundary.cpp
		src/function/expression.cpp

		src/function/expressions/call.cpp
		src/function/expressions/interfacecall.cpp
		src/function/expressions/construct.cpp
		src/function/expressions/destructure.cpp
		src/function/expressions/block.cpp
		src/function/expressions/discard.cpp
		src/function/expressions/externs.cpp
		src/function/expressions/if.cpp
		src/function/expressions/constantstr.cpp
		src/function/expressions/while.cpp
		src/function/expressions/newruntimesizearray.cpp
		src/function/expressions/staticarrayfromcallable.cpp
		src/function/expressions/newarrayfromvalues.cpp

		src/function/expressions/shared/elements.cpp
		src/function/expressions/shared/members.cpp
		src/function/expressions/shared/shared.cpp
		src/utils/branch.cpp
		src/utils/call.cpp
		src/function/expressions/shared/string.cpp
		src/region/common/heap.cpp
		src/region/common/controlblock.cpp
		src/region/ffihandlestructs.cpp
		src/function/expressions/shared/ref.cpp

		src/region/common/common.cpp
		src/region/common/defaultlayout/structs.cpp
		src/region/rcimm/rcimm.cpp
		src/region/common/wrcweaks/wrcweaks.cpp
		src/region/common/fatweaks/fatweaks.cpp
		src/region/unsafe/unsafe.cpp
		src/region/regions.cpp

		src/fileio.cpp
		src/utils/definefunction.cpp
		src/utils/definefunction.h
		src/utils/llvm.cpp
		src/utils/llvm.h
		src/utils/structlt.h
		src/region/common/migration.h
		src/region/common/migration.cpp
		src/region/iregion.cpp

		src/aliasing/aliasing.cpp
)

if(VALE_RUST_INTEROP)
	list(APPEND BACKEND_LIB_SRCS src/rust_interop/rust_interop.cpp)
endif()

add_library(backend_lib STATIC ${BACKEND_LIB_SRCS})
target_link_libraries(backend_lib ${llvm_libs})
target_compile_features(backend_lib PRIVATE cxx_std_17)

if(VALE_RUST_INTEROP)
	target_compile_definitions(backend_lib PRIVATE VALE_RUST_INTEROP)
endif()


