fn main() {
    for (key, command, args) in [
        ("GIT_HASH", "git", vec!["rev-parse", "--short", "HEAD"]),
        ("RUSTC_VERSION", "rustc", vec!["--version"]),
    ] {
        let value = std::process::Command::new(command)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_else(|| "unknown".into());
        println!("cargo:rustc-env={key}={value}");
    }
    println!("cargo:rerun-if-changed=.git/HEAD");
}
