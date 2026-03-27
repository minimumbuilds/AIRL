fn main() {
    // Embed build timestamp so --version can report when the binary was compiled.
    let now = std::process::Command::new("date")
        .arg("+%Y-%m-%d %H:%M:%S UTC")
        .env("TZ", "UTC")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=AIRL_BUILD_TIME={}", now);
}
