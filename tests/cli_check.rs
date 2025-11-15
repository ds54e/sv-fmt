use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use walkdir::WalkDir;

fn fixtures_root() -> PathBuf {
    PathBuf::from("tests/fixtures")
}

fn copy_unformatted_fixtures(dest: &Path) {
    let src = fixtures_root().join("unformatted");
    for entry in WalkDir::new(&src).into_iter().filter_map(|e| e.ok()) {
        let rel = entry.path().strip_prefix(&src).unwrap();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dest.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).unwrap();
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn assert_matches_formatted(tree: &Path) {
    let expected_root = fixtures_root().join("formatted");
    for entry in WalkDir::new(&expected_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let rel = entry.path().strip_prefix(&expected_root).unwrap();
        let expected = fs::read_to_string(entry.path()).unwrap();
        let actual_path = tree.join(rel);
        let actual =
            fs::read_to_string(&actual_path).unwrap_or_else(|_| panic!("expected file {:?} to exist", actual_path));
        assert_eq!(actual, expected, "formatted fixture {:?} did not match output", rel);
    }
}

#[test]
fn check_detects_unformatted_file() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("demo.sv");
    fs::write(
        &file,
        "module demo;
initial begin
if (cond)
  a <= 1;
  b <= 2;
end
endmodule
",
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("sv-fmt"))
        .arg("--check")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("needs formatting"));
}

#[test]
fn check_passes_when_formatted() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("demo.sv");
    fs::write(
        &file,
        "module demo;
  initial begin
    if (cond)
    begin
      a <= 1;
      b <= 2;
    end
  end
endmodule
",
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("sv-fmt"))
        .arg("--check")
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn in_place_rewrites_file() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("demo.sv");
    fs::write(
        &file,
        "module demo;
if (cond)
  a <= 1;
  b <= 2;
endmodule
",
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("sv-fmt"))
        .arg("-i")
        .arg(&file)
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("sv-fmt"))
        .arg("--check")
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn check_and_in_place_conflict() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("demo.sv");
    fs::write(&file, "module x; endmodule\n").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("sv-fmt"))
        .arg("--check")
        .arg("-i")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with '--in-place'"));
}

#[test]
fn config_file_overrides_defaults() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("demo.sv");
    fs::write(
        &file,
        "module demo;
initial begin
if (cond)
  foo ();
  bar ();
end
endmodule
",
    )
    .unwrap();

    let config_path = dir.path().join("sv-fmt.toml");
    fs::write(
        &config_path,
        r#"
indent_width = 4
wrap_multiline_blocks = false
space_after_comma = false
remove_call_space = false
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("sv-fmt"))
        .arg("-i")
        .arg("--config")
        .arg(&config_path)
        .arg(&file)
        .assert()
        .success();

    let contents = fs::read_to_string(&file).unwrap();
    assert!(
        contents.contains("        if (cond)"),
        "indent_width should be 4 (8 spaces at depth 2): {contents}"
    );
    assert!(contents.contains("foo ()"), "call spacing should remain: {contents}");
    assert!(
        !contents.contains("if (cond)\n        begin"),
        "wrap_multiline_blocks=false should avoid begin/end insertion: {contents}"
    );
}

#[test]
fn formats_directories_and_individual_files() {
    let dir = tempdir().unwrap();
    copy_unformatted_fixtures(dir.path());

    let extra = dir.path().join("top.sv");
    fs::write(
        &extra,
        "module top;
assign data = foo ( bar );
endmodule
",
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("sv-fmt"))
        .arg("-i")
        .arg(dir.path())
        .arg(&extra)
        .assert()
        .success();

    assert_matches_formatted(dir.path());
    let extra_contents = fs::read_to_string(&extra).unwrap();
    assert!(
        extra_contents.contains("assign data = foo(bar);"),
        "top.sv should also be rewritten: {extra_contents}"
    );
}

#[test]
fn check_uses_config_file() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("demo.sv");
    fs::write(
        &file,
        "module demo;
  initial begin
    foo (a, b);
  end
endmodule
",
    )
    .unwrap();

    let config = dir.path().join("sv-fmt.toml");
    fs::write(
        &config,
        r#"
remove_call_space = false
space_after_comma = false
"#,
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("sv-fmt"))
        .arg("--check")
        .arg(&file)
        .assert()
        .failure();

    Command::new(assert_cmd::cargo::cargo_bin!("sv-fmt"))
        .arg("--check")
        .arg("--config")
        .arg(&config)
        .arg(&file)
        .assert()
        .success();
}

#[test]
fn reports_line_length_violation() {
    let dir = tempdir().unwrap();
    let file = dir.path().join("wide.sv");
    let line = "module wide; assign parametric_bus_value = foo; endmodule";
    fs::write(&file, format!("{line}\n")).unwrap();

    let config = dir.path().join("sv-fmt.toml");
    fs::write(&config, "max_line_length = 20\n").unwrap();

    let expected = format!("has {} columns (max 20)", line.chars().count());
    Command::new(assert_cmd::cargo::cargo_bin!("sv-fmt"))
        .arg("--check")
        .arg("--config")
        .arg(&config)
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains(expected));
}
