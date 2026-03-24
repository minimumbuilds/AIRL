fn main() {
    // Check for LLVM 19+ required by melior.
    // If MLIR_SYS_190_PREFIX or LLVM_SYS_190_PREFIX is set, melior will use
    // that installation directly — no further checks needed.
    let have_prefix = std::env::var("MLIR_SYS_190_PREFIX").is_ok()
        || std::env::var("LLVM_SYS_190_PREFIX").is_ok();

    if !have_prefix {
        // Try llvm-config to check the installed version.
        let output = std::process::Command::new("llvm-config-19")
            .arg("--version")
            .output()
            .or_else(|_| {
                std::process::Command::new("llvm-config")
                    .arg("--version")
                    .output()
            });

        match output {
            Ok(out) if out.status.success() => {
                let version = String::from_utf8_lossy(&out.stdout);
                let major: u32 = version
                    .trim()
                    .split('.')
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);

                if major < 19 {
                    eprintln!();
                    eprintln!("╔══════════════════════════════════════════════════════╗");
                    eprintln!("║  airl-mlir requires LLVM 19+                        ║");
                    eprintln!("║  Found: LLVM {}                                  ║", version.trim());
                    eprintln!("║                                                      ║");
                    eprintln!("║  Ubuntu/Debian:                                      ║");
                    eprintln!("║    apt install llvm-19-dev libmlir-19-dev            ║");
                    eprintln!("║    apt install mlir-19-tools libzstd-dev             ║");
                    eprintln!("║                                                      ║");
                    eprintln!("║  Or point to an existing LLVM 19 install:            ║");
                    eprintln!("║    export MLIR_SYS_190_PREFIX=/path/to/llvm-19       ║");
                    eprintln!("║                                                      ║");
                    eprintln!("║  Or use Docker for a fully reproducible build:       ║");
                    eprintln!("║    docker build -t airl . && docker run airl         ║");
                    eprintln!("╚══════════════════════════════════════════════════════╝");
                    eprintln!();
                }
            }
            _ => {
                eprintln!();
                eprintln!("╔══════════════════════════════════════════════════════╗");
                eprintln!("║  airl-mlir requires LLVM 19+ (not found in PATH)    ║");
                eprintln!("║                                                      ║");
                eprintln!("║  Ubuntu/Debian:                                      ║");
                eprintln!("║    apt install llvm-19-dev libmlir-19-dev            ║");
                eprintln!("║    apt install mlir-19-tools libzstd-dev             ║");
                eprintln!("║                                                      ║");
                eprintln!("║  Or point to an existing LLVM 19 install:            ║");
                eprintln!("║    export MLIR_SYS_190_PREFIX=/path/to/llvm-19       ║");
                eprintln!("║                                                      ║");
                eprintln!("║  Or use Docker for a fully reproducible build:       ║");
                eprintln!("║    docker build -t airl . && docker run airl         ║");
                eprintln!("╚══════════════════════════════════════════════════════╝");
                eprintln!();
            }
        }
    }

    // Remind cargo to re-run this script if the env vars change.
    println!("cargo:rerun-if-env-changed=MLIR_SYS_190_PREFIX");
    println!("cargo:rerun-if-env-changed=LLVM_SYS_190_PREFIX");
}
