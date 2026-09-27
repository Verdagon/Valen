use std::env;
use std::process::Command;

pub fn get_env_rustc_and_sysroot_locations() -> (String, String) {
  let rustc_location =
      std::env::var("COMPANION_RUSTC")
          .expect("Set COMPANION_RUSTC env var, for tests to know path to rustc");
  let sysroot_output = Command::new(rustc_location.clone())
      .arg("--print=sysroot")
      .output()
      .expect("could not run rustc to find the sysroot");
  let sysroot_location = String::from_utf8(sysroot_output.stdout).expect("sysroot was not utf8").trim().to_string();
  return (rustc_location, sysroot_location);
}

pub fn get_env_borrow_check() -> Result<bool, String> {
  match env::var("VALEN_BORROW_CHECK") {
    Err(e) => Err(format!("Couldn't look up VALEN_BORROW_CHECK: {e:?}")),
    Ok(value) => {
      match value.as_str() {
        "" => Ok(true),
        "on" => Ok(true),
        "off" => Ok(false),
        other => Err(format!("VALEN_BORROW_CHECK must be \"on\" or \"off\", was: {other:?}"))
      }
    }
  }
}
