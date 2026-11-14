use std::{
    path::PathBuf,
    process::{Command, Output},
};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gop-composer"))
        .args(args)
        .output()
        .unwrap()
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn help_and_version_exit_without_opening_gui() {
    let help = run(&["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--export-gops"));
    let version = run(&["--version"]);
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn invalid_command_and_missing_input_have_failure_exit_codes() {
    assert_eq!(run(&["--not-a-command"]).status.code(), Some(2));
    assert_eq!(run(&["--probe"]).status.code(), Some(2));
    assert_eq!(
        run(&["--probe", "does-not-exist.mp4"]).status.code(),
        Some(1)
    );
}

#[test]
fn timecode_maps_replacement_to_main_gops_three_and_four() {
    let output = run(&[
        "--timecode-map",
        fixture("h264-main.mp4").to_str().unwrap(),
        fixture("h264-replacement.mp4").to_str().unwrap(),
    ]);
    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("replacement GOP 1 -> main GOP 3"));
    assert!(stdout.contains("replacement GOP 2 -> main GOP 4"));
}

#[test]
fn cli_exports_selected_gops_and_rejects_zero_based_selection() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("selected.mp4");
    let input = fixture("h264-audio.mkv");
    let exported = run(&[
        "--export-gops",
        input.to_str().unwrap(),
        "2,5",
        "mp4",
        output.to_str().unwrap(),
    ]);
    assert!(exported.status.success(), "{:?}", exported);
    let probe = run(&["--probe", output.to_str().unwrap()]);
    assert!(probe.status.success());
    let stdout = String::from_utf8_lossy(&probe.stdout);
    assert!(stdout.contains("packets=24 | gops=2"), "{stdout}");
    assert!(stdout.contains("audio aac"), "{stdout}");
    assert_eq!(
        run(&[
            "--export-gops",
            input.to_str().unwrap(),
            "0",
            "mp4",
            output.to_str().unwrap()
        ])
        .status
        .code(),
        Some(2)
    );
}
