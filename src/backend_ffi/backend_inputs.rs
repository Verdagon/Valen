
use std::ffi::c_void;
use std::os::raw::c_char;

use super::metal_cache::{MetalCache, Program};
use super::{BackendCompileOptions, BackendCompileOptionsFFIRaw};

pub(crate) const BACKEND_MODE_STANDALONE: i32 = 0;

#[repr(C)]
pub(crate) struct SourceFilePathFFIRaw {
    pub(crate) basename: *const c_char,
    pub(crate) abspath: *const c_char,
}

#[repr(C)]
pub(crate) struct BackendInputsFFIRaw {
    pub(crate) cache: *mut c_void,
    pub(crate) program: *mut c_void,
    pub(crate) options: BackendCompileOptionsFFIRaw,
    pub(crate) mode: i32,
    pub(crate) source_paths: *const SourceFilePathFFIRaw,
    pub(crate) num_source_paths: usize,
}

pub struct BackendInputs<'a, 'c> {
    pub cache: &'a MetalCache,
    pub program: &'a Program<'c>,
    pub options: BackendCompileOptions,
    pub mode: BackendMode,
    pub absolute_source_paths: Vec<SourceFilePath>,
}

pub struct SourceFilePath {
    pub basename: String,
    pub abspath: String,
}

pub enum BackendMode {
    Standalone(StandaloneInputs),
}

pub struct StandaloneInputs {}
