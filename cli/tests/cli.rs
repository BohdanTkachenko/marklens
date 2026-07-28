//! Smoke tests for the `marklens` binary.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("marklens-{name}"));
    fs::create_dir_all(&p).unwrap();
    p
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_marklens"))
}

#[test]
fn validate_and_extract() {
    let dir = tmp("validate");
    let schema = dir.join("s.mdp");
    let doc = dir.join("d.md");
    fs::write(&schema, "## @plan Plan\n  - +@cases\n").unwrap();
    fs::write(&doc, "## Plan\n- a\n- b\n").unwrap();

    let out = bin()
        .arg("validate")
        .arg(&schema)
        .arg(&doc)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = bin()
        .arg("extract")
        .arg(&schema)
        .arg(&doc)
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["plan"]["cases"], serde_json::json!(["a", "b"]));
}

#[test]
fn validate_failure_sets_exit_code() {
    let dir = tmp("fail");
    let schema = dir.join("s.mdp");
    let doc = dir.join("d.md");
    fs::write(&schema, "## @plan Plan\n  - +@cases\n").unwrap();
    fs::write(&doc, "## Plan\n").unwrap(); // no items

    let out = bin()
        .arg("validate")
        .arg(&schema)
        .arg(&doc)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(!out.stderr.is_empty());
}

#[test]
fn extract_and_render_round_trip_every_format() {
    let dir = tmp("formats");
    let schema = dir.join("s.mdp");
    let doc = dir.join("d.md");
    fs::write(
        &schema,
        "---\ntitle: string\ncount: int\n---\n## @plan Plan\n  - [ ] +@cases\n",
    )
    .unwrap();
    // Values with XML/YAML/TOML metacharacters, so every codec's escaping is
    // exercised (a regression here — e.g. dropped XML entities — must fail).
    fs::write(
        &doc,
        "---\ntitle: Dark mode\ncount: 3\n---\n\n## Plan\n- [x] a & b <c> \"d\"\n- [ ] plain © 日本\n",
    )
    .unwrap();

    let canonical = run_ok(bin().arg("extract").arg(&schema).arg(&doc));

    for fmt in ["json", "yaml", "toml", "xml"] {
        let data = dir.join(format!("data.{fmt}"));
        let dumped = run_ok(
            bin()
                .arg("extract")
                .args(["-f", fmt])
                .arg(&schema)
                .arg(&doc),
        );
        fs::write(&data, dumped).unwrap();

        let rendered = dir.join(format!("r-{fmt}.md"));
        let md = run_ok(
            bin()
                .arg("render")
                .args(["-f", fmt])
                .arg(&schema)
                .arg(&data),
        );
        fs::write(&rendered, md).unwrap();

        // The rendered document validates and re-extracts to the same JSON.
        let out = bin()
            .arg("validate")
            .arg(&schema)
            .arg(&rendered)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{fmt}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let back = run_ok(bin().arg("extract").arg(&schema).arg(&rendered));
        assert_eq!(back, canonical, "{fmt} round-trip diverged");
    }
}

fn run_ok(cmd: &mut Command) -> String {
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn missing_file_error_names_the_path() {
    let out = bin()
        .arg("validate")
        .arg("/no/such/schema.mdp")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("/no/such/schema.mdp"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn both_inputs_from_stdin_is_rejected() {
    let out = bin().arg("validate").arg("-").arg("-").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("stdin"));
}

#[test]
fn scaffold_render_and_edit() {
    let dir = tmp("edit");
    let schema = dir.join("s.mdp");
    fs::write(&schema, "## @plan Plan\n  - +@cases\n").unwrap();

    // scaffold prints a starter document that validates.
    let out = bin().arg("scaffold").arg(&schema).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("## Plan"));

    // edit to stdout.
    let doc = dir.join("d.md");
    fs::write(&doc, "## Plan\n- old\n- keep\n").unwrap();
    let out = bin()
        .arg("edit")
        .arg(&schema)
        .arg(&doc)
        .arg("cases.0")
        .arg("new")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("- new\n"));

    // edit in place.
    let out = bin()
        .arg("edit")
        .arg(&schema)
        .arg(&doc)
        .arg("cases.1")
        .arg("done")
        .arg("--in-place")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(fs::read_to_string(&doc).unwrap().contains("- done\n"));
}
