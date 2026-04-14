use std::fs;
use std::path::{Path, PathBuf};

fn run_fixture(source: &str) -> Result<String, String> {
    airl_driver::pipeline::run_source(source)
        .map(|v| format!("{}", v))
        .map_err(|e| format!("{}", e))
}

fn extract_expect(source: &str) -> Option<String> {
    source
        .lines()
        .find(|l| l.contains(";; EXPECT:"))
        .map(|l| l.split(";; EXPECT:").nth(1).unwrap().trim().to_string())
}

fn extract_error(source: &str) -> Option<String> {
    source
        .lines()
        .find(|l| l.contains(";; ERROR:"))
        .map(|l| l.split(";; ERROR:").nth(1).unwrap().trim().to_string())
}

/// Extract all ;;Z3-PROVEN: annotations from a fixture source file.
/// Each annotation names one function that must have been fully Z3-verified.
fn extract_z3_proven(source: &str) -> Vec<String> {
    source
        .lines()
        .filter(|l| l.contains(";;Z3-PROVEN:"))
        .map(|l| l.split(";;Z3-PROVEN:").nth(1).unwrap().trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn fixtures_root() -> PathBuf {
    // Integration tests run from the crate directory, but fixtures are at workspace root
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
}

fn collect_airl_files(dir: &Path) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "airl") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    files.sort();
    files
}

// ── Valid fixture tests ──────────────────────────────────

#[test]
fn valid_fixtures_all_pass() {
    // SEC-6: AIRL_ALLOW_EXEC is injected by .cargo/config.toml [env] before
    // the test binary starts, so no set_var call is needed here (issue-057).
    let valid_dir = fixtures_root().join("valid");
    let files = collect_airl_files(&valid_dir);
    assert!(!files.is_empty(), "No valid fixture files found in {:?}", valid_dir);

    let mut failures = Vec::new();

    for file in &files {
        // Skip import fixtures — they use run_file_with_imports, tested separately
        if file.file_name().map_or(false, |n| n.to_str().map_or(false, |s| s.starts_with("import_"))) {
            continue;
        }
        let source = fs::read_to_string(file).unwrap();
        let expected = match extract_expect(&source) {
            Some(e) => e,
            None => {
                failures.push(format!("{}: missing ;; EXPECT: annotation", file.display()));
                continue;
            }
        };

        match run_fixture(&source) {
            Ok(output) => {
                if output != expected {
                    failures.push(format!(
                        "{}: expected '{}', got '{}'",
                        file.display(),
                        expected,
                        output
                    ));
                }
            }
            Err(err) => {
                failures.push(format!("{}: unexpected error: {}", file.display(), err));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "\n{} valid fixture(s) failed:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    eprintln!("  {} valid fixtures passed", files.len());
}

// ── Error fixture tests ──────────────────────────────────

fn run_error_fixtures(dir_name: &str) -> (usize, Vec<String>) {
    let dir = fixtures_root().join(dir_name);
    let files = collect_airl_files(&dir);
    let mut failures = Vec::new();

    for file in &files {
        let source = fs::read_to_string(file).unwrap();
        let expected_fragment = match extract_error(&source) {
            Some(e) => e,
            None => {
                failures.push(format!("{}: missing ;; ERROR: annotation", file.display()));
                continue;
            }
        };

        // Try check_source first (strict mode catches type errors, missing contracts, etc.).
        // If it fails and the error matches, skip run_fixture.
        let check_result = airl_driver::pipeline::check_source(&source);
        if let Err(ref e) = check_result {
            let err_str = format!("{}", e);
            if err_str.contains(&expected_fragment) {
                continue; // Error caught at check time — test passes
            }
        }

        match run_fixture(&source) {
            Ok(output) => {
                failures.push(format!(
                    "{}: expected error containing '{}', but succeeded with '{}'",
                    file.display(),
                    expected_fragment,
                    output
                ));
            }
            Err(err) => {
                if !err.contains(&expected_fragment) {
                    failures.push(format!(
                        "{}: error message '{}' does not contain '{}'",
                        file.display(),
                        err,
                        expected_fragment
                    ));
                }
            }
        }
    }

    (files.len(), failures)
}

#[test]
fn type_error_fixtures_all_fail() {
    let (count, failures) = run_error_fixtures("type_errors");
    assert!(count > 0, "No type_error fixture files found");
    if !failures.is_empty() {
        panic!(
            "\n{} type_error fixture(s) failed:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }
    eprintln!("  {} type_error fixtures passed", count);
}

#[test]
fn contract_error_fixtures_all_fail() {
    let (count, failures) = run_error_fixtures("contract_errors");
    assert!(count > 0, "No contract_error fixture files found");
    if !failures.is_empty() {
        panic!(
            "\n{} contract_error fixture(s) failed:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }
    eprintln!("  {} contract_error fixtures passed", count);
}

#[test]
fn contract_disproven_fixtures_all_fail() {
    let (count, failures) = run_error_fixtures("contract_disproven");
    assert!(count > 0, "No contract_disproven fixture files found");
    if !failures.is_empty() {
        panic!(
            "\n{} contract_disproven fixture(s) failed:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }
    eprintln!("  {} contract_disproven fixtures passed", count);
}

#[test]
fn linearity_error_fixtures_all_fail() {
    let (count, failures) = run_error_fixtures("linearity_errors");
    assert!(count > 0, "No linearity_error fixture files found");
    if !failures.is_empty() {
        panic!(
            "\n{} linearity_error fixture(s) failed:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }
    eprintln!("  {} linearity_error fixtures passed", count);
}

#[test]
fn check_type_error_fixtures() {
    let dir = fixtures_root().join("type_errors");
    let files = collect_airl_files(&dir);
    if files.is_empty() { return; }

    let mut failures = Vec::new();

    for file in &files {
        let source = fs::read_to_string(file).unwrap();
        let expected_error = extract_error(&source);

        if expected_error.is_some() {
            // Use check_source (which runs type checker in strict mode)
            let check_failed = airl_driver::pipeline::check_source(&source).is_err();
            // Only try running if check passed.
            let run_failed = if check_failed {
                true // already caught by type checker
            } else {
                run_fixture(&source).is_err()
            };

            if !check_failed && !run_failed {
                failures.push(format!(
                    "{}: should fail (check or run) but both check and run passed",
                    file.display()
                ));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "\n{} check_type_error fixture(s) failed:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    eprintln!("  {} check_type_error fixtures verified", files.len());
}

// ── Import integration tests ─────────────────────────────

fn run_import_fixture(fixture_name: &str) -> Result<String, String> {
    let path = fixtures_root().join("valid").join(fixture_name);
    airl_driver::pipeline::run_file_with_imports(path.to_str().unwrap())
        .map(|v| format!("{}", v))
        .map_err(|e| format!("{}", e))
}

#[test]
fn import_basic_prefix() {
    let result = run_import_fixture("import_basic.airl");
    match result {
        Ok(v) => assert_eq!(v, "25", "import_basic.airl expected 25, got {}", v),
        Err(e) => panic!("import_basic.airl failed: {}", e),
    }
}

#[test]
fn import_with_alias() {
    let result = run_import_fixture("import_alias.airl");
    match result {
        Ok(v) => assert_eq!(v, "5", "import_alias.airl expected 5, got {}", v),
        Err(e) => panic!("import_alias.airl failed: {}", e),
    }
}

#[test]
fn import_selective_only() {
    let result = run_import_fixture("import_only.airl");
    match result {
        Ok(v) => assert_eq!(v, "25", "import_only.airl expected 25, got {}", v),
        Err(e) => panic!("import_only.airl failed: {}", e),
    }
}

#[test]
fn import_private_rejected() {
    let path = fixtures_root().join("valid").join("import_private.airl");
    let result = airl_driver::pipeline::run_file_with_imports(path.to_str().unwrap());
    assert!(result.is_err(), "accessing private symbol should fail");
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("not public") || err.contains("private") || err.contains("not found"),
        "error should mention visibility: {}", err);
}

// ── Z3-PROVEN fixture tests ──────────────────────────────

#[test]
fn z3_proven_fixtures_all_pass() {
    let z3_dir = fixtures_root().join("z3_proven");
    let files = collect_airl_files(&z3_dir);

    if files.is_empty() {
        // If the directory doesn't exist yet, skip gracefully
        eprintln!("  no z3_proven fixture files found — skipping");
        return;
    }

    let mut failures = Vec::new();

    for file in &files {
        let source = fs::read_to_string(file).unwrap();
        let proven_names = extract_z3_proven(&source);

        if proven_names.is_empty() {
            // A z3_proven fixture with no annotations is a misconfiguration
            failures.push(format!("{}: no ;;Z3-PROVEN: annotation found", file.display()));
            continue;
        }

        match airl_driver::pipeline::run_source_with_z3_info(&source) {
            Ok((_value, z3_verified)) => {
                for name in &proven_names {
                    if !z3_verified.contains(name) {
                        failures.push(format!(
                            "{}: function '{}' was not Z3-verified (verified: {:?})",
                            file.display(), name, z3_verified
                        ));
                    }
                }
            }
            Err(e) => {
                failures.push(format!("{}: pipeline error: {}", file.display(), e));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "\n{} z3_proven fixture(s) failed:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    eprintln!("  {} z3_proven fixtures passed", files.len());
}
