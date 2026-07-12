use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use yar_compiler::{
    compile::{CompileOptions, Unit, compile_path},
    manifest::{LockDependency, LockEntry, write_lock_file},
};

static NEXT_PROJECT_ID: AtomicU64 = AtomicU64::new(0);
const COMPILE_PROBE_ENTRY: &str = "YAR_PACKAGE_IDENTITY_PROBE_ENTRY";

#[test]
fn path_dependency_cannot_import_undeclared_root_sibling() {
    let fixture = TestProject::new("undeclared-sibling");
    fixture.write(
        "project/yar.toml",
        r#"[package]
name = "app"

[dependencies]
alpha = { path = "../alpha" }
beta = { path = "../beta" }
"#,
    );
    fixture.write(
        "project/main.yar",
        r#"package main

import "alpha"

fn main() i32 {
    return alpha.value()
}
"#,
    );
    fixture.write("alpha/yar.toml", "[package]\nname = \"alpha\"\n");
    fixture.write(
        "alpha/alpha.yar",
        r#"package alpha

import "beta"

pub fn value() i32 {
    return beta.value()
}
"#,
    );
    fixture.write("beta/yar.toml", "[package]\nname = \"beta\"\n");
    fixture.write(
        "beta/beta.yar",
        r#"package beta

pub fn value() i32 {
    return 0
}
"#,
    );

    let failure = compile_rejection(&fixture.path("project/main.yar"));

    assert!(
        failure.contains("beta") && failure.contains("not declared"),
        "unexpected rejection: {failure}"
    );
}

#[test]
fn root_cannot_import_transitive_alias_before_cache_verification() {
    let fixture = TestProject::new("transitive-alias-visibility");
    let parent_git = "https://example.com/parent.git";
    let child_git = "https://example.com/child.git";
    fixture.write(
        "project/yar.toml",
        r#"[package]
name = "app"

[dependencies]
parent = { git = "https://example.com/parent.git", tag = "v1" }
"#,
    );
    fixture.write(
        "project/main.yar",
        r#"package main

import "child"

fn main() i32 {
    return child.value()
}
"#,
    );

    let mut parent = locked_entry(
        "parent",
        parent_git,
        "1111111111111111111111111111111111111111",
        'a',
    );
    parent.dependencies.push(LockDependency {
        name: "child".to_owned(),
        git: child_git.to_owned(),
        tag: "v1".to_owned(),
        ..LockDependency::default()
    });
    let child = locked_entry(
        "child",
        child_git,
        "2222222222222222222222222222222222222222",
        'b',
    );
    fixture.write("project/yar.lock", &write_lock_file(&[parent, child]));

    let cache = fixture.path("cache");
    let failure = compile_rejection_in_isolated_process(&fixture.path("project/main.yar"), &cache);

    assert!(
        failure.contains("child") && failure.contains("not declared"),
        "unexpected rejection: {failure}"
    );
    assert!(
        !failure.contains("cache") && !failure.contains("hash mismatch"),
        "visibility rejection inspected the dependency cache: {failure}"
    );
    assert!(!cache.exists(), "visibility rejection created a cache");
}

#[test]
fn bare_stdlib_migration_precedes_an_undeclared_transitive_alias() {
    let fixture = TestProject::new("stdlib-migration-transitive-alias");
    let parent_git = "https://example.com/parent.git";
    let strings_git = "https://example.com/strings.git";
    fixture.write(
        "project/yar.toml",
        r#"[package]
name = "app"

[dependencies]
parent = { git = "https://example.com/parent.git", tag = "v1" }
"#,
    );
    fixture.write(
        "project/main.yar",
        r#"package main

import "strings"

fn main() i32 {
    return 0
}
"#,
    );

    let mut parent = locked_entry(
        "parent",
        parent_git,
        "1111111111111111111111111111111111111111",
        'a',
    );
    parent.dependencies.push(LockDependency {
        name: "strings".to_owned(),
        git: strings_git.to_owned(),
        tag: "v1".to_owned(),
        ..LockDependency::default()
    });
    let strings = locked_entry(
        "strings",
        strings_git,
        "2222222222222222222222222222222222222222",
        'b',
    );
    fixture.write("project/yar.lock", &write_lock_file(&[parent, strings]));

    let cache = fixture.path("cache");
    let failure = compile_rejection_in_isolated_process(&fixture.path("project/main.yar"), &cache);

    assert!(
        failure
            .contains("standard library package \"strings\" must be imported as \"std/strings\""),
        "unexpected rejection: {failure}"
    );
    assert!(
        !failure.contains("not declared")
            && !failure.contains("cache")
            && !failure.contains("hash mismatch"),
        "migration diagnostic inspected or exposed the transitive dependency: {failure}"
    );
    assert!(!cache.exists(), "migration diagnostic created a cache");
}

#[test]
fn same_git_commit_with_conflicting_hashes_fails_package_index_construction() {
    let fixture = TestProject::new("conflicting-git-source-hashes");
    let git = "https://example.com/shared.git";
    let commit = "3333333333333333333333333333333333333333";
    fixture.write(
        "project/yar.toml",
        r#"[package]
name = "app"

[dependencies]
first = { git = "https://example.com/shared.git", tag = "v1" }
second = { git = "https://example.com/shared.git", tag = "v1" }
"#,
    );
    fixture.write(
        "project/main.yar",
        r#"package main

fn main() i32 {
    return 0
}
"#,
    );
    fixture.write(
        "project/yar.lock",
        &write_lock_file(&[
            locked_entry("first", git, commit, 'a'),
            locked_entry("second", git, commit, 'b'),
        ]),
    );

    let cache = fixture.path("cache");
    let failure = compile_rejection_in_isolated_process(&fixture.path("project/main.yar"), &cache);

    assert!(
        failure.contains("shared.git")
            && failure.contains(commit)
            && failure.contains("conflicting hashes"),
        "unexpected rejection: {failure}"
    );
    assert!(!cache.exists(), "package-index rejection created a cache");
}

#[test]
fn dependency_self_import_cannot_be_hijacked_by_root_package() {
    let fixture = TestProject::new("same-origin-self-import");
    fixture.write(
        "project/yar.toml",
        r#"[package]
name = "app"

[dependencies]
vendor = { path = "../vendor" }
"#,
    );
    fixture.write(
        "project/main.yar",
        r#"package main

import "vendor/api"
import "vendor/internal"

fn main() i32 {
    return api.dependency_value() + internal.root_value()
}
"#,
    );
    fixture.write(
        "project/vendor/internal/internal.yar",
        r#"package internal

pub fn root_value() i32 {
    return 0
}
"#,
    );
    fixture.write("vendor/yar.toml", "[package]\nname = \"vendor\"\n");
    fixture.write(
        "vendor/api/api.yar",
        r#"package api

import "vendor/internal"

pub fn dependency_value() i32 {
    return internal.dependency_value()
}
"#,
    );
    fixture.write(
        "vendor/internal/internal.yar",
        r#"package internal

pub fn dependency_value() i32 {
    return 0
}
"#,
    );

    compile_success(&fixture.path("project/main.yar"));
}

#[test]
fn stdlib_internal_import_coexists_with_root_shadow() {
    let fixture = TestProject::new("stdlib-owner-scope");
    fixture.write(
        "main.yar",
        r#"package main

import "std/io"
import "conv"

fn main() i32 {
    return conv.local_value()
}
"#,
    );
    fixture.write(
        "conv/conv.yar",
        r#"package conv

pub fn local_value() i32 {
    return 0
}
"#,
    );

    compile_success(&fixture.path("main.yar"));
}

#[test]
fn duplicate_final_segment_import_qualifiers_are_rejected() {
    let fixture = TestProject::new("duplicate-qualifier");
    fixture.write(
        "main.yar",
        r#"package main

import "a/shared"
import "b/shared"

fn main() i32 {
    return shared.value()
}
"#,
    );
    fixture.write(
        "a/shared/shared.yar",
        r#"package shared

pub fn value() i32 {
    return 1
}
"#,
    );
    fixture.write(
        "b/shared/shared.yar",
        r#"package shared

pub fn value() i32 {
    return 2
}
"#,
    );

    let failure = compile_rejection(&fixture.path("main.yar"));

    assert!(
        failure.contains("import qualifier \"shared\"") && failure.contains("ambiguous"),
        "unexpected rejection: {failure}"
    );
}

fn compile_success(entry: &Path) -> Unit {
    let (unit, diagnostics) = compile_path(entry, &CompileOptions::default())
        .unwrap_or_else(|error| panic!("compile {}: {error}", entry.display()));
    assert!(
        diagnostics.is_empty(),
        "compile {}: {}",
        entry.display(),
        diagnostic_messages(&diagnostics)
    );
    unit.unwrap_or_else(|| panic!("compile {} produced no unit", entry.display()))
}

fn compile_rejection(entry: &Path) -> String {
    match compile_path(entry, &CompileOptions::default()) {
        Err(error) => error.to_string(),
        Ok((None, diagnostics)) => diagnostic_messages(&diagnostics),
        Ok((Some(_), diagnostics)) => panic!(
            "compile {} unexpectedly succeeded: {}",
            entry.display(),
            diagnostic_messages(&diagnostics)
        ),
    }
}

fn diagnostic_messages(diagnostics: &[yar_compiler::diag::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn compile_rejection_in_isolated_process(entry: &Path, cache: &Path) -> String {
    let output = Command::new(std::env::current_exe().expect("resolve test executable"))
        .args([
            "--ignored",
            "--exact",
            "compile_rejection_probe",
            "--nocapture",
        ])
        .env(COMPILE_PROBE_ENTRY, entry)
        .env("YAR_CACHE", cache)
        .output()
        .expect("run isolated compile probe");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "isolated compile probe failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    format!("{stdout}\n{stderr}")
}

#[test]
#[ignore = "subprocess entrypoint for cache-isolated compilation"]
fn compile_rejection_probe() {
    let entry = std::env::var_os(COMPILE_PROBE_ENTRY)
        .map(PathBuf::from)
        .expect("compile probe entry path");
    println!("{}", compile_rejection(&entry));
}

fn locked_entry(name: &str, git: &str, commit: &str, hash_digit: char) -> LockEntry {
    LockEntry {
        name: name.to_owned(),
        git: git.to_owned(),
        tag: "v1".to_owned(),
        commit: commit.to_owned(),
        hash: format!("sha256:{}", hash_digit.to_string().repeat(64)),
        ..LockEntry::default()
    }
}

struct TestProject {
    root: PathBuf,
}

impl TestProject {
    fn new(name: &str) -> Self {
        loop {
            let id = NEXT_PROJECT_ID.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "yar-package-identity-{name}-{}-{id}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Ok(()) => return Self { root },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create {}: {error}", root.display()),
            }
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("create {}: {error}", parent.display()));
        }
        fs::write(&path, contents)
            .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
