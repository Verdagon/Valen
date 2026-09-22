
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;
use crate::backend_ffi::BACKEND_OPT_LEVEL_O0;

pub mod tests;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Backend {
    Native,
    Wasi,
}

pub fn target_backend() -> Backend {
    match std::env::var("VALE_TEST_BACKEND").as_deref() {
        Ok("wasi") => Backend::Wasi,
        _ => Backend::Native,
    }
}

impl Backend {
    pub fn exe_name(self) -> &'static str {
        match self {
            Backend::Native => "a.out",
            Backend::Wasi => "a.out.wasm",
        }
    }

    pub fn target_triple(self) -> Option<&'static str> {
        match self {
            Backend::Native => None,
            Backend::Wasi => Some("wasm32-wasi"),
        }
    }

    pub fn sysroot(self) -> Option<PathBuf> {
        match self {
            Backend::Native => None,
            Backend::Wasi => Some(wasi_sdk_path().join("share/wasi-sysroot")),
        }
    }

    pub fn clang_path(self) -> Option<String> {
        match self {
            Backend::Native => None,
            Backend::Wasi => Some(
                wasi_sdk_path()
                    .join("bin/clang")
                    .display()
                    .to_string(),
            ),
        }
    }
}

fn wasi_sdk_path() -> PathBuf {
    if let Ok(p) = std::env::var("WASI_SDK_PATH") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").expect("HOME unset");
    PathBuf::from(home).join("wasi-sdk")
}

#[macro_export]
macro_rules! wasi_skip {
    ($reason:expr) => {
        if $crate::end_to_end_tests::target_backend()
            == $crate::end_to_end_tests::Backend::Wasi
        {
            eprintln!("wasi_skip: {}", $reason);
            return;
        }
    };
}

pub enum KeepDir {
    Temp(tempfile::TempDir),
    Kept(std::path::PathBuf),
}
impl KeepDir {
    pub fn path(&self) -> &std::path::Path {
        match self {
            KeepDir::Temp(t) => t.path(),
            KeepDir::Kept(p) => p.as_path(),
        }
    }
}

pub struct CompiledProgram {
    exe: PathBuf,
    pub cwd: PathBuf,
    _work: KeepDir,
    _extra_keepalive: Vec<tempfile::TempDir>,
    backend: Backend,
}

pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn compile_program(
    primary_vale: &Path,
    extra_c: &[&Path],
    configure_backend: impl FnOnce(&mut crate::backend_ffi::BackendCompileOptions),
) -> CompiledProgram {
    compile_inputs(
        vec![primary_vale.to_path_buf()],
        extra_c,
        configure_backend,
        Vec::new(),
        true,
    )
}

pub fn compile_program_without_borrow_check(
    primary_vale: &Path,
    extra_c: &[&Path],
    configure_backend: impl FnOnce(&mut crate::backend_ffi::BackendCompileOptions),
) -> CompiledProgram {
    compile_inputs(
        vec![primary_vale.to_path_buf()],
        extra_c,
        configure_backend,
        Vec::new(),
        false,
    )
}

pub fn compile_inline(
    code: &str,
    configure_backend: impl FnOnce(&mut crate::backend_ffi::BackendCompileOptions),
) -> CompiledProgram {
    let src_dir = tempfile::tempdir().unwrap();
    let src_file = src_dir.path().join("test.vale");
    std::fs::write(&src_file, code).unwrap();
    compile_inputs(
        vec![src_dir.path().to_path_buf()],
        &[],
        configure_backend,
        vec![src_dir],
        true,
    )
}

pub fn compile_inline_without_borrow_check(
    code: &str,
    configure_backend: impl FnOnce(&mut crate::backend_ffi::BackendCompileOptions),
) -> CompiledProgram {
    let src_dir = tempfile::tempdir().unwrap();
    let src_file = src_dir.path().join("test.vale");
    std::fs::write(&src_file, code).unwrap();
    compile_inputs(
        vec![src_dir.path().to_path_buf()],
        &[],
        configure_backend,
        vec![src_dir],
        false,
    )
}

pub fn compile_inline_debug(code: &str) -> (CompiledProgram, PathBuf) {
    let src_dir = tempfile::tempdir().unwrap();
    let src_file = src_dir.path().join("test.vale");
    fs::write(&src_file, code).unwrap();
    let src_file_path = src_file.clone();
    let cp = compile_inputs(
        vec![src_dir.path().to_path_buf()],
        &[],
        |opts| {
            opts.debug = true;
            opts.opt_level = BACKEND_OPT_LEVEL_O0;
        },
        vec![src_dir],
        true,
    );
    (cp, src_file_path)
}

pub fn compile_inline_debug_without_borrow_check(code: &str) -> (CompiledProgram, PathBuf) {
    let src_dir = tempfile::tempdir().unwrap();
    let src_file = src_dir.path().join("test.vale");
    fs::write(&src_file, code).unwrap();
    let src_file_path = src_file.clone();
    let cp = compile_inputs(
        vec![src_dir.path().to_path_buf()],
        &[],
        |opts| {
            opts.debug = true;
            opts.opt_level = BACKEND_OPT_LEVEL_O0;
        },
        vec![src_dir],
        false,
    );
    (cp, src_file_path)
}

fn compile_inputs(
    vale_inputs: Vec<PathBuf>,
    extra_c: &[&Path],
    configure_backend: impl FnOnce(&mut crate::backend_ffi::BackendCompileOptions),
    keepalive: Vec<tempfile::TempDir>,
    borrow_check: bool,
) -> CompiledProgram {
    let parse_bump = bumpalo::Bump::new();
    let parse_arena = crate::parse_arena::ParseArena::new(&parse_bump);
    let keywords = crate::keywords::Keywords::new_for_parse(&parse_arena);

    let work = if let Ok(name) = std::env::var("NEXTEST_TEST_NAME") {
        let safe = name.replace("::", "-");
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tmp/vale-test-runs")
            .join(safe);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        println!("Test outputs: {}", root.display());
        KeepDir::Kept(root)
    } else {
        KeepDir::Temp(tempfile::tempdir().unwrap())
    };
    let out_dir = work.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut cli_args: Vec<String> = vec![
        "build".to_string(),
        "--output_dir".to_string(),
        out_dir.display().to_string(),
        "--output_vast".to_string(),
        "true".to_string(),
        "--include_builtins".to_string(),
        "true".to_string(),
        "--sanity_check".to_string(),
        "false".to_string(),
        "--borrow_check".to_string(),
        borrow_check.to_string(),
    ];
    for input in &vale_inputs {
        cli_args.push(format!("vtest={}", input.display()));
    }
    let opts = crate::pass_manager::pass_manager::parse_opts(
        &parse_arena,
        crate::pass_manager::pass_manager::Options {
            inputs: vec![],
            output_dir_path: None,
            benchmark: false,
            output_vast: true,
            include_builtins: true,
            mode: None,
            sanity_check: false,
            use_optimized_solver: true,
            use_overload_index: true,
            verbose_errors: false,
            debug_output: false,
            borrow_check: true,
        },
        cli_args,
    );

    let mut backend_opts = crate::backend_ffi::BackendCompileOptions::default();
    backend_opts.output_dir = out_dir.display().to_string();
    if std::env::var("VALE_FLARES").is_ok() {
        backend_opts.flares = true;
    }
    if std::env::var("VALE_LLVM_IR").is_ok() {
        backend_opts.print_llvmir = true;
    }

    if std::env::var("VALE_TEST_CENSUS").is_ok() {
        backend_opts.census = true;
    }
    backend_opts.verify = true;
    configure_backend(&mut backend_opts);
    let link_with_debug = backend_opts.debug;

    let builtins_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("Backend/builtins");
    let test_builtins = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("Backend/test_builtins/testbuiltins.c");

    let mut extra_inputs: Vec<PathBuf> = vec![test_builtins];
    for c in extra_c {
        extra_inputs.push(c.to_path_buf());
    }

    let backend = target_backend();
    let asan_enabled = matches!(backend, Backend::Native)
        && std::env::var("VALE_TEST_ASAN").is_ok();
    let clang_cfg = crate::pass_manager::pass_manager::ClangConfig {
        builtins_dir,
        extra_inputs,
        clang_path: backend.clang_path(),
        libc_path: None,
        executable_name: backend.exe_name().to_string(),
        asan: asan_enabled,
        debug_symbols: link_with_debug,
        pic: false,
        pie: false,
        windows: false,
        target_triple: backend.target_triple().map(str::to_string),
        sysroot: backend.sysroot(),
    };

    let bp = crate::pass_manager::pass_manager::build(
        &parse_arena,
        &keywords,
        &opts,
        backend_opts,
        &clang_cfg,
    )
    .unwrap_or_else(|e| panic!("pass_manager::build failed:\n{}", e));
    assert_eq!(bp.rc, 0, "backend returned {}", bp.rc);

    if link_with_debug && matches!(backend, Backend::Native) {
        let dsym_status = Command::new("dsymutil")
            .arg(&bp.exe_path)
            .status()
            .expect("dsymutil spawn failed");
        assert!(dsym_status.success(), "dsymutil failed");
    }

    CompiledProgram {
        exe: bp.exe_path,
        cwd: out_dir,
        _work: work,
        _extra_keepalive: keepalive,
        backend,
    }
}

impl CompiledProgram {
    pub fn run(&self, args: &[&str]) -> ExecResult {
        let asan_env = std::env::var("VALE_TEST_ASAN").is_ok();
        let out = match self.backend {
            Backend::Native => {
                let mut cmd = std::process::Command::new(&self.exe);
                cmd.current_dir(&self.cwd).args(args);
                if asan_env {
                    cmd.env("ASAN_OPTIONS", "detect_leaks=1:abort_on_error=1:halt_on_error=1");
                    let suppressions = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("lsan-suppressions.txt");
                    cmd.env("LSAN_OPTIONS", format!("suppressions={}", suppressions.display()));
                }
                cmd.output().expect("exec failed")
            },
            Backend::Wasi => {
                let mut cmd = std::process::Command::new("wasmtime");
                cmd.current_dir(&self.cwd)
                    .arg("run")
                    .arg("--dir=.")
                    .arg(&self.exe)
                    .args(args);
                cmd.output().expect("wasmtime exec failed")
            }
        };
        ExecResult {
            exit_code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    pub fn lldb_capture(&self, commands: &[&str]) -> String {
        let mut cmd = Command::new("lldb");
        cmd.arg("-b");
        for c in commands {
            cmd.arg("-o").arg(c);
        }
        cmd.arg(&self.exe);
        let out = cmd.output().expect("lldb spawn failed");
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        )
    }

    pub fn lldb_check(&self, commands: &[&str], expected_substrings: &[&str]) {
        let combined = self.lldb_capture(commands);
        for needle in expected_substrings {
            assert!(
                combined.contains(needle),
                "lldb output missing expected substring {:?}\nlldb commands: {:?}\nfull output:\n{}",
                needle, commands, combined,
            );
        }
    }

    pub fn lldb_check_ordered(&self, commands: &[&str], expected_in_order: &[&str]) {
        let combined = self.lldb_capture(commands);
        let mut cursor = 0usize;
        for needle in expected_in_order {
            match combined[cursor..].find(needle) {
                Some(rel) => cursor += rel + needle.len(),
                None => panic!(
                    "lldb output missing {:?} in order (after offset {})\nlldb commands: {:?}\nfull output:\n{}",
                    needle, cursor, commands, combined,
                ),
            }
        }
    }

    pub fn dwarfdump_capture(&self, args: &[&str]) -> String {
        let on_path = Command::new("llvm-dwarfdump")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        let bin = if on_path {
            "llvm-dwarfdump".to_string()
        } else {
            let fallback = "/opt/homebrew/opt/llvm/bin/llvm-dwarfdump";
            assert!(
                Path::new(fallback).exists(),
                "llvm-dwarfdump not on PATH and {fallback} doesn't exist"
            );
            fallback.to_string()
        };
        let dsym = format!("{}.dSYM", self.exe.display());
        let target = if Path::new(&dsym).exists() {
            dsym
        } else {
            self.exe.display().to_string()
        };
        let out = Command::new(&bin)
            .args(args)
            .arg(&target)
            .output()
            .expect("llvm-dwarfdump spawn failed");
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        )
    }
}

pub fn programs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/tests")
}

pub fn assert_compile_and_run(vale_path: &Path, expected: i32) {
    let cp = compile_program(vale_path, &[], |_| {});
    let r = cp.run(&[]);
    assert_eq!(
        r.exit_code, expected,
        "stdout={:?} stderr={:?}",
        r.stdout, r.stderr
    );
}

pub fn assert_compile_and_run_without_borrow_check(vale_path: &Path, expected: i32) {
    let cp = compile_program_without_borrow_check(vale_path, &[], |_| {});
    let r = cp.run(&[]);
    assert_eq!(
        r.exit_code, expected,
        "stdout={:?} stderr={:?}",
        r.stdout, r.stderr
    );
}

pub fn assert_compile_and_run_with_c(
    vale_dir: &Path,
    extra_c: &[&Path],
    expected: i32,
) {
    let cp = compile_program(vale_dir, extra_c, |_| {});
    let r = cp.run(&[]);
    assert_eq!(
        r.exit_code, expected,
        "stdout={:?} stderr={:?}",
        r.stdout, r.stderr
    );
}

pub fn assert_compile_and_run_with_c_without_borrow_check(
    vale_dir: &Path,
    extra_c: &[&Path],
    expected: i32,
) {
    let cp = compile_program_without_borrow_check(vale_dir, extra_c, |_| {});
    let r = cp.run(&[]);
    assert_eq!(
        r.exit_code, expected,
        "stdout={:?} stderr={:?}",
        r.stdout, r.stderr
    );
}

pub fn assert_inline_compile_and_run(code: &str, expected: i32) {
    let cp = compile_inline(code, |_| {});
    let r = cp.run(&[]);
    assert_eq!(
        r.exit_code, expected,
        "stdout={:?} stderr={:?}",
        r.stdout, r.stderr
    );
}

pub fn assert_inline_compile_and_run_without_borrow_check(code: &str, expected: i32) {
    let cp = compile_inline_without_borrow_check(code, |_| {});
    let r = cp.run(&[]);
    assert_eq!(
        r.exit_code, expected,
        "stdout={:?} stderr={:?}",
        r.stdout, r.stderr
    );
}

pub struct Step<'a> {
    pub cmd: &'a str,
    pub expect: &'a [&'a str],
    pub reject: &'a [&'a str],
}

pub fn cmd(c: &str) -> Step<'_> {
    Step { cmd: c, expect: &[], reject: &[] }
}

pub fn expect<'a>(c: &'a str, present: &'a [&'a str]) -> Step<'a> {
    Step { cmd: c, expect: present, reject: &[] }
}

pub fn reject<'a>(c: &'a str, present: &'a [&'a str], absent: &'a [&'a str]) -> Step<'a> {
    Step { cmd: c, expect: present, reject: absent }
}

fn split_lldb_session<'a>(out: &'a str, cmds: &[&str]) -> Vec<&'a str> {
    let mut echo_start: Vec<Option<usize>> = Vec::with_capacity(cmds.len());
    let mut echo_end: Vec<Option<usize>> = Vec::with_capacity(cmds.len());
    let mut from = 0usize;
    for c in cmds {
        let needle = format!("(lldb) {}", c);
        if let Some(rel) = out[from..].find(&needle) {
            let start = from + rel;
            let end = start + needle.len();
            echo_start.push(Some(start));
            echo_end.push(Some(end));
            from = end;
        } else {
            echo_start.push(None);
            echo_end.push(None);
        }
    }
    let mut segs = Vec::with_capacity(cmds.len());
    for i in 0..cmds.len() {
        match echo_end[i] {
            None => segs.push(""),
            Some(start) => {
                let mut next = out.len();
                for j in (i + 1)..cmds.len() {
                    if let Some(s) = echo_start[j] {
                        next = s;
                        break;
                    }
                }
                segs.push(&out[start..next]);
            }
        }
    }
    segs
}

fn run_dbg_session(cp: &CompiledProgram, steps: &[Step]) {
    let cmds: Vec<&str> = steps.iter().map(|s| s.cmd).collect();
    let out = cp.lldb_capture(&cmds);
    let segs = split_lldb_session(&out, &cmds);
    for (i, step) in steps.iter().enumerate() {
        let seg = segs[i];
        for needle in step.expect {
            assert!(
                seg.contains(needle),
                "lldb step {} `{}`: output missing {:?}\n--- this step's output ---\n{}\n--- full session ---\n{}",
                i, step.cmd, needle, seg, out,
            );
        }
        for needle in step.reject {
            assert!(
                !seg.contains(needle),
                "lldb step {} `{}`: output unexpectedly contains {:?}\n--- this step's output ---\n{}\n--- full session ---\n{}",
                i, step.cmd, needle, seg, out,
            );
        }
    }
}

fn compile_fixture_debug(vale_path: &Path, native: bool, borrow_check: bool) -> CompiledProgram {
    let src_dir = tempfile::tempdir().unwrap();
    let base = vale_path.file_name().expect("fixture path has no file name");
    let dest = src_dir.path().join(base);
    fs::copy(vale_path, &dest)
        .unwrap_or_else(|e| panic!("copying {:?} into temp dir failed: {}", vale_path, e));
    compile_inputs(
        vec![dest],
        &[],
        |opts| {
            if native {
                opts.debug = true;
                opts.opt_level = BACKEND_OPT_LEVEL_O0;
            }
        },
        vec![src_dir],
        borrow_check,
    )
}

pub fn assert_compile_and_run_dbg(vale_path: &Path, expected: i32, steps: &[Step]) {
    let native = matches!(target_backend(), Backend::Native);
    let cp = compile_fixture_debug(vale_path, native, true);
    let r = cp.run(&[]);
    assert_eq!(
        r.exit_code, expected,
        "stdout={:?} stderr={:?}",
        r.stdout, r.stderr
    );
    if native {
        run_dbg_session(&cp, steps);
    } else {
        eprintln!(
            "SKIP: debug gate for {:?} requires the Native backend (lldb/dSYM); \
             ran exit-code check only under wasi.",
            vale_path
        );
    }
}

pub fn assert_compile_and_run_dbg_without_borrow_check(vale_path: &Path, expected: i32, steps: &[Step]) {
    let native = matches!(target_backend(), Backend::Native);
    let cp = compile_fixture_debug(vale_path, native, false);
    let r = cp.run(&[]);
    assert_eq!(
        r.exit_code, expected,
        "stdout={:?} stderr={:?}",
        r.stdout, r.stderr
    );
    if native {
        run_dbg_session(&cp, steps);
    } else {
        eprintln!(
            "SKIP: debug gate for {:?} requires the Native backend (lldb/dSYM); \
             ran exit-code check only under wasi.",
            vale_path
        );
    }
}

pub fn assert_inline_compile_and_run_dbg(code: &str, expected: i32, steps: &[Step]) {
    let native = matches!(target_backend(), Backend::Native);
    let cp = compile_inline(code, |opts| {
        if native {
            opts.debug = true;
            opts.opt_level = BACKEND_OPT_LEVEL_O0;
        }
    });
    let r = cp.run(&[]);
    assert_eq!(
        r.exit_code, expected,
        "stdout={:?} stderr={:?}",
        r.stdout, r.stderr
    );
    if native {
        run_dbg_session(&cp, steps);
    } else {
        eprintln!(
            "SKIP: inline debug gate requires the Native backend (lldb/dSYM); \
             ran exit-code check only under wasi."
        );
    }
}

pub fn assert_inline_compile_and_run_dbg_without_borrow_check(code: &str, expected: i32, steps: &[Step]) {
    let native = matches!(target_backend(), Backend::Native);
    let src_dir = tempfile::tempdir().unwrap();
    let src_file = src_dir.path().join("test.vale");
    std::fs::write(&src_file, code).unwrap();
    let cp = compile_inputs(
        vec![src_dir.path().to_path_buf()],
        &[],
        |opts| {
            if native {
                opts.debug = true;
                opts.opt_level = BACKEND_OPT_LEVEL_O0;
            }
        },
        vec![src_dir],
        false,
    );
    let r = cp.run(&[]);
    assert_eq!(
        r.exit_code, expected,
        "stdout={:?} stderr={:?}",
        r.stdout, r.stderr
    );
    if native {
        run_dbg_session(&cp, steps);
    } else {
        eprintln!(
            "SKIP: inline debug gate requires the Native backend (lldb/dSYM); \
             ran exit-code check only under wasi."
        );
    }
}
