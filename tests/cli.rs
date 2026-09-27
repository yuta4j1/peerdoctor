use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Output};
use std::thread;

fn peerdoctor(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_peerdoctor"))
        .args(args)
        .output()
        .expect("peerdoctor を実行できる")
}

fn peerdoctor_offline(args: &[&str]) -> Output {
    peerdoctor(&[args, &["--no-suggest"]].concat())
}

// tests/fixtures/packuments/ の packument を返すレジストリを立て、その URL を返す
fn fixture_registry() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("ポートを確保できる");
    let base = format!("http://{}", listener.local_addr().expect("アドレスがある"));
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let mut reader = BufReader::new(stream.try_clone().expect("複製できる"));
            let mut request_line = String::new();
            let _ = reader.read_line(&mut request_line);
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
            }
            let name = request_line
                .split(' ')
                .nth(1)
                .unwrap_or("/")
                .trim_start_matches('/')
                .replace("%2f", "/");
            let file = format!(
                "{}/tests/fixtures/packuments/{name}.json",
                env!("CARGO_MANIFEST_DIR")
            );
            let (status, body) = match std::fs::read_to_string(file) {
                Ok(body) => ("200 OK", body),
                Err(_) => ("404 Not Found", "{}".to_string()),
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    base
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
    let output = peerdoctor_offline(&["next@16.3.0", "--project", &fixture("nested")]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        "\
Target: next 15.3.0 -> 16.3.0

BLOCKERS (1)
  plugin-a@2.1.0 (node_modules/plugin-a)  peer next: ^14 || ^15

Summary: 1 blocker, 0 unverified
"
    );
}

#[test]
fn reports_every_blocker_at_once() {
    let output = peerdoctor_offline(&["next@16.3.0", "--project", &fixture("basic")]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        "\
Target: next 15.3.0 -> 16.3.0

BLOCKERS (2)
  plugin-a@2.1.0 (node_modules/plugin-a)  peer next: ^14 || ^15
  plugin-c@1.0.0 (node_modules/plugin-c)  peer next: ^15

Summary: 2 blockers, 0 unverified
"
    );
}

#[test]
fn reads_lockfile_v2_the_same_as_v3() {
    let v3 = peerdoctor_offline(&["next@16.3.0", "--project", &fixture("basic")]);
    let v2 = peerdoctor_offline(&["next@16.3.0", "--project", &fixture("basic-v2")]);

    assert_eq!(v2.status.code(), v3.status.code());
    assert_eq!(stdout(&v2), stdout(&v3));
}

#[test]
fn reports_peers_of_scoped_packages() {
    let output = peerdoctor_offline(&["next@16.3.0", "--project", &fixture("scoped")]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains(
            "  @acme/next-plugin@1.0.0 (node_modules/@acme/next-plugin)  peer next: ^15\n"
        )
    );
}

#[test]
fn exits_with_zero_when_nothing_blocks() {
    let output = peerdoctor_offline(&["next@15.3.0", "--project", &fixture("basic")]);

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
        let output = peerdoctor_offline(&args);
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
        let output = peerdoctor_offline(&["next@16.3.0", "--project", &fixture(name)]);
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
    let output = peerdoctor_offline(&["next@16.3.0", "--project", &fixture("overrides")]);

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

    let output = peerdoctor_offline(&[
        "next@16.3.0",
        "--project",
        project.to_str().expect("UTF-8 のパス"),
    ]);
    std::fs::remove_dir_all(&project).expect("一時ディレクトリを消せる");

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("warning: could not read package.json"));
}

#[test]
fn reports_peer_specs_that_are_not_ranges_as_unverified() {
    let output = peerdoctor_offline(&["next@16.3.0", "--project", &fixture("unverified")]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        "\
Target: next 15.3.0 -> 16.3.0

BLOCKERS (1)
  plugin-a@2.1.0 (node_modules/plugin-a)  peer next: ^14 || ^15

UNVERIFIED (1)
  plugin-tag@1.0.0 (node_modules/plugin-tag)  peer next: latest  (not a version range; npm treats it as a dist-tag, which peerdoctor cannot check)

Summary: 1 blocker, 1 unverified
"
    );
}

#[test]
fn exits_with_zero_when_only_unverified_remain() {
    let output = peerdoctor_offline(&["next@15.3.0", "--project", &fixture("unverified")]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("Summary: 0 blockers, 1 unverified\n"));
}

#[test]
fn suggests_versions_that_accept_the_target() {
    let registry = fixture_registry();
    let output = peerdoctor(&[
        "next@16.3.0",
        "--project",
        &fixture("candidates"),
        "--registry",
        &registry,
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stdout(&output),
        "\
Target: next 15.3.0 -> 16.3.0

BLOCKERS (4)
  @acme/next-plugin@1.0.0 (node_modules/@acme/next-plugin)  peer next: ^15
    SATISFIED BY  >=2.0.0
                  minimum 2.0.0 / newest in range 3.0.0 / latest overall 3.0.0
                  note: no peer on next declared: 3.0.0
  plugin-a@2.1.0 (node_modules/plugin-a)  peer next: ^14 || ^15
    SATISFIED BY  >=3.0.0 <4.0.0
                  minimum 3.0.0 / newest in range 3.2.1 / latest overall 4.1.0
                  note: deprecated: 3.0.5
  plugin-c@1.0.0 (node_modules/plugin-c)  peer next: ^15
    DEAD END      no published version accepts next@16.3.0 (latest 1.1.0)
  plugin-flaky@1.1.0 (node_modules/plugin-flaky)  peer next: ^15
    SATISFIED BY  >=1.0.0 <1.1.0 || >=1.2.0 <1.3.0
                  minimum 1.0.0 / newest in range 1.2.0 / latest overall 1.3.0

Summary: 4 blockers (3 satisfiable, 1 dead end), 0 unverified
"
    );
}

#[test]
fn marks_blockers_unverified_when_versions_cannot_be_fetched() {
    let closed = TcpListener::bind("127.0.0.1:0").expect("ポートを確保できる");
    let registry = format!("http://{}", closed.local_addr().expect("アドレスがある"));
    drop(closed);

    let output = peerdoctor(&[
        "next@16.3.0",
        "--project",
        &fixture("basic"),
        "--registry",
        &registry,
    ]);

    assert_eq!(output.status.code(), Some(1));
    let stdout = stdout(&output);
    assert!(stdout.contains(
        "    UNVERIFIED    could not check other versions: could not reach the registry for plugin-a"
    ));
    assert!(stdout.contains("Summary: 2 blockers (0 satisfiable, 0 dead end), 2 unverified\n"));
}
