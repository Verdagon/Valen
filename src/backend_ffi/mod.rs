pub mod backend_inputs;
pub mod metal_cache;
pub mod metal_lowerer;

use std::ffi::{c_void, CString};
use std::os::raw::c_char;
use std::ptr;

use self::backend_inputs::{
    BackendInputs, BackendInputsFFIRaw, BackendMode, CallbackFFIRaw, InteropInputsFFIRaw,
    SourceFilePathFFIRaw, BACKEND_MODE_INTEROP, BACKEND_MODE_STANDALONE,
};

pub const BACKEND_OPT_LEVEL_O0: i32 = 0;
pub const BACKEND_OPT_LEVEL_O1: i32 = 1;
pub const BACKEND_OPT_LEVEL_O2: i32 = 2;
pub const BACKEND_OPT_LEVEL_O2I: i32 = 3;
pub const BACKEND_OPT_LEVEL_O3: i32 = 4;

#[repr(C)]
pub(crate) struct BackendCompileOptionsFFIRaw {
    output_dir: *const c_char,
    triple: *const c_char,
    cpu: *const c_char,
    opt_level: i32,
    pic: u8,
    verify: u8,
    print_asm: u8,
    print_llvmir: u8,
    census: u8,
    flares: u8,
    include_bounds_checks: u8,
    use_atomic_rc: u8,
    print_mem_overhead: u8,
    debug: u8,
    suppress_alias_metadata: u8,
}

extern "C" {
    fn backend_compile(inputs: *const BackendInputsFFIRaw) -> i32;
}

pub struct BackendCompileOptions {
    pub output_dir: String,
    pub triple: String,
    pub cpu: String,
    pub opt_level: i32, // VCOORD: let's get rid of this soon
    pub pic: bool,
    pub verify: bool,
    pub print_asm: bool,
    pub print_llvmir: bool,
    pub census: bool,
    pub flares: bool,
    pub include_bounds_checks: bool,
    pub use_atomic_rc: bool,
    pub print_mem_overhead: bool,
    pub debug: bool,
    pub suppress_alias_metadata: bool,
}

impl Default for BackendCompileOptions {
    fn default() -> Self {
        Self {
            output_dir: String::new(),
            triple: String::new(),
            cpu: String::new(),
            opt_level: BACKEND_OPT_LEVEL_O2I,
            pic: false,
            verify: false,
            print_asm: false,
            print_llvmir: false,
            census: false,
            flares: false,
            include_bounds_checks: true,
            use_atomic_rc: false,
            print_mem_overhead: false,
            debug: false,
            suppress_alias_metadata: false,
        }
    }
}

pub fn compile(inputs: BackendInputs) -> i32 {
    let opts = &inputs.options;
    let output_dir_c = CString::new(opts.output_dir.as_str()).expect("output_dir contains NUL");
    let triple_c = CString::new(opts.triple.as_str()).expect("triple contains NUL");
    let cpu_c = CString::new(opts.cpu.as_str()).expect("cpu contains NUL");

    let options = BackendCompileOptionsFFIRaw {
        output_dir: output_dir_c.as_ptr(),
        triple: triple_c.as_ptr(),
        cpu: cpu_c.as_ptr(),
        opt_level: opts.opt_level,
        pic: opts.pic as u8,
        verify: opts.verify as u8,
        print_asm: opts.print_asm as u8,
        print_llvmir: opts.print_llvmir as u8,
        census: opts.census as u8,
        flares: opts.flares as u8,
        include_bounds_checks: opts.include_bounds_checks as u8,
        use_atomic_rc: opts.use_atomic_rc as u8,
        print_mem_overhead: opts.print_mem_overhead as u8,
        debug: opts.debug as u8,
        suppress_alias_metadata: opts.suppress_alias_metadata as u8,
    };

    let (mode, context, module, entry_symbol_c) = match &inputs.mode {
        BackendMode::Standalone(_) => (
            BACKEND_MODE_STANDALONE,
            ptr::null_mut(),
            ptr::null_mut(),
            CString::new("").expect("empty string is NUL-free"),
        ),
        BackendMode::Interop(interop) => (
            BACKEND_MODE_INTEROP,
            interop.context,
            interop.module,
            CString::new(interop.entry_symbol.unwrap_or("")).expect("entry_symbol contains NUL"),
        ),
    };

    let callback_cstrings: Vec<(CString, CString)> = match &inputs.mode {
        BackendMode::Interop(interop) => interop
            .callbacks
            .iter()
            .map(|c| {
                (
                    CString::new(c.symbol).expect("callback symbol contains NUL"),
                    CString::new(c.vale_name).expect("callback vale_name contains NUL"),
                )
            })
            .collect(),
        BackendMode::Standalone(_) => Vec::new(),
    };
    let callbacks_raw: Vec<CallbackFFIRaw> = callback_cstrings
        .iter()
        .map(|(symbol, vale_name)| CallbackFFIRaw {
            symbol: symbol.as_ptr(),
            vale_name: vale_name.as_ptr(),
        })
        .collect();

    let source_path_cstrings: Vec<(CString, CString)> = inputs
        .absolute_source_paths
        .iter()
        .map(|sp| {
            (
                CString::new(sp.basename.as_str()).expect("source basename contains NUL"),
                CString::new(sp.abspath.as_str()).expect("source abspath contains NUL"),
            )
        })
        .collect();
    let source_paths_raw: Vec<SourceFilePathFFIRaw> = source_path_cstrings
        .iter()
        .map(|(basename, abspath)| SourceFilePathFFIRaw {
            basename: basename.as_ptr(),
            abspath: abspath.as_ptr(),
        })
        .collect();

    let raw = BackendInputsFFIRaw {
        cache: inputs.cache.raw() as *mut c_void,
        program: inputs.program.raw(),
        options,
        mode,
        interop: InteropInputsFFIRaw {
            context,
            module,
            entry_symbol: entry_symbol_c.as_ptr(),
            callbacks: callbacks_raw.as_ptr(),
            num_callbacks: callbacks_raw.len(),
        },
        source_paths: if source_paths_raw.is_empty() {
            ptr::null()
        } else {
            source_paths_raw.as_ptr()
        },
        num_source_paths: source_paths_raw.len(),
    };
    unsafe { backend_compile(&raw) }
}
