use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

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
        .stderr(predicate::str::contains("--check and --in-place"));
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
