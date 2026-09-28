
use std::ffi::c_void;
use std::os::raw::c_char;

use super::metal_cache::{MetalCache, Program};
use super::{BackendCompileOptions, BackendCompileOptionsFFIRaw};

pub(crate) const BACKEND_MODE_STANDALONE: i32 = 0;
pub(crate) const BACKEND_MODE_INTEROP: i32 = 1;

#[repr(C)]
pub(crate) struct CallbackFFIRaw {
    pub(crate) symbol: *const c_char,
    pub(crate) vale_name: *const c_char,
}

#[repr(C)]
pub(crate) struct SourceFilePathFFIRaw {
    pub(crate) basename: *const c_char,
    pub(crate) abspath: *const c_char,
}

#[repr(C)]
pub(crate) struct InteropInputsFFIRaw {
    pub(crate) context: *mut c_void,
    pub(crate) module: *mut c_void,
    pub(crate) entry_symbol: *const c_char,
    pub(crate) callbacks: *const CallbackFFIRaw,
    pub(crate) num_callbacks: usize,
}

#[repr(C)]
pub(crate) struct BackendInputsFFIRaw {
    pub(crate) cache: *mut c_void,
    pub(crate) program: *mut c_void,
    pub(crate) options: BackendCompileOptionsFFIRaw,
    pub(crate) mode: i32,
    pub(crate) interop: InteropInputsFFIRaw,
    pub(crate) source_paths: *const SourceFilePathFFIRaw,
    pub(crate) num_source_paths: usize,
}

pub struct BackendInputs<'a, 'c> {
    pub cache: &'a MetalCache,
    pub program: &'a Program<'c>,
    pub options: BackendCompileOptions,
    pub mode: BackendMode<'a>,
    pub absolute_source_paths: Vec<SourceFilePath>,
}

pub struct SourceFilePath {
    pub basename: String,
    pub abspath: String,
}

pub enum BackendMode<'a> {
    Standalone(StandaloneInputs),
    Interop(InteropInputs<'a>),
}

pub struct StandaloneInputs {}

pub struct Callback<'a> {
    pub symbol: &'a str,
    pub vale_name: &'a str,
}

pub struct InteropInputs<'a> {
    pub context: *mut c_void,
    pub module: *mut c_void,
    pub entry_symbol: Option<&'a str>,
    pub callbacks: Vec<Callback<'a>>,
}
