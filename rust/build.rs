use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=resources.rc");
    println!("cargo:rerun-if-changed=assets/SlimBrowser.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let kits = PathBuf::from(
        env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".into()),
    )
    .join("Windows Kits/10/bin");
    let mut versions = std::fs::read_dir(kits)
        .expect("Windows SDK is required")
        .filter_map(|d| d.ok())
        .map(|d| d.path())
        .filter(|p| p.join("x64/rc.exe").exists())
        .collect::<Vec<_>>();
    versions.sort();
    let rc = versions
        .last()
        .expect("Windows SDK resource compiler was not found")
        .join("x64/rc.exe");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("slimbrowser.res");
    let status = Command::new(rc)
        .args(["/nologo", "/c65001"])
        .arg(format!("/fo{}", output.display()))
        .arg("resources.rc")
        .status()
        .expect("Could not run rc.exe");
    assert!(status.success(), "Resource compiler failed");
    println!("cargo:rustc-link-arg={}", output.display());
}
