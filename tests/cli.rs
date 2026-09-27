use std::process::{Command, Output};

fn peerdoctor(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_peerdoctor"))
        .args(args)
        .output()
        .expect("peerdoctor を実行できる")
}

fn fixture(name: &str) -> String {
    format!(
        "{}/tests/fixtures/lockfiles/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("標準出力は UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("標準エラー出力は UTF-8")
}

#[test]
fn reports_only_peers_that_resolve_to_the_root_target() {
    let output = peerdoctor(&["next@16.3.0", "--project", &fixture("nested")]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        "\
Target: next 15.3.0 -> 16.3.0

BLOCKERS (1)
  plugin-a@2.1.0 (node_modules/plugin-a)  peer next: ^14 || ^15

Summary: 1 blocker
"
    );
}

#[test]
fn reports_every_blocker_at_once() {
    let output = peerdoctor(&["next@16.3.0", "--project", &fixture("basic")]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        "\
Target: next 15.3.0 -> 16.3.0

BLOCKERS (2)
  plugin-a@2.1.0 (node_modules/plugin-a)  peer next: ^14 || ^15
  plugin-c@1.0.0 (node_modules/plugin-c)  peer next: ^15

Summary: 2 blockers
"
    );
}

#[test]
fn reads_lockfile_v2_the_same_as_v3() {
    let v3 = peerdoctor(&["next@16.3.0", "--project", &fixture("basic")]);
    let v2 = peerdoctor(&["next@16.3.0", "--project", &fixture("basic-v2")]);

    assert_eq!(v2.status.code(), v3.status.code());
    assert_eq!(stdout(&v2), stdout(&v3));
}

#[test]
fn reports_peers_of_scoped_packages() {
    let output = peerdoctor(&["next@16.3.0", "--project", &fixture("scoped")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains(
            "  @acme/next-plugin@1.0.0 (node_modules/@acme/next-plugin)  peer next: ^15\n"
        )
    );
}

#[test]
fn exits_with_zero_when_nothing_blocks() {
    let output = peerdoctor(&["next@15.3.0", "--project", &fixture("basic")]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("BLOCKERS (0)\n"));
}

#[test]
fn exits_with_two_when_the_check_cannot_run() {
    let basic = fixture("basic");
    let cases = [
        (
            vec!["next@16", "--project", &basic],
            "is not an exact version",
        ),
        (vec!["next", "--project", &basic], "has no version"),
        (
            vec!["vue@3.0.0", "--project", &basic],
            "is not installed at the project root",
        ),
        (
            vec!["next@16.3.0", "--project", "/nonexistent"],
            "cannot read",
        ),
    ];
    for (args, message) in cases {
        let output = peerdoctor(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            stderr(&output).contains(message),
            "{args:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn exits_with_two_on_unsupported_configurations() {
    let cases = [
        ("unsupported-workspaces", "npm workspaces"),
        ("unsupported-link", "linked package"),
        ("unsupported-file", "file: dependency"),
        ("unsupported-alias", "npm alias"),
        ("unsupported-bundled", "bundled dependencies"),
    ];
    for (name, reason) in cases {
        let output = peerdoctor(&["next@16.3.0", "--project", &fixture(name)]);
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert!(stdout(&output).is_empty(), "{name}");
        let stderr = stderr(&output);
        assert!(
            stderr.contains("unsupported configuration:"),
            "{name}: {stderr}"
        );
        assert!(stderr.contains(reason), "{name}: {stderr}");
    }
}

#[test]
fn warns_about_overrides_and_carries_on() {
    let output = peerdoctor(&["next@16.3.0", "--project", &fixture("overrides")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("warning: package.json declares overrides"));
    assert!(stdout(&output).contains("BLOCKERS (1)\n"));
}

#[test]
fn warns_when_package_json_cannot_be_read() {
    let project =
        std::env::temp_dir().join(format!("peerdoctor-no-manifest-{}", std::process::id()));
    std::fs::create_dir_all(&project).expect("一時ディレクトリを作れる");
    std::fs::copy(
        format!("{}/package-lock.json", fixture("basic")),
        project.join("package-lock.json"),
    )
    .expect("ロックファイルをコピーできる");

    let output = peerdoctor(&[
        "next@16.3.0",
        "--project",
        project.to_str().expect("UTF-8 のパス"),
    ]);
    std::fs::remove_dir_all(&project).expect("一時ディレクトリを消せる");

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("warning: could not read package.json"));
}
