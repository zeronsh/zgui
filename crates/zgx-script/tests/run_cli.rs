//! `zgx run` and `zgx audit` hand the app to zgx-run: the one beside zgx,
//! else the one on PATH, with the arguments as given and its exit status
//! as theirs. A stand-in zgx-run (a shell script) says what it was given.
#![cfg(unix)]
use std::{os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

/// A copy of zgx in a directory of its own, with a stand-in zgx-run beside
/// it if `with_run`.
fn tools(name: &str, with_run: bool) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-run-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_zgx"), dir.join("zgx")).unwrap();
    if with_run {
        let run = dir.join("zgx-run");
        std::fs::write(&run, "#!/bin/sh\necho \"zgx-run got: $*\"\nexit 7\n").unwrap();
        std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir
}
fn zgx(dir: &std::path::Path, args: &[&str]) -> (Option<i32>, String) {
    // No zgx-run on PATH: only the one beside counts.
    let out = Command::new(dir.join("zgx"))
        .args(args)
        .env("PATH", dir.join("empty"))
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code(), text)
}

#[test]
fn run_and_audit_hand_the_app_to_the_zgx_run_beside_zgx() {
    let dir = tools("beside", true);
    let (code, text) = zgx(&dir, &["run", "--inspect", "my-app", "a.txt"]);
    assert_eq!(code, Some(7), "zgx-run's exit status: {text}");
    assert!(
        text.contains("zgx-run got: --inspect my-app a.txt"),
        "{text}"
    );
    let (code, text) = zgx(&dir, &["audit", "--human", "my-app"]);
    assert_eq!(code, Some(7), "{text}");
    assert!(
        text.contains("zgx-run got: --audit --human my-app"),
        "{text}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn without_zgx_run_it_says_how_to_install_it() {
    let dir = tools("missing", false);
    let (code, text) = zgx(&dir, &["run", "my-app"]);
    assert_eq!(code, Some(2), "{text}");
    assert!(
        text.contains("couldn't run zgx-run") && text.contains("cargo install zgx-run"),
        "{text}"
    );
    let (code, text) = zgx(&dir, &["run"]);
    assert_eq!(code, Some(2), "an app is needed: {text}");
    std::fs::remove_dir_all(dir).unwrap();
}
