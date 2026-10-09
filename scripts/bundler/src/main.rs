use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

type R<T> = Result<T, Box<dyn Error>>;

const BUNDLE_NAME: &str = "valen-macos-arm64";
const EXCLUDE_PREFIXES: &[&str] = &["librustc-beta_rt."];

struct Args {
    out: PathBuf,
    toolchain: String,
    rustc: Option<PathBuf>,
    cargo: Option<PathBuf>,
    sysroot: Option<PathBuf>,
    skip_build: bool,
    tar: bool,
}

fn parse_args() -> R<Args> {
    let mut out = None;
    let mut toolchain = "rustc-fork".to_string();
    let mut rustc = None;
    let mut cargo = None;
    let mut sysroot = None;
    let mut skip_build = false;
    let mut tar = false;

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => out = Some(PathBuf::from(req(&mut it, "--out")?)),
            "--toolchain" => toolchain = req(&mut it, "--toolchain")?,
            "--rustc" => rustc = Some(PathBuf::from(req(&mut it, "--rustc")?)),
            "--cargo" => cargo = Some(PathBuf::from(req(&mut it, "--cargo")?)),
            "--sysroot" => sysroot = Some(PathBuf::from(req(&mut it, "--sysroot")?)),
            "--skip-build" => skip_build = true,
            "--tar" => tar = true,
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other} (try --help)").into()),
        }
    }

    Ok(Args {
        out: out.ok_or("--out <dir> is required")?,
        toolchain,
        rustc,
        cargo,
        sysroot,
        skip_build,
        tar,
    })
}

fn req(it: &mut impl Iterator<Item = String>, flag: &str) -> R<String> {
    it.next().ok_or_else(|| format!("{flag} needs a value").into())
}

fn print_help() {
    eprintln!(
        "valen-bundler — assemble a self-contained Valen toolchain (macOS arm64)\n\n\
         USAGE: valen-bundler --out <dir> [options]\n\n\
         OPTIONS:\n\
         \x20 --out <dir>        output directory (bundle written to <dir>/{BUNDLE_NAME})\n\
         \x20 --toolchain <name> fork rustup toolchain name (default: rustc-fork)\n\
         \x20 --rustc <path>     fork rustc (default: `rustup which --toolchain <name> rustc`)\n\
         \x20 --cargo <path>     cargo to ship (default: resolved from rustup)\n\
         \x20 --sysroot <path>   fork sysroot (default: `<rustc> --print sysroot`)\n\
         \x20 --skip-build       do not rebuild; use existing target/release binaries\n\
         \x20 --tar              also produce <out>/{BUNDLE_NAME}.tar.gz\n"
    );
}

fn main() {
    if let Err(e) = run() {
        eprintln!("\nerror: {e}");
        std::process::exit(1);
    }
}

fn run() -> R<()> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let args = parse_args()?;

    let rustc = match &args.rustc {
        Some(p) => p.clone(),
        None => PathBuf::from(capture(
            "rustup",
            &["which", "--toolchain", &args.toolchain, "rustc"],
            None,
        )?),
    };
    step(&format!("fork rustc:     {}", rustc.display()));

    let sysroot = match &args.sysroot {
        Some(p) => p.clone(),
        None => PathBuf::from(capture(rustc.to_str().unwrap(), &["--print", "sysroot"], None)?),
    };
    let target_libdir =
        PathBuf::from(capture(rustc.to_str().unwrap(), &["--print", "target-libdir"], None)?);
    step(&format!("sysroot:        {}", sysroot.display()));
    step(&format!("target-libdir:  {}", target_libdir.display()));

    let triple = target_libdir
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "aarch64-apple-darwin".to_string());
    step(&format!("host triple:    {triple}"));

    let cargo = match &args.cargo {
        Some(p) => p.clone(),
        None => resolve_cargo(&args.toolchain)?,
    };
    step(&format!("cargo:          {}", cargo.display()));

    if !args.skip_build {
        step("building valen + valenc-rs (release, --features rust_interop) …");
        status(
            "cargo",
            &[
                &format!("+{}", args.toolchain),
                "build",
                "--release",
                "--features",
                "rust_interop",
                "--bin",
                "valen",
                "--bin",
                "valenc-rs",
            ],
            Some(&repo_root),
        )?;
    } else {
        step("skipping build (--skip-build)");
    }

    let valen_src = repo_root.join("target/release/valen");
    let valenc_rs_src = repo_root.join("target/release/valenc-rs");
    for p in [&valen_src, &valenc_rs_src] {
        if !p.exists() {
            return Err(format!(
                "missing {} — build first (drop --skip-build)",
                p.display()
            )
            .into());
        }
    }

    let bundle = args.out.join(BUNDLE_NAME);
    if bundle.exists() {
        step(&format!("removing existing {}", bundle.display()));
        fs::remove_dir_all(&bundle)?;
    }
    let bin = bundle.join("bin");
    let lib = bundle.join("lib");
    fs::create_dir_all(&bin)?;
    fs::create_dir_all(&lib)?;

    step("copying sysroot lib/ (this is the big one, ~hundreds of MB) …");
    let mut seen: HashMap<u64, PathBuf> = HashMap::new();
    let lib_src = sysroot.join("lib");
    copy_tree(&lib_src, &lib, &lib_src, &mut seen)?;

    step("copying bin/ (rustc, cargo, valen, valenc-rs) …");
    copy_file(&sysroot.join("bin/rustc"), &bin.join("rustc"))?;
    copy_file(&cargo, &bin.join("cargo"))?;
    let valen = bin.join("valen");
    let valenc_rs = bin.join("valenc-rs");
    copy_file(&valen_src, &valen)?;
    copy_file(&valenc_rs_src, &valenc_rs)?;
    fs::write(bin.join("valen-bundle.marker"), format!("{BUNDLE_NAME}\n"))?;

    step("adding @loader_path rpaths + ad-hoc codesigning valen & valenc-rs …");
    let rel_rpaths = [
        "@loader_path/../lib".to_string(),
        format!("@loader_path/../lib/rustlib/{triple}/lib"),
    ];
    for exe in [&valen, &valenc_rs] {
        for rp in &rel_rpaths {
            add_rpath(exe, rp)?;
        }
        for abs in absolute_rpaths(exe)? {
            delete_rpath(exe, &abs)?;
        }
        codesign(exe)?;
    }

    if args.tar {
        step("creating tarball …");
        status(
            "tar",
            &["czf", &format!("{BUNDLE_NAME}.tar.gz"), "-C", args.out.to_str().unwrap(), BUNDLE_NAME],
            Some(&args.out),
        )?;
    }

    let size = capture("du", &["-sh", bundle.to_str().unwrap()], None).unwrap_or_default();
    println!("\n✓ bundle ready: {}", bundle.display());
    if !size.is_empty() {
        println!("  size: {}", size.split_whitespace().next().unwrap_or("?"));
    }
    println!(
        "  verify: env PATH=/usr/bin:/bin {}/bin/valen build --manifest-path <project>/Valen.toml",
        bundle.display()
    );
    Ok(())
}

fn resolve_cargo(toolchain: &str) -> R<PathBuf> {
    let attempts: &[(&str, Vec<&str>)] = &[
        ("rustup", vec!["which", "--toolchain", toolchain, "cargo"]),
        ("rustup", vec!["which", "cargo"]),
        ("which", vec!["cargo"]),
    ];
    for (cmd, a) in attempts {
        if let Ok(p) = capture(cmd, a, None) {
            if !p.is_empty() && Path::new(&p).exists() {
                return Ok(PathBuf::from(p));
            }
        }
    }
    Err("could not resolve a cargo binary (pass --cargo <path>)".into())
}

fn excluded(name: &OsStr) -> bool {
    let n = name.to_string_lossy();
    EXCLUDE_PREFIXES.iter().any(|p| n.starts_with(p))
}

fn copy_tree(src: &Path, dst: &Path, root: &Path, seen: &mut HashMap<u64, PathBuf>) -> R<()> {
    let mut created = false;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if excluded(&name) || is_skipped_component(src, &name) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        let md = fs::symlink_metadata(&from)?;
        let ft = md.file_type();
        if ft.is_symlink() {
            let target = fs::read_link(&from)?;
            let abs_target = if target.is_absolute() {
                target.clone()
            } else {
                from.parent().unwrap().join(&target)
            };
            if !normalize_abs(&abs_target).starts_with(root) {
                step(&format!(
                    "  dropping escaping symlink {} -> {}",
                    from.display(),
                    target.display()
                ));
                continue;
            }
            ensure_dir(dst, &mut created)?;
            let _ = fs::remove_file(&to);
            symlink(&target, &to)?;
        } else if ft.is_dir() {
            copy_tree(&from, &to, root, seen)?;
        } else {
            ensure_dir(dst, &mut created)?;
            let ino = md.ino();
            if let Some(existing) = seen.get(&ino) {
                let _ = fs::remove_file(&to);
                fs::hard_link(existing, &to)?;
            } else {
                fs::copy(&from, &to)?;
                seen.insert(ino, to.clone());
            }
        }
    }
    Ok(())
}

fn ensure_dir(dir: &Path, created: &mut bool) -> R<()> {
    if !*created {
        fs::create_dir_all(dir)?;
        *created = true;
    }
    Ok(())
}

fn is_skipped_component(parent: &Path, name: &OsStr) -> bool {
    parent.file_name() == Some(OsStr::new("rustlib"))
        && matches!(name.to_str(), Some("src") | Some("rustc-src"))
}

fn normalize_abs(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn copy_file(from: &Path, to: &Path) -> R<()> {
    fs::copy(from, to)
        .map_err(|e| format!("copy {} -> {}: {e}", from.display(), to.display()))?;
    Ok(())
}

fn add_rpath(exe: &Path, rpath: &str) -> R<()> {
    let out = Command::new("install_name_tool")
        .args(["-add_rpath", rpath])
        .arg(exe)
        .output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("would duplicate path") || err.contains("already") {
            return Ok(());
        }
        return Err(format!("install_name_tool {} {}: {}", rpath, exe.display(), err.trim()).into());
    }
    Ok(())
}

fn absolute_rpaths(exe: &Path) -> R<Vec<String>> {
    let out = capture("otool", &["-l", exe.to_str().unwrap()], None)?;
    let mut res = Vec::new();
    for line in out.lines() {
        if let Some(rest) = line.trim().strip_prefix("path ") {
            if let Some(idx) = rest.rfind(" (offset") {
                let p = &rest[..idx];
                if !p.starts_with('@') {
                    res.push(p.to_string());
                }
            }
        }
    }
    Ok(res)
}

fn delete_rpath(exe: &Path, rpath: &str) -> R<()> {
    status("install_name_tool", &["-delete_rpath", rpath, exe.to_str().unwrap()], None)
}

fn codesign(exe: &Path) -> R<()> {
    status("codesign", &["--force", "--sign", "-", exe.to_str().unwrap()], None)
}

fn status(cmd: &str, args: &[&str], cwd: Option<&Path>) -> R<()> {
    let mut c = Command::new(cmd);
    c.args(args);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    let st = c
        .status()
        .map_err(|e| format!("spawn {cmd}: {e}"))?;
    if !st.success() {
        return Err(format!("{cmd} {:?} failed ({st})", args).into());
    }
    Ok(())
}

fn capture(cmd: &str, args: &[&str], cwd: Option<&Path>) -> R<String> {
    let mut c = Command::new(cmd);
    c.args(args);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    let out = c.output().map_err(|e| format!("spawn {cmd}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{cmd} {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn step(msg: &str) {
    eprintln!("• {msg}");
}
