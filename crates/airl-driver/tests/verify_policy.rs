use std::fs;
use tempfile::TempDir;

use airl_driver::verify_policy::{
    Baseline, BaselineKey, compute_diff, enumerate_airl_files, extract_verify_entries,
};

fn write_file(root: &std::path::Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn parse_tops(src: &str) -> Vec<airl_syntax::ast::TopLevel> {
    use airl_syntax::{Lexer, parse_sexpr_all, parser, diagnostic::Diagnostics};
    let tokens = Lexer::new(src).lex_all().expect("lex failed");
    let sexprs = parse_sexpr_all(tokens).expect("sexpr parse failed");
    let mut diags = Diagnostics::new();
    sexprs.iter()
        .map(|s| parser::parse_top_level(s, &mut diags).expect("top-level parse failed"))
        .collect()
}

#[test]
fn clean_tree_matches_baseline() {
    let td = TempDir::new().unwrap();
    let root = td.path();
    write_file(root, "crates/a/a.airl",
        "(module a :verify checked (defn x :pub :sig [-> i64] :requires [true] :body 0))");
    write_file(root, "crates/b/b.airl",
        "(module b :verify proven (defn y :pub :sig [-> i64] :requires [true] :ensures [(= result 0)] :body 0))");

    let mut baseline = Baseline::new();
    baseline.grandfathered_checked.push(BaselineKey::whole_file("crates/a/a.airl"));

    let mut scanned = Vec::new();
    for f in enumerate_airl_files(root) {
        let src = fs::read_to_string(&f).unwrap();
        let rel = f.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        let tops = parse_tops(&src);
        scanned.extend(extract_verify_entries(&rel, &tops));
    }
    let diff = compute_diff(&baseline, &scanned);
    assert!(diff.is_fully_clean(), "expected clean: {:?}", diff);
}

#[test]
fn unlisted_checked_module_is_regression() {
    let td = TempDir::new().unwrap();
    let root = td.path();
    write_file(root, "crates/a/a.airl",
        "(module a :verify checked (defn x :sig [-> i64] :requires [true] :body 0))");
    let baseline = Baseline::new();

    let mut scanned = Vec::new();
    for f in enumerate_airl_files(root) {
        let src = fs::read_to_string(&f).unwrap();
        let rel = f.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        let tops = parse_tops(&src);
        scanned.extend(extract_verify_entries(&rel, &tops));
    }
    let diff = compute_diff(&baseline, &scanned);
    assert!(!diff.is_clean());
    assert_eq!(diff.new_checked.len(), 1);
    assert_eq!(diff.new_checked[0], BaselineKey::whole_file("crates/a/a.airl"));
}

#[test]
fn baseline_file_roundtrip_on_disk() {
    let td = TempDir::new().unwrap();
    let path = td.path().join(".airl-verify-baseline.toml");
    let mut b = Baseline::new();
    b.grandfathered_checked.push(BaselineKey::whole_file("a.airl"));
    b.grandfathered_trusted.push(BaselineKey::qualified("b.airl", "mod1"));
    b.write(&path).unwrap();
    let loaded = Baseline::load(&path).unwrap();
    assert_eq!(loaded, b);
}
