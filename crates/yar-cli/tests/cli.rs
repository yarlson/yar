use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use yar_compiler::manifest::{
    Dependency, LockDependency, LockEntry, cache_path, hash_dir, parse_lock_file, parse_manifest,
    write_lock_file,
};

#[test]
fn check_accepts_testdata_fixture() {
    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "check",
            fixture("testdata/hello/main.yar").to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_stops_before_llvm_generation() {
    let dir = temp_dir("yar-cli-check-without-codegen");
    let source = dir.join("main.yar");
    fs::write(
        &source,
        r#"package main

enum Huge {
    Value { data [268435456]i64 }
}

fn main() i32 {
    return 0
}
"#,
    )
    .unwrap();

    let emit_output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["emit-ir", source.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!emit_output.status.success(), "expected emit-ir to fail");
    assert!(
        String::from_utf8_lossy(&emit_output.stderr).contains("array size overflow"),
        "stderr did not include the LLVM generation failure: {}",
        String::from_utf8_lossy(&emit_output.stderr)
    );
    assert!(emit_output.stdout.is_empty());

    let check_output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["check", source.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        check_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&check_output.stderr)
    );
    assert!(check_output.stdout.is_empty());
    assert!(check_output.stderr.is_empty());
}

#[test]
fn check_rejects_method_value_selector() {
    let dir = temp_dir("yar-cli-method-value-check");
    let source = dir.join("main.yar");
    fs::write(
        &source,
        r#"
package main

struct User {
    name str
}

fn (u User) label() str {
    return u.name
}

fn main() i32 {
    user := User{name: "ada"}
    method := user.label
    return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["check", source.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success(), "expected check to fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("method values are not supported"),
        "stderr did not include method-value diagnostic: {stderr}"
    );
}

#[test]
fn check_reports_non_errorable_propagate_operand() {
    let dir = temp_dir("yar-cli-propagate-check");
    let source = dir.join("main.yar");
    fs::write(
        &source,
        r#"
package main

fn main() i32 {
    x := 1?
    return x
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["check", source.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success(), "expected check to fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("? requires an errorable expression or error value"),
        "stderr did not include propagate operand diagnostic: {stderr}"
    );
    assert!(
        !stderr.contains("cannot use ? in a function that cannot return an error"),
        "stderr should not report propagation-context failure for a non-errorable operand: {stderr}"
    );
}

#[test]
fn check_rejects_value_or_handler_that_falls_through() {
    let dir = temp_dir("yar-cli-or-handler-check");
    let source = dir.join("main.yar");
    fs::write(
        &source,
        r#"
package main

fn maybe() !i32 {
    return 1
}

fn main() i32 {
    value := maybe() or |err| {
        print(to_str(err))
    }
    return value
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["check", source.to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success(), "expected check to fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("or handler for a value result must terminate control flow"),
        "stderr did not include value-handler termination diagnostic: {stderr}"
    );
}

#[test]
fn emit_ir_prints_rust_codegen_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "emit-ir",
            fixture("testdata/hello/main.yar").to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("; yar rust codegen"));
    assert!(stdout.contains("define i32 @main(i32 %argc, ptr %argv)"));
}

#[test]
fn run_executes_testdata_fixture() {
    let dir = temp_dir("yar-cli-run");
    let runtime_archive = build_runtime_archive(&dir);

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["run", fixture("testdata/hello/main.yar").to_str().unwrap()])
        .env("YAR_RUNTIME_ARCHIVE", &runtime_archive)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello, world\n");
}

#[cfg(unix)]
#[test]
fn build_uses_explicit_runtime_archive_without_cargo() {
    let dir = temp_dir("yar-cli-runtime-archive");
    let archive = dir.join("runtime.a");
    fs::write(&archive, b"not a real archive").unwrap();
    let cc = fake_cc(&dir);
    let output_path = dir.join("program");

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "build",
            fixture("testdata/hello/main.yar").to_str().unwrap(),
            "-o",
            output_path.to_str().unwrap(),
        ])
        .env("CC", &cc)
        .env("PATH", &dir)
        .env("YAR_RUNTIME_ARCHIVE", &archive)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output_path.is_file());
    let cc_args = fs::read_to_string(dir.join("cc-args")).unwrap();
    assert!(
        cc_args.contains(archive.to_str().unwrap()),
        "cc args did not include explicit runtime archive: {cc_args}"
    );
}

#[cfg(unix)]
#[test]
fn build_allows_cross_target_with_explicit_runtime_archive() {
    let dir = temp_dir("yar-cli-cross-runtime-archive");
    let archive = dir.join("runtime.a");
    fs::write(&archive, b"not a real archive").unwrap();
    let cc = fake_cc(&dir);
    let output_path = dir.join("program");
    let (target_os, target_arch, target_triple) = non_host_unix_target();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "build",
            fixture("testdata/hello/main.yar").to_str().unwrap(),
            "-o",
            output_path.to_str().unwrap(),
        ])
        .env("CC", &cc)
        .env("PATH", &dir)
        .env("YAR_RUNTIME_ARCHIVE", &archive)
        .env("YAR_OS", target_os)
        .env("YAR_ARCH", target_arch)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output_path.is_file());
    let cc_args = fs::read_to_string(dir.join("cc-args")).unwrap();
    assert!(
        cc_args.contains(&format!("--target={target_triple}")),
        "cc args did not include cross target: {cc_args}"
    );
    assert!(
        cc_args.contains(archive.to_str().unwrap()),
        "cc args did not include explicit runtime archive: {cc_args}"
    );
}

#[cfg(unix)]
#[test]
fn build_rejects_cross_target_without_explicit_runtime_archive() {
    let dir = temp_dir("yar-cli-cross-missing-runtime-archive");
    let cc = fake_cc(&dir);
    let output_path = dir.join("program");
    let (target_os, target_arch, _target_triple) = non_host_unix_target();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "build",
            fixture("testdata/hello/main.yar").to_str().unwrap(),
            "-o",
            output_path.to_str().unwrap(),
        ])
        .env("CC", &cc)
        .env("PATH", &dir)
        .env_remove("YAR_RUNTIME_ARCHIVE")
        .env("YAR_OS", target_os)
        .env("YAR_ARCH", target_arch)
        .output()
        .unwrap();

    assert!(!output.status.success(), "expected cross build to fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("requires YAR_RUNTIME_ARCHIVE"),
        "stderr did not explain runtime archive requirement: {stderr}"
    );
    assert!(!output_path.exists());
    assert!(!dir.join("cc-args").exists());
}

#[cfg(unix)]
#[test]
fn build_windows_target_uses_gnu_triple_and_windows_link_flags() {
    let dir = temp_dir("yar-cli-windows-target");
    let archive = dir.join("runtime.a");
    fs::write(&archive, b"not a real archive").unwrap();
    let cc = fake_cc(&dir);
    let output_path = dir.join("program.exe");

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "build",
            fixture("testdata/hello/main.yar").to_str().unwrap(),
            "-o",
            output_path.to_str().unwrap(),
        ])
        .env("CC", &cc)
        .env("PATH", &dir)
        .env("YAR_RUNTIME_ARCHIVE", &archive)
        .env("YAR_OS", "windows")
        .env("YAR_ARCH", "amd64")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output_path.is_file());
    let cc_args = fs::read_to_string(dir.join("cc-args")).unwrap();
    assert!(
        cc_args.contains("--target=x86_64-pc-windows-gnu"),
        "cc args did not use the release-compatible GNU Windows target: {cc_args}"
    );
    assert!(
        cc_args.contains("-lws2_32"),
        "cc args did not link Winsock for Windows target: {cc_args}"
    );
    assert!(
        !cc_args.contains("-pthread"),
        "cc args should not include POSIX pthread for Windows target: {cc_args}"
    );
}

#[cfg(unix)]
#[test]
fn build_process_env_fixture_runs_with_rust_runtime() {
    let dir = temp_dir("yar-cli-process-env");
    let capture_script = dir.join("capture.sh");
    let inherit_script = dir.join("inherit.sh");
    write_executable(
        &capture_script,
        "#!/bin/sh\nprintf 'captured stdout\\n'\nprintf 'captured stderr\\n' >&2\nexit 7\n",
    );
    write_executable(
        &inherit_script,
        "#!/bin/sh\nprintf 'inherit stdout\\n'\nprintf 'inherit stderr\\n' >&2\nexit 3\n",
    );
    let runtime_archive = build_runtime_archive(&dir);
    let program = dir.join("process-env");

    let build = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "build",
            fixture("testdata/stdlib_process_env/main.yar")
                .to_str()
                .unwrap(),
            "-o",
            program.to_str().unwrap(),
        ])
        .env("YAR_RUNTIME_ARCHIVE", &runtime_archive)
        .output()
        .unwrap();

    assert!(
        build.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let output = Command::new(&program)
        .arg(&capture_script)
        .arg(&inherit_script)
        .env("YAR_PROCESS_ENV_TEST", "env ok")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for expected in [
        "stdio stderr\n",
        "inherit stdout\n",
        "inherit stderr\n",
        "process_env ok\n",
    ] {
        assert!(
            combined.contains(expected),
            "expected {expected:?} in output {combined:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn build_net_fixture_runs_with_rust_runtime() {
    let dir = temp_dir("yar-cli-net");
    let runtime_archive = build_runtime_archive(&dir);
    let program = dir.join("net");

    let build = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "build",
            fixture("testdata/stdlib_net/main.yar").to_str().unwrap(),
            "-o",
            program.to_str().unwrap(),
        ])
        .env("YAR_RUNTIME_ARCHIVE", &runtime_archive)
        .output()
        .unwrap();

    assert!(
        build.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let output = Command::new(&program).output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "net ok\n");
}

#[test]
fn test_runs_passing_tests() {
    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["test", fixture("testdata/testing_basic").to_str().unwrap()])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("PASS: test_add"));
    assert!(stdout.contains("5 passed, 0 failed"));
}

#[test]
fn test_reports_failing_tests() {
    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["test", fixture("testdata/testing_fail").to_str().unwrap()])
        .output()
        .unwrap();

    assert!(!output.status.success(), "expected failing test command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FAIL: test_wrong_sum"));
    assert!(stdout.contains("got 4, want 5"));
    assert!(stdout.contains("PASS: test_pass"));
    assert!(stdout.contains("1 passed, 2 failed"));
}

#[test]
fn init_creates_manifest() {
    let dir = temp_dir("yar-cli-init");
    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .arg("init")
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "created yar.toml\n"
    );
    let content = fs::read_to_string(dir.join("yar.toml")).unwrap();
    let manifest = parse_manifest(&content).unwrap();
    assert!(!manifest.package.name.is_empty());
    assert!(manifest.dependencies.is_empty());
}

#[test]
fn init_ignores_ancestor_discovery_and_supports_an_explicit_target() {
    let dir = temp_dir("yar-cli-init-project-selection");
    let parent = dir.join("parent");
    let child = parent.join("child");
    let explicit = dir.join("explicit_project");
    fs::create_dir_all(&child).unwrap();
    fs::create_dir_all(&explicit).unwrap();
    fs::write(parent.join("yar.toml"), "[package]\nname = \"parent\"\n").unwrap();

    let nested = Command::new(env!("CARGO_BIN_EXE_yar"))
        .arg("init")
        .current_dir(&child)
        .output()
        .unwrap();
    assert!(
        nested.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&nested.stderr)
    );
    assert!(child.join("yar.toml").is_file());

    let explicit_manifest = explicit.join("yar.toml");
    let selected = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "--manifest-path",
            explicit_manifest.to_str().unwrap(),
            "init",
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        selected.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&selected.stderr)
    );
    let manifest = parse_manifest(&fs::read_to_string(&explicit_manifest).unwrap()).unwrap();
    assert_eq!(manifest.package.name, "explicit_project");
}

#[test]
fn nested_dependency_commands_mutate_the_nearest_ancestor_project() {
    let root = temp_dir("yar-cli-nested-metadata-command");
    let child = root.join("work/nested");
    let local = root.join("local-lib");
    fs::create_dir_all(&child).unwrap();
    fs::create_dir_all(&local).unwrap();
    fs::write(root.join("yar.toml"), "[package]\nname = \"app\"\n").unwrap();
    fs::write(local.join("lib.yar"), "package local_lib\n").unwrap();

    let add = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["add", "local_lib", "--path=local-lib"])
        .current_dir(&child)
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    assert!(!child.join("yar.toml").exists());
    let manifest = parse_manifest(&fs::read_to_string(root.join("yar.toml")).unwrap()).unwrap();
    assert_eq!(manifest.dependencies["local_lib"].path, "local-lib");

    let remove = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["remove", "local_lib"])
        .current_dir(&child)
        .output()
        .unwrap();
    assert!(
        remove.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&remove.stderr)
    );
    let manifest = parse_manifest(&fs::read_to_string(root.join("yar.toml")).unwrap()).unwrap();
    assert!(manifest.dependencies.is_empty());
}

#[test]
fn explicit_add_bootstraps_only_the_selected_project() {
    let dir = temp_dir("yar-cli-explicit-add-bootstrap");
    let project = dir.join("selected_project");
    let invocation = dir.join("invocation");
    let dependency = project.join("dep");
    fs::create_dir_all(&invocation).unwrap();
    fs::create_dir_all(&dependency).unwrap();
    fs::write(dependency.join("dep.yar"), "package dep\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "--manifest-path",
            project.join("yar.toml").to_str().unwrap(),
            "add",
            "dep",
            "--path=dep",
        ])
        .current_dir(&invocation)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!invocation.join("yar.toml").exists());
    let manifest = parse_manifest(&fs::read_to_string(project.join("yar.toml")).unwrap()).unwrap();
    assert_eq!(manifest.package.name, "selected_project");
    assert_eq!(manifest.dependencies["dep"].path, "dep");
}

#[test]
fn check_discovers_an_ancestor_manifest_for_a_nested_entry() {
    let root = temp_dir("yar-cli-nested-entry-project");
    let entry = root.join("cmd/app");
    let dependency = root.join("dep");
    let shared = root.join("shared");
    fs::create_dir_all(&entry).unwrap();
    fs::create_dir_all(&dependency).unwrap();
    fs::create_dir_all(&shared).unwrap();
    fs::write(
        root.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
dep = { path = "dep" }
"#,
    )
    .unwrap();
    fs::write(
        dependency.join("dep.yar"),
        "package dep\n\npub fn value() i32 { return 20 }\n",
    )
    .unwrap();
    fs::write(
        shared.join("shared.yar"),
        "package shared\n\npub fn value() i32 { return 22 }\n",
    )
    .unwrap();
    fs::write(
        entry.join("main.yar"),
        r#"package main

import "dep"
import "shared"

fn main() i32 {
    return dep.value() + shared.value()
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["check", entry.to_str().unwrap()])
        .current_dir(root.parent().unwrap())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn explicit_manifest_overrides_a_nearer_manifest() {
    let workspace = temp_dir("yar-cli-explicit-manifest");
    let outer = workspace.join("outer");
    let inner = outer.join("inner");
    let entry = inner.join("app");
    let dependency = outer.join("dep");
    let invocation = workspace.join("invocation");
    fs::create_dir_all(&entry).unwrap();
    fs::create_dir_all(&dependency).unwrap();
    fs::create_dir_all(&invocation).unwrap();
    fs::write(
        outer.join("yar.toml"),
        r#"[package]
name = "outer"

[dependencies]
dep = { path = "dep" }
"#,
    )
    .unwrap();
    fs::write(inner.join("yar.toml"), "[package]\nname = \"inner\"\n").unwrap();
    fs::write(
        dependency.join("dep.yar"),
        "package dep\n\npub fn value() i32 { return 0 }\n",
    )
    .unwrap();
    fs::write(
        entry.join("main.yar"),
        "package main\n\nimport \"dep\"\n\nfn main() i32 { return dep.value() }\n",
    )
    .unwrap();

    let automatic = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["check", "../outer/inner/app"])
        .current_dir(&invocation)
        .output()
        .unwrap();
    assert!(!automatic.status.success(), "nearer manifest was skipped");

    let explicit = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "--manifest-path",
            "../outer/yar.toml",
            "check",
            "../outer/inner/app",
        ])
        .current_dir(&invocation)
        .output()
        .unwrap();
    assert!(
        explicit.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&explicit.stderr)
    );
}

#[test]
fn explicit_selection_does_not_recover_the_invocation_directory() {
    let dir = temp_dir("yar-cli-explicit-selection-recovery-scope");
    let project = dir.join("project");
    let invocation = dir.join("invocation");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&invocation).unwrap();
    fs::write(project.join("yar.toml"), "[package]\nname = \"project\"\n").unwrap();
    fs::write(
        project.join("main.yar"),
        "package main\n\nfn main() i32 { return 0 }\n",
    )
    .unwrap();
    let unrelated_journal = invocation.join(".yar-metadata-transaction");
    fs::create_dir(&unrelated_journal).unwrap();
    fs::write(unrelated_journal.join("version"), "malformed\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "--manifest-path",
            project.join("yar.toml").to_str().unwrap(),
            "check",
            project.join("main.yar").to_str().unwrap(),
        ])
        .current_dir(&invocation)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read(unrelated_journal.join("version")).unwrap(),
        b"malformed\n"
    );
    assert!(unrelated_journal.exists());
}

#[test]
fn nested_check_recovers_an_ancestor_transaction_without_a_manifest() {
    let root = temp_dir("yar-cli-nested-first-add-recovery");
    let entry = root.join("nested/app");
    fs::create_dir_all(&entry).unwrap();
    fs::write(
        entry.join("main.yar"),
        "package main\n\nfn main() i32 { return 0 }\n",
    )
    .unwrap();
    let journal = root.join(".yar-metadata-transaction");
    fs::create_dir(&journal).unwrap();
    fs::write(journal.join("version"), "1\n").unwrap();
    fs::write(journal.join("yar.toml.before-absent"), "").unwrap();
    fs::write(journal.join("yar.lock.before-absent"), "").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["check", "main.yar"])
        .current_dir(&entry)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!journal.exists());
    assert!(!root.join(".yar-metadata-transaction.complete").exists());
    assert!(!root.join("yar.toml").exists());
}

#[test]
fn manifest_path_is_a_single_prefix_and_usage_errors_do_not_recover() {
    let dir = temp_dir("yar-cli-manifest-path-usage");
    let journal = dir.join(".yar-metadata-transaction");
    fs::create_dir(&journal).unwrap();
    fs::write(journal.join("version"), "1\n").unwrap();

    let cases: &[&[&str]] = &[
        &["--manifest-path"],
        &["--manifest-path", ""],
        &["--manifest-path", "other.toml", "check", "main.yar"],
        &[
            "--manifest-path",
            "yar.toml",
            "--manifest-path",
            "yar.toml",
            "check",
            "main.yar",
        ],
        &["check", "--manifest-path", "yar.toml", "main.yar"],
    ];

    for args in cases {
        let output = Command::new(env!("CARGO_BIN_EXE_yar"))
            .args(*args)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "args {args:?}, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            journal.exists(),
            "usage error recovered journal for {args:?}"
        );
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn explicit_manifest_rejects_an_entry_outside_its_project() {
    let dir = temp_dir("yar-cli-entry-outside-project");
    let project = dir.join("project");
    let outside = dir.join("outside");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(project.join("yar.toml"), "[package]\nname = \"project\"\n").unwrap();
    fs::write(
        outside.join("main.yar"),
        "package main\n\nfn main() i32 { return 0 }\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "--manifest-path",
            project.join("yar.toml").to_str().unwrap(),
            "check",
            outside.to_str().unwrap(),
        ])
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("outside the selected project root"));
    assert!(output.stdout.is_empty());
}

#[test]
fn explicit_missing_manifest_is_not_an_implicit_manifestless_project() {
    let project = temp_dir("yar-cli-explicit-missing-manifest");
    let entry = project.join("main.yar");
    fs::write(&entry, "package main\n\nfn main() i32 { return 0 }\n").unwrap();
    let manifest = project.join("yar.toml");

    for command in [vec!["check", entry.to_str().unwrap()], vec!["fetch"]] {
        let mut args = vec!["--manifest-path", manifest.to_str().unwrap()];
        args.extend(command);
        let output = Command::new(env!("CARGO_BIN_EXE_yar"))
            .args(&args)
            .current_dir(&project)
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "command {args:?} accepted no manifest"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("does not exist"),
            "command {args:?}, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(!manifest.exists());
    }
}

#[test]
fn malformed_nearest_manifest_fails_instead_of_falling_back() {
    let root = temp_dir("yar-cli-malformed-nearest-manifest");
    let inner = root.join("inner");
    let entry = inner.join("app");
    fs::create_dir_all(&entry).unwrap();
    fs::write(root.join("yar.toml"), "[package]\nname = \"outer\"\n").unwrap();
    fs::create_dir(inner.join("yar.toml")).unwrap();
    fs::write(
        entry.join("main.yar"),
        "package main\n\nfn main() i32 { return 0 }\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["check", entry.to_str().unwrap()])
        .current_dir(&root)
        .output()
        .unwrap();

    assert!(!output.status.success(), "non-file manifest was skipped");
    assert!(String::from_utf8_lossy(&output.stderr).contains("is not a regular file"));

    fs::remove_dir(inner.join("yar.toml")).unwrap();
    fs::write(inner.join("yar.toml"), "not valid TOML = [\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["check", entry.to_str().unwrap()])
        .current_dir(&root)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "malformed manifest was skipped");
    assert!(stderr.contains("TOML"), "stderr: {stderr}");
    assert!(
        stderr.contains(inner.join("yar.toml").to_str().unwrap()),
        "stderr: {stderr}"
    );
}

#[test]
fn every_project_command_honors_the_explicit_manifest() {
    let dir = temp_dir("yar-cli-explicit-manifest-command-matrix");
    let project = dir.join("project");
    let inner = project.join("inner");
    let invocation = dir.join("invocation");
    let entry = inner.join("main.yar");
    fs::create_dir_all(&inner).unwrap();
    fs::create_dir_all(&invocation).unwrap();
    fs::write(project.join("yar.toml"), "not valid TOML = [\n").unwrap();
    fs::create_dir(inner.join("yar.toml")).unwrap();
    fs::write(&entry, "package main\n\nfn main() i32 { return 0 }\n").unwrap();
    let manifest = project.join("yar.toml");

    let compile_commands: &[&[&str]] = &[
        &["check", entry.to_str().unwrap()],
        &["emit-ir", entry.to_str().unwrap()],
        &["build", entry.to_str().unwrap()],
        &["run", entry.to_str().unwrap()],
        &["test", entry.to_str().unwrap()],
    ];
    for command in compile_commands {
        let mut args = vec!["--manifest-path", manifest.to_str().unwrap()];
        args.extend_from_slice(command);
        let output = Command::new(env!("CARGO_BIN_EXE_yar"))
            .args(&args)
            .current_dir(&invocation)
            .env_remove("YAR_OS")
            .env_remove("YAR_ARCH")
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "command {command:?} ignored selected malformed manifest"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("TOML"),
            "command {command:?}, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(manifest.to_str().unwrap()),
            "command {command:?}, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let metadata_commands: &[&[&str]] = &[
        &["add", "dep", "--path=dep"],
        &["remove", "dep"],
        &["fetch"],
        &["lock"],
        &["update"],
        &["update", "dep"],
    ];
    for command in metadata_commands {
        let mut args = vec!["--manifest-path", manifest.to_str().unwrap()];
        args.extend_from_slice(command);
        let output = Command::new(env!("CARGO_BIN_EXE_yar"))
            .args(&args)
            .current_dir(&invocation)
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "command {command:?} ignored selected malformed manifest"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("TOML"),
            "command {command:?}, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
    }
    assert!(!invocation.join("yar.toml").exists());
}

#[test]
fn add_and_remove_local_dependency_updates_manifest() {
    let dir = temp_dir("yar-cli-local-dep");

    let add = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["add", "local_lib", "--path=../local-lib"])
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(
        add.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&add.stdout),
        "added dependency \"local_lib\"\n"
    );
    let content = fs::read_to_string(dir.join("yar.toml")).unwrap();
    let manifest = parse_manifest(&content).unwrap();
    assert_eq!(
        manifest.dependencies["local_lib"].path,
        "../local-lib".to_string()
    );

    fs::write(dir.join("yar.lock"), "stale lock").unwrap();
    let remove = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["remove", "local_lib"])
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(
        remove.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&remove.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&remove.stdout),
        "removed dependency \"local_lib\"\n"
    );
    let content = fs::read_to_string(dir.join("yar.toml")).unwrap();
    let manifest = parse_manifest(&content).unwrap();
    assert!(manifest.dependencies.is_empty());
    assert!(!dir.join("yar.lock").exists());
}

#[test]
fn add_resolution_failure_preserves_metadata_pair_and_stdout() {
    let dir = temp_dir("yar-cli-add-metadata-rollback");
    let broken = dir.join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("yar.toml"), "this is not valid TOML = [\n").unwrap();
    let original_manifest = r#"[package]
name = "app"

[dependencies]
broken = { path = "broken" }
"#;
    let original_lock = "original lock bytes\n";
    fs::write(dir.join("yar.toml"), original_manifest).unwrap();
    fs::write(dir.join("yar.lock"), original_lock).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["add", "extra", "--path=extra"])
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "malformed path manifest was accepted"
    );
    assert!(
        output.stdout.is_empty(),
        "failed add emitted success output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(dir.join("yar.toml")).unwrap(),
        original_manifest
    );
    assert_eq!(
        fs::read_to_string(dir.join("yar.lock")).unwrap(),
        original_lock
    );
}

#[test]
fn first_add_resolution_failure_preserves_absent_metadata() {
    let dir = temp_dir("yar-cli-first-add-metadata-rollback");
    let broken = dir.join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("yar.toml"), "this is not valid TOML = [\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["add", "broken", "--path=broken"])
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "malformed path manifest was accepted"
    );
    assert!(
        output.stdout.is_empty(),
        "failed first add emitted success output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!dir.join("yar.toml").exists());
    assert!(!dir.join("yar.lock").exists());
    assert!(!dir.join(".yar-metadata-transaction").exists());
}

#[test]
fn add_publication_failure_emits_no_success_output() {
    let dir = temp_dir("yar-cli-add-publication-failure");
    fs::create_dir(dir.join("yar.lock")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["add", "local", "--path=local"])
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(!output.status.success(), "invalid lock target was accepted");
    assert!(
        output.stdout.is_empty(),
        "failed publication emitted success output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!dir.join("yar.toml").exists());
    assert!(dir.join("yar.lock").is_dir());
    assert!(!dir.join(".yar-metadata-transaction").exists());
}

#[test]
fn remove_resolution_failure_preserves_metadata_pair_and_stdout() {
    let dir = temp_dir("yar-cli-remove-metadata-rollback");
    let broken = dir.join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("yar.toml"), "this is not valid TOML = [\n").unwrap();
    let original_manifest = r#"[package]
name = "app"

[dependencies]
broken = { path = "broken" }
remove_me = { path = "remove-me" }
"#;
    let original_lock = "original lock bytes\n";
    fs::write(dir.join("yar.toml"), original_manifest).unwrap();
    fs::write(dir.join("yar.lock"), original_lock).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["remove", "remove_me"])
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "remove accepted a malformed remaining dependency"
    );
    assert!(
        output.stdout.is_empty(),
        "failed remove emitted success output: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        fs::read_to_string(dir.join("yar.toml")).unwrap(),
        original_manifest
    );
    assert_eq!(
        fs::read_to_string(dir.join("yar.lock")).unwrap(),
        original_lock
    );
}

#[test]
fn prepared_metadata_recovery_runs_before_fetch() {
    let dir = temp_dir("yar-cli-prepared-metadata-recovery");
    let old_manifest = r#"[package]
name = "app"

[dependencies]
local = { path = "local" }
"#;
    let partial_manifest = r#"[package]
name = "app"

[dependencies]
remote = { git = "https://example.com/remote.git", tag = "v1" }
"#;
    fs::write(dir.join("yar.toml"), partial_manifest).unwrap();
    let journal = dir.join(".yar-metadata-transaction");
    fs::create_dir(&journal).unwrap();
    fs::write(journal.join("version"), "1\n").unwrap();
    fs::write(journal.join("yar.toml.before"), old_manifest).unwrap();
    fs::write(journal.join("yar.lock.before-absent"), "").unwrap();
    fs::write(journal.join("yar.lock.after"), "partially staged lock\n").unwrap();

    for _ in 0..2 {
        let output = Command::new(env!("CARGO_BIN_EXE_yar"))
            .arg("fetch")
            .current_dir(&dir)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "all dependencies fetched\n"
        );
        assert_eq!(
            fs::read_to_string(dir.join("yar.toml")).unwrap(),
            old_manifest
        );
        assert!(!dir.join("yar.lock").exists());
        assert!(!journal.exists());
    }
}

#[test]
fn remove_last_git_dependency_commits_manifest_and_lock_deletion() {
    let dir = temp_dir("yar-cli-remove-last-git-dependency");
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
remote = { git = "https://example.com/remote.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(dir.join("yar.lock"), "old lock bytes\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["remove", "remote"])
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "removed dependency \"remote\"\n"
    );
    let manifest = parse_manifest(&fs::read_to_string(dir.join("yar.toml")).unwrap()).unwrap();
    assert!(manifest.dependencies.is_empty());
    assert!(!dir.join("yar.lock").exists());
    assert!(!dir.join(".yar-metadata-transaction").exists());
}

#[test]
fn path_only_lock_deletion_preserves_manifest_bytes() {
    let dir = temp_dir("yar-cli-path-lock-deletion");
    let manifest = r#"# keep this formatting
[package]
name = "app"

[dependencies]
local = { path = "local" }
"#;
    fs::write(dir.join("yar.toml"), manifest).unwrap();
    fs::write(dir.join("yar.lock"), "stale lock bytes\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .arg("lock")
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read_to_string(dir.join("yar.toml")).unwrap(), manifest);
    assert!(!dir.join("yar.lock").exists());
}

#[test]
fn full_lock_and_update_failures_preserve_the_prior_lock() {
    let dir = temp_dir("yar-cli-full-lock-failure");
    let broken = dir.join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("yar.toml"), "this is not valid TOML = [\n").unwrap();
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
broken = { path = "broken" }
"#,
    )
    .unwrap();
    let original_lock = "original lock bytes\n";
    fs::write(dir.join("yar.lock"), original_lock).unwrap();

    for command in ["lock", "update"] {
        let output = Command::new(env!("CARGO_BIN_EXE_yar"))
            .arg(command)
            .current_dir(&dir)
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "{command} accepted a malformed path manifest"
        );
        assert!(
            output.stdout.is_empty(),
            "failed {command} emitted success output: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(
            fs::read_to_string(dir.join("yar.lock")).unwrap(),
            original_lock
        );
    }
}

#[test]
fn add_rejects_reserved_std_alias_without_writing_manifest() {
    let dir = temp_dir("yar-cli-reserved-stdlib-alias");

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["add", "std", "--path=../stdlib-shadow"])
        .current_dir(&dir)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "dependency alias \"std\" is reserved for the standard library\n"
    );
    assert!(!dir.join("yar.toml").exists());
}

#[test]
fn check_verifies_a_locked_dependency_before_parsing_it() {
    let project = locked_dependency_project("yar-cli-tampered-locked-cache");
    let valid = run_yar(&project.dir, &project.cache_dir, &["check", "."]);
    assert!(
        valid.status.success(),
        "valid locked cache failed: {}",
        String::from_utf8_lossy(&valid.stderr)
    );

    let source = project.dependency_dir.join("dep.yar");
    fs::write(&source, "this is not valid Yar source\n").unwrap();

    let output = run_yar(&project.dir, &project.cache_dir, &["check", "."]);

    assert!(!output.status.success(), "expected check to fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("hash mismatch"),
        "stderr did not report cache corruption: {stderr}"
    );
    assert!(
        !stderr.contains("expected package declaration"),
        "tampered source was parsed before its cache hash was checked: {stderr}"
    );
}

#[test]
fn check_rejects_a_missing_locked_cache_without_stdlib_substitution() {
    let dir = temp_dir("yar-cli-missing-locked-cache");
    let cache_dir = dir.join("cache");
    let git = "https://example.com/strings.git";
    let commit = "3333333333333333333333333333333333333333";
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
strings = { git = "https://example.com/strings.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("yar.lock"),
        write_lock_file(&[locked_entry(
            "strings",
            git,
            commit,
            format!("sha256:{}", "0".repeat(64)),
        )]),
    )
    .unwrap();
    fs::write(
        dir.join("main.yar"),
        r#"package main

import "strings"

fn main() i32 {
    if strings.contains("yar", "ar") {
        return 0
    }
    return 1
}
"#,
    )
    .unwrap();

    let output = run_yar(&dir, &cache_dir, &["check", "."]);

    assert!(
        !output.status.success(),
        "missing locked dependency was replaced by the same-named stdlib package"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("strings") && stderr.contains("run 'yar fetch'"),
        "stderr did not report the missing locked cache: {stderr}"
    );
}

#[test]
fn check_rejects_a_missing_direct_lock_entry_without_stdlib_substitution() {
    let dir = temp_dir("yar-cli-missing-direct-lock-entry");
    let cache_dir = dir.join("cache");
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
strings = { git = "https://example.com/strings.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(dir.join("yar.lock"), write_lock_file(&[])).unwrap();
    fs::write(
        dir.join("main.yar"),
        "package main\n\nimport \"strings\"\n\nfn main() i32 { return 0 }\n",
    )
    .unwrap();

    let output = run_yar(&dir, &cache_dir, &["check", "."]);

    assert!(!output.status.success(), "missing lock entry was accepted");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing package \"strings\"") && stderr.contains("run 'yar lock'"),
        "stderr: {stderr}"
    );
    assert!(
        !cache_dir.exists(),
        "lock reconciliation touched the dependency cache"
    );
}

#[test]
fn check_only_verifies_a_locked_cache_when_the_dependency_is_selected() {
    let dir = temp_dir("yar-cli-lazy-locked-cache");
    let cache_dir = dir.join("cache");
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
dep = { git = "https://example.com/dep.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("yar.lock"),
        write_lock_file(&[locked_entry(
            "dep",
            "https://example.com/dep.git",
            "3333333333333333333333333333333333333333",
            format!("sha256:{}", "0".repeat(64)),
        )]),
    )
    .unwrap();
    fs::write(
        dir.join("main.yar"),
        "package main\n\nfn main() i32 { return 0 }\n",
    )
    .unwrap();

    let unused = run_yar(&dir, &cache_dir, &["check", "."]);
    assert!(
        unused.status.success(),
        "unused locked dependency required a cache: {}",
        String::from_utf8_lossy(&unused.stderr)
    );

    fs::create_dir_all(dir.join("dep")).unwrap();
    fs::write(
        dir.join("dep").join("dep.yar"),
        "package dep\n\npub fn value() i32 { return 0 }\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.yar"),
        "package main\n\nimport \"dep\"\n\nfn main() i32 { return dep.value() }\n",
    )
    .unwrap();

    let shadowed = run_yar(&dir, &cache_dir, &["check", "."]);
    assert!(
        shadowed.status.success(),
        "local package did not shadow the uncached dependency: {}",
        String::from_utf8_lossy(&shadowed.stderr)
    );
}

#[test]
fn check_rejects_selected_dependency_manifest_edge_drift_after_hash_verification() {
    let dir = temp_dir("yar-cli-locked-manifest-edge-drift");
    let cache_dir = dir.join("cache");
    let git = "https://example.com/parent.git";
    let commit = "1111111111111111111111111111111111111111";
    let dependency_dir = cache_path(&cache_dir, git, commit);
    fs::create_dir_all(&dependency_dir).unwrap();
    fs::write(
        dependency_dir.join("yar.toml"),
        r#"[package]
name = "parent"

[dependencies]
child = { git = "https://example.com/child.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(dependency_dir.join("parent.yar"), "package parent\n").unwrap();
    let hash = hash_dir(&dependency_dir).unwrap();

    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
parent = { git = "https://example.com/parent.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("yar.lock"),
        write_lock_file(&[locked_entry("parent", git, commit, hash)]),
    )
    .unwrap();
    fs::write(
        dir.join("main.yar"),
        "package main\n\nimport \"parent\"\n\nfn main() i32 { return 0 }\n",
    )
    .unwrap();

    let output = run_yar(&dir, &cache_dir, &["check", "."]);

    assert!(
        !output.status.success(),
        "stale dependency edges were accepted"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("manifest") && stderr.contains("run 'yar lock'"),
        "stderr: {stderr}"
    );
    assert!(
        !stderr.contains("hash mismatch"),
        "valid cached content was reported as corrupt: {stderr}"
    );
}

#[test]
fn check_rejects_path_dependencies_below_the_root_manifest() {
    let dir = temp_dir("yar-cli-nested-path-dependency");
    let cache_dir = dir.join("cache");
    let local = dir.join("local");
    fs::create_dir_all(&local).unwrap();
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
local = { path = "local" }
"#,
    )
    .unwrap();
    fs::write(
        local.join("yar.toml"),
        r#"[package]
name = "local"

[dependencies]
nested = { path = "../nested" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("main.yar"),
        "package main\n\nfn main() i32 { return 0 }\n",
    )
    .unwrap();

    let output = run_yar(&dir, &cache_dir, &["check", "."]);

    assert!(
        !output.status.success(),
        "nested path dependency was accepted"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("path dependencies are supported only in the root yar.toml"),
        "stderr: {stderr}"
    );
    assert!(!cache_dir.exists(), "check touched dependency cache");
}

#[test]
fn check_file_entry_resolves_a_sibling_path_dependency() {
    let dir = temp_dir("yar-cli-sibling-path-dependency");
    let project = dir.join("app");
    let dependency = dir.join("local");
    let cache_dir = dir.join("cache");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&dependency).unwrap();
    fs::write(
        project.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
local = { path = "../local" }
"#,
    )
    .unwrap();
    fs::write(
        project.join("main.yar"),
        r#"package main

import "local"

fn main() i32 {
    return local.value()
}
"#,
    )
    .unwrap();
    fs::write(
        dependency.join("yar.toml"),
        r#"[package]
name = "local"
"#,
    )
    .unwrap();
    fs::write(
        dependency.join("local.yar"),
        "package local\n\npub fn value() i32 { return 0 }\n",
    )
    .unwrap();

    let output = run_yar(&project, &cache_dir, &["check", "main.yar"]);

    assert!(
        output.status.success(),
        "sibling dependency failed for a file entry: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_rejects_a_missing_path_dependency_without_stdlib_substitution() {
    let dir = temp_dir("yar-cli-missing-path-dependency");
    let cache_dir = dir.join("cache");
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
fs = { path = "../missing" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("main.yar"),
        "package main\n\nimport \"fs\"\n\nfn main() i32 { return 0 }\n",
    )
    .unwrap();

    let output = run_yar(&dir, &cache_dir, &["check", "."]);

    assert!(
        !output.status.success(),
        "missing path dependency was replaced by stdlib"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("local dependency \"fs\"") && stderr.contains("does not exist"),
        "stderr: {stderr}"
    );
}

#[test]
fn check_does_not_load_stdlib_for_an_empty_path_dependency() {
    let dir = temp_dir("yar-cli-empty-path-dependency");
    let dependency = dir.join("local-fs");
    let cache_dir = dir.join("cache");
    fs::create_dir_all(&dependency).unwrap();
    fs::write(dependency.join("yar.toml"), "[package]\nname = \"fs\"\n").unwrap();
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
fs = { path = "local-fs" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("main.yar"),
        "package main\n\nimport \"fs\"\n\nfn main() i32 { return 0 }\n",
    )
    .unwrap();

    let output = run_yar(&dir, &cache_dir, &["check", "."]);

    assert!(
        !output.status.success(),
        "empty path dependency was accepted"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("has no .yar files") && stderr.contains("could not be loaded"),
        "declared path dependency was replaced by stdlib: {stderr}"
    );
}

#[test]
fn fetch_succeeds_without_a_lock_for_a_path_only_graph() {
    let dir = temp_dir("yar-cli-fetch-path-only");
    let dependency = dir.join("local");
    let cache_dir = dir.join("cache");
    fs::create_dir_all(&dependency).unwrap();
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
local = { path = "local" }
"#,
    )
    .unwrap();
    fs::write(dependency.join("yar.toml"), "[package]\nname = \"local\"\n").unwrap();

    let output = run_yar(&dir, &cache_dir, &["fetch"]);

    assert!(
        output.status.success(),
        "path-only fetch failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "all dependencies fetched\n"
    );
    assert!(!dir.join("yar.lock").exists());
    assert!(!cache_dir.exists(), "path-only fetch created a git cache");
}

#[test]
fn fetch_validates_an_existing_locked_dependency_cache() {
    let project = locked_dependency_project("yar-cli-fetch-tampered-cache");

    let valid = run_yar(&project.dir, &project.cache_dir, &["fetch"]);
    assert!(
        valid.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&valid.stderr)
    );
    assert!(
        String::from_utf8_lossy(&valid.stdout).contains("all dependencies fetched"),
        "fetch did not report success for a valid cache: {}",
        String::from_utf8_lossy(&valid.stdout)
    );

    fs::write(
        project.dependency_dir.join("dep.yar"),
        "package dep\n\npub fn value() i32 { return 99 }\n",
    )
    .unwrap();

    let output = run_yar(&project.dir, &project.cache_dir, &["fetch"]);

    assert!(
        !output.status.success(),
        "expected fetch to reject corruption"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("hash mismatch"),
        "stderr did not report cache corruption: {stderr}"
    );
    assert!(
        !stdout.contains("all dependencies fetched"),
        "fetch reported success for a corrupt cache: {stdout}"
    );
}

#[cfg(unix)]
#[test]
fn fetch_uses_the_locked_commit_instead_of_the_declared_ref() {
    let dir = temp_dir("yar-cli-fetch-locked-commit");
    let cache_dir = dir.join("cache");
    let expected_tree = dir.join("expected-child");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let git = "https://example.com/child.git";
    let commit = "2222222222222222222222222222222222222222";
    fs::create_dir_all(&expected_tree).unwrap();
    fs::write(expected_tree.join("child.yar"), "package child\n").unwrap();
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
child = { git = "https://example.com/child.git", tag = "moved-tag" }
"#,
    )
    .unwrap();
    let mut entry = locked_entry("child", git, commit, hash_dir(&expected_tree).unwrap());
    entry.tag = "moved-tag".to_owned();
    fs::write(dir.join("yar.lock"), write_lock_file(&[entry])).unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["fetch"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let git_log = fs::read_to_string(fake_git.join("git-args")).unwrap();
    assert!(
        git_log.contains(&format!("fetch --depth=1 --no-tags {git} {commit}")),
        "fetch did not request the locked commit:\n{git_log}"
    );
    assert!(
        !git_log.contains("moved-tag") && !git_log.contains("--branch"),
        "fetch followed mutable ref data from the lock:\n{git_log}"
    );
    assert!(cache_path(&cache_dir, git, commit).is_dir());
}

#[cfg(unix)]
#[test]
fn fetch_does_not_publish_an_unavailable_locked_commit() {
    let dir = temp_dir("yar-cli-fetch-unavailable-commit");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let git = "https://example.com/child.git";
    let commit = "9999999999999999999999999999999999999999";
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
child = { git = "https://example.com/child.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("yar.lock"),
        write_lock_file(&[locked_entry(
            "child",
            git,
            commit,
            format!("sha256:{}", "0".repeat(64)),
        )]),
    )
    .unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["fetch"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("git fetch locked commit {commit}"))
            && stderr.contains("is not available"),
        "stderr did not report the unavailable locked object: {stderr}"
    );
    assert!(
        !cache_path(&cache_dir, git, commit).exists(),
        "fetch published an unavailable locked commit"
    );
}

#[cfg(unix)]
#[test]
fn fetch_does_not_publish_when_checkout_head_differs_from_the_lock() {
    let dir = temp_dir("yar-cli-fetch-wrong-head");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let git = "https://example.com/wrong-head.git";
    let commit = "2222222222222222222222222222222222222222";
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
child = { git = "https://example.com/wrong-head.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("yar.lock"),
        write_lock_file(&[locked_entry(
            "child",
            git,
            commit,
            format!("sha256:{}", "0".repeat(64)),
        )]),
    )
    .unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["fetch"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("commit mismatch")
            && stderr.contains(commit)
            && stderr.contains("3333333333333333333333333333333333333333"),
        "stderr did not report the checkout mismatch: {stderr}"
    );
    assert!(
        !cache_path(&cache_dir, git, commit).exists(),
        "fetch published a checkout whose HEAD differed from the lock"
    );
}

#[cfg(unix)]
#[test]
fn fetch_does_not_publish_a_fresh_dependency_with_the_wrong_hash() {
    let dir = temp_dir("yar-cli-fetch-wrong-hash");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let git = "https://example.com/child.git";
    let commit = "2222222222222222222222222222222222222222";
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
child = { git = "https://example.com/child.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("yar.lock"),
        write_lock_file(&[locked_entry(
            "child",
            git,
            commit,
            format!("sha256:{}", "0".repeat(64)),
        )]),
    )
    .unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["fetch"]);

    assert!(
        !output.status.success(),
        "expected fetch to reject the hash"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("hash mismatch"),
        "stderr did not report the bad lock hash: {stderr}"
    );
    assert!(
        !stdout.contains("all dependencies fetched"),
        "fetch reported success for a dependency with the wrong hash: {stdout}"
    );
    assert!(
        !cache_path(&cache_dir, git, commit).exists(),
        "fetch published a dependency before verifying its lock hash"
    );
}

#[cfg(unix)]
#[test]
fn fetch_does_not_publish_a_fresh_dependency_with_stale_edges() {
    let dir = temp_dir("yar-cli-fetch-stale-edges");
    let cache_dir = dir.join("cache");
    let expected_tree = dir.join("expected-parent");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let git = "https://example.com/parent.git";
    let commit = "1111111111111111111111111111111111111111";
    fs::create_dir_all(&expected_tree).unwrap();
    fs::write(
        expected_tree.join("yar.toml"),
        r#"[package]
name = "parent"

[dependencies]
child = { git = "https://example.com/child.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(expected_tree.join("parent.yar"), "package parent\n").unwrap();
    let hash = hash_dir(&expected_tree).unwrap();

    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
parent = { git = "https://example.com/parent.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("yar.lock"),
        write_lock_file(&[locked_entry("parent", git, commit, hash)]),
    )
    .unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["fetch"]);

    assert!(
        !output.status.success(),
        "stale dependency edges were fetched"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("manifest") && stderr.contains("run 'yar lock'"),
        "stderr: {stderr}"
    );
    assert!(
        !cache_path(&cache_dir, git, commit).exists(),
        "fetch published a dependency before verifying its manifest edges"
    );
}

#[cfg(unix)]
#[test]
fn lock_does_not_bless_a_corrupt_preexisting_dependency_cache() {
    let (dir, cache_dir, path) = corrupt_parent_cache("yar-cli-lock-corrupt-cache");
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
parent = { git = "https://example.com/parent.git", tag = "v1" }
"#,
    )
    .unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["lock"]);

    assert!(
        !output.status.success(),
        "yar lock accepted a cache that differed from the fresh checkout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("hash mismatch"),
        "stderr did not report cache corruption: {stderr}"
    );
    assert!(
        !dir.join("yar.lock").exists(),
        "yar lock wrote a hash for corrupt cached content"
    );
}

#[cfg(unix)]
#[test]
fn add_git_dependency_resolves_transitive_lock_and_fetches_without_network() {
    let dir = temp_dir("yar-cli-git-dep");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args([
            "add",
            "parent",
            "https://example.com/parent.git",
            "--tag=v1",
        ])
        .current_dir(&dir)
        .env("PATH", &path)
        .env("YAR_CACHE", &cache_dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "added dependency \"parent\"\nyar.lock updated\n"
    );

    let manifest = parse_manifest(&fs::read_to_string(dir.join("yar.toml")).unwrap()).unwrap();
    assert_eq!(
        manifest.dependencies["parent"].git,
        "https://example.com/parent.git"
    );
    let lock_content = fs::read_to_string(dir.join("yar.lock")).unwrap();
    let lock_entries = parse_lock_file(&lock_content).unwrap();
    let names = lock_entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["child", "parent"]);
    let parent = lock_entries
        .iter()
        .find(|entry| entry.name == "parent")
        .expect("parent lock entry");
    assert_eq!(
        parent
            .dependencies
            .iter()
            .map(|dependency| dependency.name.as_str())
            .collect::<Vec<_>>(),
        ["child"]
    );
    assert_eq!(parent.dependencies[0].tag, "v1");
    for entry in &lock_entries {
        assert!(cache_path(&cache_dir, &entry.git, &entry.commit).is_dir());
    }

    fs::remove_dir_all(&cache_dir).unwrap();
    let fetch = Command::new(env!("CARGO_BIN_EXE_yar"))
        .arg("fetch")
        .current_dir(&dir)
        .env("PATH", &path)
        .env("YAR_CACHE", &cache_dir)
        .output()
        .unwrap();

    assert!(
        fetch.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&fetch.stderr)
    );
    let stdout = String::from_utf8_lossy(&fetch.stdout);
    assert!(stdout.contains("fetching child..."));
    assert!(stdout.contains("fetching parent..."));
    assert!(stdout.contains("all dependencies fetched"));
    for entry in &lock_entries {
        assert!(cache_path(&cache_dir, &entry.git, &entry.commit).is_dir());
    }
}

#[cfg(unix)]
#[test]
fn remove_git_dependency_rebuilds_only_the_reachable_lock_graph() {
    let dir = temp_dir("yar-cli-remove-git-closure");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
parent = { git = "https://example.com/parent.git", tag = "v1" }
sibling = { git = "https://example.com/sibling.git", tag = "v1" }
"#,
    )
    .unwrap();
    let child_source = Dependency {
        git: "https://example.com/child.git".to_string(),
        tag: "v1".to_string(),
        ..Dependency::default()
    };
    let mut parent = locked_entry(
        "parent",
        "https://example.com/parent.git",
        "1111111111111111111111111111111111111111",
        format!("sha256:{}", "a".repeat(64)),
    );
    parent.dependencies = vec![LockDependency::new("child", &child_source)];
    fs::write(
        dir.join("yar.lock"),
        write_lock_file(&[
            locked_entry(
                "child",
                "https://example.com/child.git",
                "2222222222222222222222222222222222222222",
                format!("sha256:{}", "b".repeat(64)),
            ),
            parent,
            locked_entry(
                "sibling",
                "https://example.com/sibling.git",
                "2222222222222222222222222222222222222222",
                format!("sha256:{}", "c".repeat(64)),
            ),
        ]),
    )
    .unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["remove", "parent"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "removed dependency \"parent\"\nyar.lock updated\n"
    );
    let manifest = parse_manifest(&fs::read_to_string(dir.join("yar.toml")).unwrap()).unwrap();
    assert_eq!(
        manifest
            .dependencies
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["sibling"]
    );
    let entries = parse_lock_file(&fs::read_to_string(dir.join("yar.lock")).unwrap()).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["sibling"]
    );
}

#[cfg(unix)]
#[test]
fn remove_git_dependency_preserves_a_shared_transitive_dependency() {
    let dir = temp_dir("yar-cli-remove-shared-git-dependency");
    let local = dir.join("local");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    fs::create_dir(&local).unwrap();
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
local = { path = "local" }
parent = { git = "https://example.com/parent.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(
        local.join("yar.toml"),
        r#"[package]
name = "local"

[dependencies]
child = { git = "https://example.com/child.git", tag = "v1" }
"#,
    )
    .unwrap();
    let child_source = Dependency {
        git: "https://example.com/child.git".to_string(),
        tag: "v1".to_string(),
        ..Dependency::default()
    };
    let mut parent = locked_entry(
        "parent",
        "https://example.com/parent.git",
        "1111111111111111111111111111111111111111",
        format!("sha256:{}", "a".repeat(64)),
    );
    parent.dependencies = vec![LockDependency::new("child", &child_source)];
    fs::write(
        dir.join("yar.lock"),
        write_lock_file(&[
            locked_entry(
                "child",
                "https://example.com/child.git",
                "2222222222222222222222222222222222222222",
                format!("sha256:{}", "b".repeat(64)),
            ),
            parent,
        ]),
    )
    .unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["remove", "parent"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "removed dependency \"parent\"\nyar.lock updated\n"
    );
    let manifest = parse_manifest(&fs::read_to_string(dir.join("yar.toml")).unwrap()).unwrap();
    assert_eq!(
        manifest
            .dependencies
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["local"]
    );
    let entries = parse_lock_file(&fs::read_to_string(dir.join("yar.lock")).unwrap()).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["child"]
    );
}

#[cfg(unix)]
#[test]
fn lock_follows_git_dependencies_below_a_live_local_root() {
    let dir = temp_dir("yar-cli-lock-local-root");
    let local = dir.join("local-lib");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    fs::create_dir_all(&local).unwrap();
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
local_lib = { path = "local-lib" }
"#,
    )
    .unwrap();
    let original_manifest = fs::read_to_string(dir.join("yar.toml")).unwrap();
    fs::write(
        local.join("yar.toml"),
        r#"[package]
name = "local_lib"

[dependencies]
child = { git = "https://example.com/child.git", tag = "v1" }
"#,
    )
    .unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["lock"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let entries = parse_lock_file(&fs::read_to_string(dir.join("yar.lock")).unwrap()).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["child"]
    );
    assert_eq!(
        fs::read_to_string(dir.join("yar.toml")).unwrap(),
        original_manifest
    );
}

#[cfg(unix)]
#[test]
fn legacy_lock_is_rejected_before_fetch_and_regenerated_by_full_lock() {
    let dir = temp_dir("yar-cli-fetch-legacy-lock");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
child = { git = "https://example.com/child.git", tag = "v1" }
"#,
    )
    .unwrap();
    let legacy_lock = r#"[[package]]
name = "child"
git = "https://example.com/child.git"
tag = "v1"
commit = "2222222222222222222222222222222222222222"
hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
"#;
    fs::write(dir.join("yar.lock"), legacy_lock).unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["fetch"]);

    assert!(!output.status.success(), "legacy lock was fetched");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("missing version") && stderr.contains("run 'yar lock'"),
        "stderr: {stderr}"
    );
    assert!(!cache_dir.exists(), "fetch touched cache before validation");

    let selective = run_yar_with_path(&dir, &cache_dir, &path, &["update", "child"]);
    assert!(
        !selective.status.success(),
        "selective update accepted a legacy lock"
    );
    let stderr = String::from_utf8_lossy(&selective.stderr);
    assert!(
        stderr.contains("invalid or legacy") && stderr.contains("run 'yar lock'"),
        "stderr: {stderr}"
    );
    assert!(
        !cache_dir.exists(),
        "selective update fetched before validation"
    );
    assert_eq!(
        fs::read_to_string(dir.join("yar.lock")).unwrap(),
        legacy_lock
    );

    let lock = run_yar_with_path(&dir, &cache_dir, &path, &["lock"]);
    assert!(
        lock.status.success(),
        "full lock regeneration failed: {}",
        String::from_utf8_lossy(&lock.stderr)
    );
    let content = fs::read_to_string(dir.join("yar.lock")).unwrap();
    assert!(content.starts_with("# Auto-generated by yar. Do not edit.\n\nversion = 1\n"));
    let entries = parse_lock_file(&content).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "child");
}

#[cfg(unix)]
#[test]
fn fetch_rejects_unreachable_lock_entries_before_invoking_git() {
    let dir = temp_dir("yar-cli-fetch-unreachable-lock");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
child = { git = "https://example.com/child.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("yar.lock"),
        write_lock_file(&[
            locked_entry(
                "child",
                "https://example.com/child.git",
                "2222222222222222222222222222222222222222",
                format!("sha256:{}", "a".repeat(64)),
            ),
            locked_entry(
                "injected",
                "https://example.com/injected.git",
                "3333333333333333333333333333333333333333",
                format!("sha256:{}", "b".repeat(64)),
            ),
        ]),
    )
    .unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["fetch"]);

    assert!(
        !output.status.success(),
        "unreachable lock entry was fetched"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unreachable package entries") && stderr.contains("injected"),
        "stderr: {stderr}"
    );
    assert!(!cache_dir.exists(), "fetch touched cache before validation");
}

#[cfg(unix)]
#[test]
fn update_one_git_dependency_preserves_unrelated_lock_entries() {
    let dir = temp_dir("yar-cli-update-one-dep");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
parent = { git = "https://example.com/parent.git", tag = "v1" }
sibling = { git = "https://example.com/sibling.git", tag = "v1" }
"#,
    )
    .unwrap();
    let original_manifest = fs::read_to_string(dir.join("yar.toml")).unwrap();
    fs::write(
        dir.join("yar.lock"),
        r#"# Auto-generated by yar. Do not edit.

version = 1

[[package]]
name = "old_child"
git = "https://example.com/old-child.git"
tag = "v0"
commit = "0000000000000000000000000000000000000000"
hash = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"

[[package]]
name = "parent"
git = "https://example.com/parent.git"
tag = "v0"
commit = "0000000000000000000000000000000000000000"
hash = "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"

[[package.dependencies]]
name = "old_child"
git = "https://example.com/old-child.git"
tag = "v0"

[[package]]
name = "sibling"
git = "https://example.com/sibling.git"
tag = "v1"
commit = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
hash = "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["update", "parent"])
        .current_dir(&dir)
        .env("PATH", &path)
        .env("YAR_CACHE", &cache_dir)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "yar.lock updated\n"
    );

    let lock_content = fs::read_to_string(dir.join("yar.lock")).unwrap();
    let lock_entries = parse_lock_file(&lock_content).unwrap();
    let entry = |name: &str| {
        lock_entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| panic!("missing lock entry {name:?}: {lock_entries:?}"))
    };
    assert_eq!(
        entry("parent").commit,
        "1111111111111111111111111111111111111111"
    );
    assert_eq!(
        entry("child").commit,
        "2222222222222222222222222222222222222222"
    );
    assert!(
        lock_entries.iter().all(|entry| entry.name != "old_child"),
        "orphaned selected descendant was preserved: {lock_entries:?}"
    );
    assert_eq!(
        entry("sibling").commit,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(
        entry("sibling").hash,
        "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    );
    assert_eq!(
        fs::read_to_string(dir.join("yar.toml")).unwrap(),
        original_manifest
    );
}

#[cfg(unix)]
#[test]
fn update_one_rejects_stale_unselected_roots_before_fetching() {
    let dir = temp_dir("yar-cli-update-stale-unselected");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
parent = { git = "https://example.com/parent.git", tag = "v1" }
sibling = { git = "https://example.com/sibling.git", tag = "v2" }
"#,
    )
    .unwrap();
    let original = write_lock_file(&[
        locked_entry(
            "parent",
            "https://example.com/parent.git",
            "0000000000000000000000000000000000000000",
            format!("sha256:{}", "a".repeat(64)),
        ),
        locked_entry(
            "sibling",
            "https://example.com/sibling.git",
            "0000000000000000000000000000000000000000",
            format!("sha256:{}", "b".repeat(64)),
        ),
    ]);
    fs::write(dir.join("yar.lock"), &original).unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["update", "parent"]);

    assert!(
        !output.status.success(),
        "stale unselected root was accepted"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("sibling") && stderr.contains("run 'yar lock'"),
        "stderr: {stderr}"
    );
    assert!(
        !cache_dir.exists(),
        "update fetched before graph validation"
    );
    assert_eq!(fs::read_to_string(dir.join("yar.lock")).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn update_one_rejects_a_shared_source_conflict_before_fetching() {
    let dir = temp_dir("yar-cli-update-shared-source-conflict");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
parent = { git = "https://example.com/parent.git", tag = "v2" }
sibling = { git = "https://example.com/sibling.git", tag = "v1" }
"#,
    )
    .unwrap();

    let parent_source = Dependency {
        git: "https://example.com/parent.git".to_string(),
        tag: "v1".to_string(),
        ..Dependency::default()
    };
    let mut sibling = locked_entry(
        "sibling",
        "https://example.com/sibling.git",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        format!("sha256:{}", "b".repeat(64)),
    );
    sibling.dependencies = vec![LockDependency::new("parent", &parent_source)];
    let original = write_lock_file(&[
        locked_entry(
            "parent",
            "https://example.com/parent.git",
            "1111111111111111111111111111111111111111",
            format!("sha256:{}", "a".repeat(64)),
        ),
        sibling,
    ]);
    fs::write(dir.join("yar.lock"), &original).unwrap();

    let output = run_yar_with_path(&dir, &cache_dir, &path, &["update", "parent"]);

    assert!(
        !output.status.success(),
        "shared source conflict was accepted"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("dependency conflict for \"parent\"")
            && stderr.contains("unselected root")
            && stderr.contains("run 'yar lock'"),
        "stderr: {stderr}"
    );
    assert!(
        !cache_dir.exists(),
        "update fetched before detecting the shared source conflict"
    );
    assert_eq!(fs::read_to_string(dir.join("yar.lock")).unwrap(), original);
}

#[cfg(unix)]
#[test]
fn update_one_local_dependency_requires_full_lock_regeneration() {
    let dir = temp_dir("yar-cli-update-one-local-dep");
    let cache_dir = dir.join("cache");
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    fs::create_dir_all(dir.join("local-lib")).unwrap();
    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
local_lib = { path = "local-lib" }
sibling = { git = "https://example.com/sibling.git", tag = "v1" }
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(["update", "local_lib"])
        .current_dir(&dir)
        .env("PATH", &path)
        .env("YAR_CACHE", &cache_dir)
        .output()
        .unwrap();

    assert!(!output.status.success(), "targeted path update succeeded");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("run 'yar lock'"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !dir.join("yar.lock").exists(),
        "selected local update should not create a lock for unrelated git dependencies"
    );
    assert!(
        !cache_dir.exists(),
        "selected local update should not fetch unrelated git dependencies"
    );
}

fn fixture(path: &str) -> PathBuf {
    repo_root().join(path)
}

fn repo_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop();
    dir.pop();
    dir
}

fn temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct LockedDependencyProject {
    dir: PathBuf,
    cache_dir: PathBuf,
    dependency_dir: PathBuf,
}

fn locked_dependency_project(prefix: &str) -> LockedDependencyProject {
    let dir = temp_dir(prefix);
    let cache_dir = dir.join("cache");
    let git = "https://example.com/dep.git";
    let commit = "3333333333333333333333333333333333333333";
    let dependency_dir = cache_path(&cache_dir, git, commit);
    fs::create_dir_all(&dependency_dir).unwrap();
    fs::write(
        dependency_dir.join("dep.yar"),
        "package dep\n\npub fn value() i32 { return 42 }\n",
    )
    .unwrap();
    let hash = hash_dir(&dependency_dir).unwrap();

    fs::write(
        dir.join("yar.toml"),
        r#"[package]
name = "app"

[dependencies]
dep = { git = "https://example.com/dep.git", tag = "v1" }
"#,
    )
    .unwrap();
    fs::write(
        dir.join("yar.lock"),
        write_lock_file(&[locked_entry("dep", git, commit, hash)]),
    )
    .unwrap();
    fs::write(
        dir.join("main.yar"),
        r#"package main

import "dep"

fn main() i32 {
    return dep.value()
}
"#,
    )
    .unwrap();

    LockedDependencyProject {
        dir,
        cache_dir,
        dependency_dir,
    }
}

fn locked_entry(name: &str, git: &str, commit: &str, hash: String) -> LockEntry {
    LockEntry {
        name: name.to_string(),
        git: git.to_string(),
        tag: "v1".to_string(),
        commit: commit.to_string(),
        hash,
        ..LockEntry::default()
    }
}

fn run_yar(dir: &Path, cache_dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(args)
        .current_dir(dir)
        .env("YAR_CACHE", cache_dir)
        .output()
        .unwrap()
}

#[cfg(unix)]
fn run_yar_with_path(
    dir: &Path,
    cache_dir: &Path,
    path: &str,
    args: &[&str],
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_yar"))
        .args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env("YAR_CACHE", cache_dir)
        .output()
        .unwrap()
}

#[cfg(unix)]
fn corrupt_parent_cache(prefix: &str) -> (PathBuf, PathBuf, String) {
    let dir = temp_dir(prefix);
    let cache_dir = dir.join("cache");
    let git = "https://example.com/parent.git";
    let commit = "1111111111111111111111111111111111111111";
    let dependency_dir = cache_path(&cache_dir, git, commit);
    fs::create_dir_all(&dependency_dir).unwrap();
    fs::write(
        dependency_dir.join("parent.yar"),
        "package parent\n\npub fn compromised() bool { return true }\n",
    )
    .unwrap();
    let fake_git = fake_git_dir();
    let path = format!(
        "{}:{}",
        fake_git.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    (dir, cache_dir, path)
}

#[cfg(unix)]
fn non_host_unix_target() -> (&'static str, &'static str, &'static str) {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => ("darwin", "arm64", "aarch64-apple-darwin"),
        ("linux", "aarch64") => ("darwin", "amd64", "x86_64-apple-darwin"),
        ("macos", "x86_64") => ("linux", "arm64", "aarch64-unknown-linux-gnu"),
        ("macos", "aarch64") => ("linux", "amd64", "x86_64-unknown-linux-gnu"),
        _ => ("linux", "amd64", "x86_64-unknown-linux-gnu"),
    }
}

#[cfg(unix)]
fn write_executable(path: &std::path::Path, content: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, content).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn build_runtime_archive(dir: &std::path::Path) -> PathBuf {
    let target_dir = dir.join("cargo-target");
    let output = Command::new("cargo")
        .args([
            "build",
            "-p",
            "yar-runtime",
            "--release",
            "--target-dir",
            target_dir.to_str().unwrap(),
        ])
        .current_dir(repo_root())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let archive_name = if cfg!(windows) {
        "yar_runtime.lib"
    } else {
        "libyar_runtime.a"
    };
    let archive = target_dir.join("release").join(archive_name);
    assert!(archive.is_file(), "missing {}", archive.display());
    archive
}

#[cfg(unix)]
fn fake_cc(dir: &std::path::Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let script = dir.join("cc");
    let log = dir.join("cc-args");
    fs::write(
        &script,
        format!(
            r#"#!/bin/sh
set -eu
printf '%s\n' "$*" > '{}'
out=""
previous=""
for arg in "$@"; do
  if [ "$previous" = "-o" ]; then
    out="$arg"
  fi
  previous="$arg"
done
if [ -z "$out" ]; then
  echo "missing -o" >&2
  exit 1
fi
: > "$out"
"#,
            log.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).unwrap();
    script
}

#[cfg(unix)]
fn fake_git_dir() -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let dir = temp_dir("yar-fake-git");
    let script = dir.join("git");
    fs::write(
        &script,
        r#"#!/bin/sh
set -eu

log="$(dirname "$0")/git-args"
printf '%s\n' "$*" >> "$log"

materialize() {
  dest="$1"
  url="$2"
  mkdir -p "$dest/.git"
  if echo "$url" | grep -q parent; then
    cat > "$dest/yar.toml" <<'EOF'
[package]
name = "parent"

[dependencies]
child = { git = "https://example.com/child.git", tag = "v1" }
EOF
    printf 'package parent\n' > "$dest/parent.yar"
    head="1111111111111111111111111111111111111111"
  else
    printf 'package child\n' > "$dest/child.yar"
    if echo "$url" | grep -q wrong-head; then
      head="3333333333333333333333333333333333333333"
    else
      head="2222222222222222222222222222222222222222"
    fi
  fi
  printf '%s\n' "$head" > "$dest/.git/fake-head"
}

if [ "$1" = "clone" ]; then
  url=""
  dest=""
  for arg in "$@"; do
    case "$arg" in
      https://*) url="$arg" ;;
    esac
    dest="$arg"
  done
  materialize "$dest" "$url"
  exit 0
fi

if [ "$1" = "init" ]; then
  if [ "$#" -ne 2 ]; then
    echo "unexpected git init invocation: $*" >&2
    exit 1
  fi
  dest=""
  for arg in "$@"; do
    dest="$arg"
  done
  mkdir -p "$dest/.git"
  : > "$dest/.git/fake-initialized"
  exit 0
fi

if [ "$1" = "-C" ] && [ "$3" = "fetch" ]; then
  if [ "$#" -ne 7 ] || [ "$4" != "--depth=1" ] || [ "$5" != "--no-tags" ]; then
    echo "unexpected locked fetch invocation: $*" >&2
    exit 1
  fi
  if [ ! -f "$2/.git/fake-initialized" ]; then
    echo "fetch destination was not initialized" >&2
    exit 1
  fi
  url="$6"
  requested="$7"
  expected="2222222222222222222222222222222222222222"
  if echo "$url" | grep -q parent; then
    expected="1111111111111111111111111111111111111111"
  fi
  if [ "$requested" != "$expected" ]; then
    echo "locked commit $requested is not available" >&2
    exit 1
  fi
  printf '%s\n' "$url" > "$2/.git/fake-fetched-url"
  printf '%s\n' "$requested" > "$2/.git/fake-fetched-commit"
  exit 0
fi

if [ "$1" = "-C" ] && [ "$3" = "rev-parse" ]; then
  if [ "$#" -ne 4 ] || [ "$4" != "HEAD" ] || [ ! -f "$2/.git/fake-head" ]; then
    echo "HEAD is not checked out: $*" >&2
    exit 1
  fi
  cat "$2/.git/fake-head"
  exit 0
fi

if [ "$1" = "-C" ] && [ "$3" = "checkout" ]; then
  if [ -f "$2/.git/fake-fetched-commit" ]; then
    fetched="$(cat "$2/.git/fake-fetched-commit")"
    if [ "$#" -ne 5 ] || [ "$4" != "--detach" ] || [ "$5" != "$fetched" ]; then
      echo "checkout did not detach the fetched commit: $*" >&2
      exit 1
    fi
    url="$(cat "$2/.git/fake-fetched-url")"
    materialize "$2" "$url"
  elif [ "$#" -eq 4 ]; then
    printf '%s\n' "$4" > "$2/.git/fake-head"
  else
    echo "unexpected git checkout invocation: $*" >&2
    exit 1
  fi
  exit 0
fi

echo "unexpected git invocation: $*" >&2
exit 1
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&script, permissions).unwrap();
    dir
}
