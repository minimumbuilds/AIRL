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
    let valid_dir = fixtures_root().join("valid");
    let files = collect_airl_files(&valid_dir);
    assert!(!files.is_empty(), "No valid fixture files found in {:?}", valid_dir);

    let mut failures = Vec::new();

    for file in &files {
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
