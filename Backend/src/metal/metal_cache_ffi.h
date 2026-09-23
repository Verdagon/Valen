
#ifndef METAL_CACHE_FFI_H_
#define METAL_CACHE_FFI_H_

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct MetalCacheHandle      MetalCacheHandle;
typedef struct PackageCoordHandle    PackageCoordHandle;
typedef struct RegionIdHandle        RegionIdHandle;
typedef struct NameHandle            NameHandle;
typedef struct KindHandle            KindHandle;
typedef struct PrototypeHandle       PrototypeHandle;
typedef struct LocalHandle           LocalHandle;
typedef struct InterfaceMethodHandle InterfaceMethodHandle;
typedef struct StructMemberHandle    StructMemberHandle;
typedef struct EdgeHandle            EdgeHandle;
typedef struct StructDefHandle       StructDefHandle;
typedef struct InterfaceDefHandle    InterfaceDefHandle;
typedef struct FunctionHandle        FunctionHandle;
typedef struct ExpressionHandle      ExpressionHandle;
typedef struct SourceLocationHandle  SourceLocationHandle;
typedef struct PackageHandle         PackageHandle;
typedef struct ProgramHandle         ProgramHandle;
typedef struct StaticSizedArrayDefHandle StaticSizedArrayDefHandle;
typedef struct RuntimeSizedArrayDefHandle RuntimeSizedArrayDefHandle;
typedef struct PackageBuilderHandle  PackageBuilderHandle;
typedef struct ProgramBuilderHandle  ProgramBuilderHandle;

typedef struct CoercionFFI {
  uint32_t kind;
  uint32_t bits;
  uint32_t bits2;
} CoercionFFI;


MetalCacheHandle* metal_cache_new(void);
void              metal_cache_free(MetalCacheHandle*);


PackageCoordHandle* metal_cache_builtin_package_coord(MetalCacheHandle*);
RegionIdHandle*     metal_cache_rcimm_region_id(MetalCacheHandle*);
RegionIdHandle*     metal_cache_mut_region_id(MetalCacheHandle*);

KindHandle*         metal_cache_i32(MetalCacheHandle*);
KindHandle*         metal_cache_i64(MetalCacheHandle*);
KindHandle*         metal_cache_bool(MetalCacheHandle*);
KindHandle*         metal_cache_float(MetalCacheHandle*);
KindHandle*         metal_cache_str(MetalCacheHandle*);
KindHandle*         metal_cache_never(MetalCacheHandle*);
KindHandle*         metal_cache_void(MetalCacheHandle*);


PackageCoordHandle* metal_cache_get_package_coordinate(
    MetalCacheHandle*,
    const char* project_name_ptr, size_t project_name_len,
    const char* const* steps_ptrs, const size_t* steps_lens, size_t steps_count);

RegionIdHandle* metal_cache_get_region_id(
    MetalCacheHandle*,
    PackageCoordHandle* package_coord,
    const char* id_ptr, size_t id_len);

NameHandle* metal_cache_get_name(
    MetalCacheHandle*,
    PackageCoordHandle* package_coord,
    const char* name_ptr, size_t name_len);

KindHandle* metal_cache_get_int(MetalCacheHandle*, RegionIdHandle* region, int32_t bits);
KindHandle* metal_cache_get_bool(MetalCacheHandle*, RegionIdHandle* region);
KindHandle* metal_cache_get_str(MetalCacheHandle*, RegionIdHandle* region);
KindHandle* metal_cache_get_float(MetalCacheHandle*, RegionIdHandle* region);
KindHandle* metal_cache_get_void(MetalCacheHandle*, RegionIdHandle* region);
KindHandle* metal_cache_get_never(MetalCacheHandle*, RegionIdHandle* region);
KindHandle* metal_cache_get_usize(MetalCacheHandle*, RegionIdHandle* region);

KindHandle* metal_cache_get_struct_kind(MetalCacheHandle*, NameHandle* name);
KindHandle* metal_cache_get_interface_kind(MetalCacheHandle*, NameHandle* name);
KindHandle* metal_cache_get_dyn_interface_kind(MetalCacheHandle*, NameHandle* name);
KindHandle* metal_cache_get_static_sized_array(MetalCacheHandle*, NameHandle* name);
KindHandle* metal_cache_get_runtime_sized_array(MetalCacheHandle*, NameHandle* name);

KindHandle* metal_cache_get_borrow_ref(MetalCacheHandle*, KindHandle* inner);
KindHandle* metal_cache_get_own_ref(MetalCacheHandle*, KindHandle* inner);
KindHandle* metal_cache_get_share_ref(MetalCacheHandle*, KindHandle* inner);
KindHandle* metal_cache_get_weak_ref(MetalCacheHandle*, KindHandle* inner);

PrototypeHandle* metal_cache_get_prototype(
    MetalCacheHandle*,
    NameHandle* name,
    KindHandle* return_type,
    KindHandle* const* param_types, size_t param_count);

InterfaceMethodHandle* metal_cache_get_interface_method(
    MetalCacheHandle*, PrototypeHandle* prototype, int32_t virtual_param_index);

LocalHandle* metal_cache_get_local(
    MetalCacheHandle*, const char* id_ptr, size_t id_len,
    const char* name_ptr, size_t name_len, KindHandle* kind, SourceLocationHandle* source_loc);

SourceLocationHandle* metal_cache_get_source_location(
    MetalCacheHandle*, const char* file_ptr, size_t file_len, int32_t line, int32_t col);


StructMemberHandle* metal_struct_member_new(
    const char* full_name_ptr, size_t full_name_len,
    const char* name_ptr, size_t name_len,
    KindHandle* type);

EdgeHandle* metal_edge_new(
    KindHandle* struct_kind,
    KindHandle* interface_kind,
    InterfaceMethodHandle* const* interface_methods,
    PrototypeHandle* const* struct_prototypes,
    size_t pair_count);

StructDefHandle* metal_struct_def_new(
    NameHandle* name,
    KindHandle* struct_kind,
    RegionIdHandle* region_id,
    uint32_t mutability,
    EdgeHandle* const* edges, size_t edge_count,
    StructMemberHandle* const* members, size_t member_count,
    uint32_t weakability);

InterfaceDefHandle* metal_interface_def_new(
    NameHandle* name,
    KindHandle* interface_kind,
    RegionIdHandle* region_id,
    uint32_t mutability,
    NameHandle* const* super_interfaces, size_t super_count,
    InterfaceMethodHandle* const* methods, size_t method_count,
    uint32_t weakability);

FunctionHandle* metal_function_new(
    PrototypeHandle* prototype, ExpressionHandle* body, SourceLocationHandle* loc);


ExpressionHandle* metal_expr_constant_void(SourceLocationHandle* loc);
ExpressionHandle* metal_expr_constant_int(int64_t value, int32_t bits, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_constant_bool(int32_t value , SourceLocationHandle* loc);
ExpressionHandle* metal_expr_constant_f64(double value, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_constant_str(const char* value_ptr, size_t value_len, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_break(SourceLocationHandle* loc);
ExpressionHandle* metal_expr_return(ExpressionHandle* source_expr, KindHandle* source_type, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_discard(ExpressionHandle* expr, KindHandle* source_type, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_block(ExpressionHandle* inner, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_consecutor(ExpressionHandle* const* exprs, size_t expr_count, KindHandle* result, SourceLocationHandle* loc);

ExpressionHandle* metal_expr_argument(int32_t param_index, KindHandle* tyype, SourceLocationHandle* loc);

ExpressionHandle* metal_expr_stackify(LocalHandle* variable, ExpressionHandle* expr, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_let_and_lend(LocalHandle* variable, ExpressionHandle* expr, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_restackify(LocalHandle* variable, ExpressionHandle* source_expr, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_unstackify(LocalHandle* variable, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_local_lookup(LocalHandle* local_variable, KindHandle* result, SourceLocationHandle* loc);

ExpressionHandle* metal_expr_deref(ExpressionHandle* inner, KindHandle* source_type, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_member_lookup(
    ExpressionHandle* struct_expr, KindHandle* struct_type, int32_t member_index, const char* member_name_ptr, size_t member_name_len, KindHandle* member_type, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_static_sized_array_lookup(
    ExpressionHandle* array_expr, KindHandle* array_type, ExpressionHandle* index_expr, KindHandle* index_type, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_runtime_sized_array_lookup(
    ExpressionHandle* array_expr, KindHandle* array_type, ExpressionHandle* index_expr, KindHandle* index_type, KindHandle* result, SourceLocationHandle* loc);

ExpressionHandle* metal_expr_mutate(
    ExpressionHandle* destination_expr, KindHandle* destination_type, ExpressionHandle* source_expr, KindHandle* source_type, KindHandle* result, bool has_alias_scope, const uint32_t* scope_ptr, size_t scope_len, uint32_t group_count, SourceLocationHandle* loc);

ExpressionHandle* metal_expr_new_struct(
    KindHandle* struct_kind, KindHandle* result,
    ExpressionHandle* const* args, size_t arg_count, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_destroy(
    ExpressionHandle* expr, KindHandle* struct_kind,
    LocalHandle* const* destination_locals, size_t local_count, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_copy_prim(ExpressionHandle* inner, KindHandle* source_type, KindHandle* result, bool has_alias_scope, const uint32_t* scope_ptr, size_t scope_len, uint32_t group_count, SourceLocationHandle* loc);

ExpressionHandle* metal_expr_struct_to_interface_upcast(
    ExpressionHandle* inner_expr, KindHandle* source_type, KindHandle* target_interface, NameHandle* impl_name, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_interface_to_interface_upcast(
    ExpressionHandle* inner_expr, KindHandle* target_interface, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_narrow_interface(
    ExpressionHandle* inner_expr, KindHandle* source_type, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_as_subtype(
    ExpressionHandle* source_expr, KindHandle* source_type, KindHandle* target_type,
    PrototypeHandle* ok_constructor, PrototypeHandle* err_constructor,
    NameHandle* impl_name, NameHandle* ok_impl_name, NameHandle* err_impl_name,
    KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_is_same_instance(ExpressionHandle* left, KindHandle* left_type, ExpressionHandle* right, KindHandle* right_type, SourceLocationHandle* loc);

ExpressionHandle* metal_expr_weak_alias(ExpressionHandle* inner_expr, KindHandle* source_type, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_lock_weak(
    ExpressionHandle* inner_expr, KindHandle* source_type,
    PrototypeHandle* some_constructor, PrototypeHandle* none_constructor,
    NameHandle* some_impl_name, NameHandle* none_impl_name,
    KindHandle* result, SourceLocationHandle* loc);

ExpressionHandle* metal_expr_call(
    PrototypeHandle* callable, ExpressionHandle* const* args, size_t arg_count, KindHandle* result, bool has_facts, const uint32_t* touched_ptr, size_t touched_len, uint32_t group_count, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_extern_call(
    PrototypeHandle* prototype, ExpressionHandle* const* args, size_t arg_count, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_interface_call(
    PrototypeHandle* super_function_prototype, int32_t virtual_param_index, int32_t index_in_edge,
    ExpressionHandle* const* args, size_t arg_count, KindHandle* result, SourceLocationHandle* loc);

ExpressionHandle* metal_expr_if(
    ExpressionHandle* condition, ExpressionHandle* then_call, ExpressionHandle* else_call,
    KindHandle* then_result_type, KindHandle* else_result_type, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_while(ExpressionHandle* block, KindHandle* result, SourceLocationHandle* loc);

ExpressionHandle* metal_expr_new_array_from_values(
    ExpressionHandle* const* elements, size_t element_count, KindHandle* result, KindHandle* array_type, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_new_mut_runtime_sized_array(
    KindHandle* array_type, ExpressionHandle* capacity_expr, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_static_array_from_callable(
    KindHandle* array_type, ExpressionHandle* generator, PrototypeHandle* generator_method, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_array_length(ExpressionHandle* array_expr, KindHandle* array_type, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_array_capacity(ExpressionHandle* array_expr, KindHandle* array_type, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_array_size(ExpressionHandle* array, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_push_runtime_sized_array(
    ExpressionHandle* array_expr, KindHandle* array_type, ExpressionHandle* new_element_expr, KindHandle* element_type, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_pop_runtime_sized_array(ExpressionHandle* array_expr, KindHandle* array_type, KindHandle* result, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_destroy_static_sized_array_into_function(
    ExpressionHandle* array_expr, KindHandle* array_type,
    ExpressionHandle* consumer, PrototypeHandle* consumer_method, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_destroy_static_sized_array_into_locals(
    ExpressionHandle* expr, KindHandle* static_sized_array,
    LocalHandle* const* destination_locals, size_t local_count, SourceLocationHandle* loc);
ExpressionHandle* metal_expr_destroy_mut_runtime_sized_array(ExpressionHandle* array_expr, SourceLocationHandle* loc);


PackageBuilderHandle* metal_package_builder_new(
    MetalCacheHandle* cache, PackageCoordHandle* package_coord);

void metal_package_builder_add_interface(
    PackageBuilderHandle*, const char* name_ptr, size_t name_len, InterfaceDefHandle*);
void metal_package_builder_add_struct(
    PackageBuilderHandle*, const char* name_ptr, size_t name_len, StructDefHandle*);
void metal_package_builder_add_function(
    PackageBuilderHandle*, const char* name_ptr, size_t name_len, FunctionHandle*);
void metal_package_builder_add_static_sized_array(
    PackageBuilderHandle*, const char* name_ptr, size_t name_len, StaticSizedArrayDefHandle*);
void metal_package_builder_add_runtime_sized_array(
    PackageBuilderHandle*, const char* name_ptr, size_t name_len, RuntimeSizedArrayDefHandle*);

StaticSizedArrayDefHandle* metal_static_sized_array_def_new(
    NameHandle* name, KindHandle* array_kind, int32_t size,
    RegionIdHandle* region_id,
    KindHandle* element_type);

RuntimeSizedArrayDefHandle* metal_runtime_sized_array_def_new(
    NameHandle* name, KindHandle* array_kind,
    RegionIdHandle* region_id,
    KindHandle* element_type);
void metal_package_builder_add_export_function(
    PackageBuilderHandle*, const char* name_ptr, size_t name_len, PrototypeHandle*);
void metal_package_builder_add_export_kind(
    PackageBuilderHandle*, const char* name_ptr, size_t name_len, KindHandle*);
void metal_package_builder_add_extern_function(
    PackageBuilderHandle*, const char* name_ptr, size_t name_len, PrototypeHandle*);
void metal_package_builder_add_extern_kind(
    PackageBuilderHandle*, const char* name_ptr, size_t name_len, KindHandle*);
void metal_package_builder_add_struct_layout(
    PackageBuilderHandle*, const char* name_ptr, size_t name_len, uint64_t size, uint64_t align);
void metal_package_builder_add_extern_abi(
    PackageBuilderHandle*, const char* symbol_ptr, size_t symbol_len,
    CoercionFFI ret, const CoercionFFI* args, size_t args_len);
void metal_package_builder_add_param_noalias(
    PackageBuilderHandle*, const char* name_ptr, size_t name_len,
    const bool* param_noalias, size_t param_noalias_len);

PackageHandle* metal_package_builder_finish(PackageBuilderHandle*);


ProgramBuilderHandle* metal_program_builder_new(MetalCacheHandle* cache);
void metal_program_builder_add_package(
    ProgramBuilderHandle*, PackageCoordHandle*, PackageHandle*);
ProgramHandle* metal_program_builder_finish(ProgramBuilderHandle*);

void metal_program_free(ProgramHandle*);

#ifdef __cplusplus
}
#endif

#endif
