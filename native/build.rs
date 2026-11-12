fn main() {
    println!("cargo:rerun-if-changed=../.git/HEAD");
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        && output.status.success()
    {
        println!(
            "cargo:rustc-env=XEN_BUILD_COMMIT={}",
            String::from_utf8_lossy(&output.stdout).trim()
        );
    }
}
