use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

use crate::{
    ast::{FunctionDecl, PackageGraph, PackageImport, Program, SourceId, TypeRefKind},
    checker::{Info, check_program},
    codegen::{CodegenError, emit_llvm_with_options},
    diag::Diagnostic,
    lower::lower_package_graph,
    mono::monomorphize_program,
    package::{LoadError, load_package_graph_with_root},
    parser::parse_file,
};

const TESTING_IMPORT_PATH: &str = "std/testing";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompileOptions {
    pub target_triple: Option<String>,
    pub project_root: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CheckedProgram {
    program: Program,
    info: Info,
}

impl CheckedProgram {
    pub fn emit_llvm(&self, target_triple: Option<&str>) -> Result<Unit, CodegenError> {
        let ir = emit_llvm_with_options(&self.program, &self.info, target_triple)?;
        Ok(Unit { ir })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unit {
    pub ir: String,
}

#[derive(Debug)]
pub enum CompileError {
    Load(LoadError),
    Codegen(CodegenError),
    Internal(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::Load(err) => write!(f, "{err}"),
            CompileError::Codegen(err) => write!(f, "{err}"),
            CompileError::Internal(message) => f.write_str(message),
        }
    }
}

impl Error for CompileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CompileError::Load(err) => Some(err),
            CompileError::Codegen(err) => Some(err),
            CompileError::Internal(_) => None,
        }
    }
}

impl From<LoadError> for CompileError {
    fn from(value: LoadError) -> Self {
        CompileError::Load(value)
    }
}

impl From<CodegenError> for CompileError {
    fn from(value: CodegenError) -> Self {
        CompileError::Codegen(value)
    }
}

pub fn compile_path(
    path: impl AsRef<Path>,
    options: &CompileOptions,
) -> Result<(Option<Unit>, Vec<Diagnostic>), CompileError> {
    let (checked, diagnostics) = check_path(path, options.project_root.as_deref())?;
    let Some(checked) = checked else {
        return Ok((None, diagnostics));
    };

    let unit = checked.emit_llvm(options.target_triple.as_deref())?;
    Ok((Some(unit), diagnostics))
}

pub fn check_path(
    path: impl AsRef<Path>,
    project_root: Option<&Path>,
) -> Result<(Option<CheckedProgram>, Vec<Diagnostic>), LoadError> {
    let (graph, diagnostics) = load_package_graph_with_root(path, project_root, false)?;
    if !diagnostics.is_empty() {
        return Ok((None, diagnostics));
    }

    Ok(check_graph(&graph))
}

pub fn compile_test_path(
    path: impl AsRef<Path>,
    options: &CompileOptions,
) -> Result<(Option<Unit>, Vec<Diagnostic>), CompileError> {
    let (mut graph, diagnostics) =
        load_package_graph_with_root(path, options.project_root.as_deref(), true)?;
    if !diagnostics.is_empty() {
        return Ok((None, diagnostics));
    }

    inject_test_runner(&mut graph)?;
    compile_graph(&graph, options)
}

fn compile_graph(
    graph: &PackageGraph,
    options: &CompileOptions,
) -> Result<(Option<Unit>, Vec<Diagnostic>), CompileError> {
    let (checked, diagnostics) = check_graph(graph);
    let Some(checked) = checked else {
        return Ok((None, diagnostics));
    };

    let unit = checked.emit_llvm(options.target_triple.as_deref())?;
    Ok((Some(unit), diagnostics))
}

fn check_graph(graph: &PackageGraph) -> (Option<CheckedProgram>, Vec<Diagnostic>) {
    let (lowered, diagnostics) = lower_package_graph(graph);
    if !diagnostics.is_empty() {
        return (None, diagnostics);
    }

    let (mono, diagnostics) = monomorphize_program(&lowered);
    if !diagnostics.is_empty() {
        return (None, diagnostics);
    }

    let (info, diagnostics) = check_program(&mono);
    if !diagnostics.is_empty() {
        return (None, diagnostics);
    }

    (
        Some(CheckedProgram {
            program: mono,
            info,
        }),
        Vec::new(),
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TestFunction {
    name: String,
}

fn discover_test_functions(graph: &PackageGraph) -> Vec<TestFunction> {
    let Some(entry) = graph.packages.get(&graph.entry) else {
        return Vec::new();
    };
    if !entry.imports.iter().any(is_testing_import) {
        return Vec::new();
    }

    let mut tests = Vec::new();
    for file in &entry.files {
        if !is_test_file(file) {
            continue;
        }
        for function in &file.functions {
            if is_test_function(function) {
                tests.push(TestFunction {
                    name: function.name.clone(),
                });
            }
        }
    }
    tests
}

fn is_testing_import(import: &PackageImport) -> bool {
    import.path == TESTING_IMPORT_PATH
        && matches!(&import.target.source, SourceId::Stdlib)
        && import.target.subpath == "testing"
}

fn is_test_file(program: &Program) -> bool {
    program.package_pos.file.ends_with("_test.yar")
}

fn is_test_function(function: &FunctionDecl) -> bool {
    if !function.name.starts_with("test_") {
        return false;
    }
    if function.receiver.is_some() {
        return false;
    }
    if !function.type_params.is_empty() {
        return false;
    }
    if function.params.len() != 1 {
        return false;
    }

    let param_type = &function.params[0].type_ref;
    if param_type.kind != TypeRefKind::Pointer {
        return false;
    }
    let Some(elem) = param_type.elem.as_deref() else {
        return false;
    };
    if elem.kind != TypeRefKind::Named || elem.name != "testing.T" {
        return false;
    }
    if function.return_type.kind != TypeRefKind::Named || function.return_type.name != "void" {
        return false;
    }
    !function.return_is_bang
}

fn generate_test_main(package_name: &str, tests: &[TestFunction]) -> String {
    let mut src = String::new();
    src.push_str("package ");
    src.push_str(package_name);
    src.push_str("\n\n");
    src.push_str("import \"std/testing\"\n\n");
    src.push_str("fn main() i32 {\n");
    src.push_str("    passed := 0\n");
    src.push_str("    failed := 0\n\n");

    for (index, test) in tests.iter().enumerate() {
        let test_var = format!("t{index}");
        src.push_str(&format!(
            "    {test_var} := &testing.T{{name: \"{}\", failed: false, messages: []str{{}}}}\n",
            test.name
        ));
        src.push_str(&format!("    {}({test_var})\n", test.name));
        src.push_str(&format!("    if (*{test_var}).failed {{\n"));
        src.push_str(&format!("        print(\"FAIL: {}\\n\")\n", test.name));
        src.push_str(&format!("        i{index} := 0\n"));
        src.push_str(&format!(
            "        for i{index} < len((*{test_var}).messages) {{\n"
        ));
        src.push_str(&format!(
            "            print(\"    \" + (*{test_var}).messages[i{index}] + \"\\n\")\n"
        ));
        src.push_str(&format!("            i{index} = i{index} + 1\n"));
        src.push_str("        }\n");
        src.push_str("        failed = failed + 1\n");
        src.push_str("    } else {\n");
        src.push_str(&format!("        print(\"PASS: {}\\n\")\n", test.name));
        src.push_str("        passed = passed + 1\n");
        src.push_str("    }\n\n");
    }

    src.push_str(
        "    print(\"\\n\" + to_str(passed) + \" passed, \" + to_str(failed) + \" failed\\n\")\n",
    );
    src.push_str("    if failed > 0 {\n");
    src.push_str("        return 1\n");
    src.push_str("    }\n");
    src.push_str("    return 0\n");
    src.push_str("}\n");
    src
}

fn inject_test_runner(graph: &mut PackageGraph) -> Result<(), CompileError> {
    let tests = discover_test_functions(graph);
    if tests.is_empty() {
        return Err(CompileError::Internal(
            "no test functions found".to_string(),
        ));
    }

    let entry_path = graph.entry.clone();
    let Some(entry_package) = graph.packages.get(&entry_path) else {
        return Err(CompileError::Internal(format!(
            "entry package {entry_path:?} not found"
        )));
    };
    let runner_src = generate_test_main(&entry_package.name, &tests);
    let (runner, diagnostics) = parse_file("<generated test runner>", &runner_src);
    if !diagnostics.is_empty() {
        return Err(CompileError::Internal(format!(
            "generated test runner failed to parse: {diagnostics:?}"
        )));
    }

    let runner_imports = runner.imports.clone();
    let new_imports = runner_imports
        .iter()
        .filter_map(|decl| {
            entry_package
                .imports
                .iter()
                .find(|import| import.path == decl.path && is_testing_import(import))
                .map(|import| PackageImport {
                    name: import.name.clone(),
                    path: decl.path.clone(),
                    target: import.target.clone(),
                    decl: decl.clone(),
                })
        })
        .collect::<Vec<_>>();

    let Some(entry_package) = graph.packages.get_mut(&entry_path) else {
        return Err(CompileError::Internal(format!(
            "entry package {entry_path:?} not found"
        )));
    };

    for file in &mut entry_package.files {
        file.functions
            .retain(|function| function.name != "main" || function.receiver.is_some());
    }
    entry_package
        .functions
        .retain(|function| function.name != "main" || function.receiver.is_some());

    for import in new_imports {
        if entry_package
            .imports
            .iter()
            .any(|existing| existing.path == import.path)
        {
            continue;
        }
        entry_package.imports.push(import);
    }

    entry_package.functions.extend(runner.functions.clone());
    entry_package.files.push(runner);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::load_package_graph;

    #[test]
    fn compiles_entry_path_to_ir() {
        let root = repo_root();
        let (unit, diagnostics) = compile_path(
            root.join("testdata/hello/main.yar"),
            &CompileOptions::default(),
        )
        .unwrap();

        assert_eq!(diagnostics, Vec::new());
        let unit = unit.unwrap();
        assert!(unit.ir.contains("define i32 @main(i32 %argc, ptr %argv)"));
    }

    #[test]
    fn compile_options_emit_target_triple() {
        let root = repo_root();
        let options = CompileOptions {
            target_triple: Some("x86_64-unknown-linux-gnu".to_string()),
            project_root: None,
        };
        let (unit, diagnostics) =
            compile_path(root.join("testdata/hello/main.yar"), &options).unwrap();

        assert_eq!(diagnostics, Vec::new());
        let unit = unit.unwrap();
        assert!(
            unit.ir
                .contains("target triple = \"x86_64-unknown-linux-gnu\"")
        );
    }

    #[test]
    fn compile_options_select_an_explicit_project_root() {
        let root = temp_dir("yar-rust-explicit-project-root");
        let inner = root.join("inner");
        let entry = inner.join("app");
        write_source(
            &root.join("yar.toml"),
            r#"[package]
name = "outer"
"#,
        );
        write_source(
            &inner.join("yar.toml"),
            r#"[package]
name = "inner"
"#,
        );
        write_source(
            &root.join("shared/shared.yar"),
            "package shared\n\npub fn value() i32 { return 0 }\n",
        );
        write_source(
            &entry.join("main.yar"),
            "package main\n\nimport \"shared\"\n\nfn main() i32 { return shared.value() }\n",
        );
        let options = CompileOptions {
            project_root: Some(root),
            ..CompileOptions::default()
        };

        let (unit, diagnostics) = compile_path(&entry, &options).unwrap();

        assert_eq!(diagnostics, Vec::new());
        assert!(unit.is_some());
    }

    #[test]
    fn discovers_test_functions_for_fixture() {
        let root = repo_root();
        let (graph, diagnostics) =
            load_package_graph(root.join("testdata/testing_basic"), true).unwrap();

        assert_eq!(diagnostics, Vec::new());
        let names = discover_test_functions(&graph)
            .into_iter()
            .map(|test| test.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "test_add",
                "test_greet",
                "test_divide",
                "test_divide_by_zero",
                "test_bool_assertions",
            ]
        );
    }

    #[test]
    fn does_not_discover_tests_bound_to_a_local_testing_package() {
        let root = temp_dir("yar-rust-local-testing-package");
        write_source(
            &root.join("main.yar"),
            r#"package main

fn main() i32 {
    return 0
}
"#,
        );
        write_source(
            &root.join("main_test.yar"),
            r#"package main

import "testing"

fn test_local(t *testing.T) void {
    return
}
"#,
        );
        write_source(
            &root.join("testing/testing.yar"),
            r#"package testing

pub struct T {}
"#,
        );

        let (graph, diagnostics) = load_package_graph(&root, true).unwrap();

        assert_eq!(diagnostics, Vec::new());
        assert!(discover_test_functions(&graph).is_empty());
    }

    #[test]
    fn generates_test_runner_source() {
        let source = generate_test_main(
            "main",
            &[
                TestFunction {
                    name: "test_add".to_string(),
                },
                TestFunction {
                    name: "test_sub".to_string(),
                },
            ],
        );

        assert!(source.contains("package main"));
        assert!(source.contains("import \"std/testing\""));
        assert!(source.contains("test_add(t0)"));
        assert!(source.contains("test_sub(t1)"));
    }

    #[test]
    fn compiles_test_path_to_ir() {
        let root = repo_root();
        let (unit, diagnostics) = compile_test_path(
            root.join("testdata/testing_basic"),
            &CompileOptions::default(),
        )
        .unwrap();

        assert_eq!(diagnostics, Vec::new());
        let unit = unit.unwrap();
        assert!(unit.ir.contains("define i32 @main(i32 %argc, ptr %argv)"));
        assert!(unit.ir.contains("PASS: test_add"));
    }

    #[test]
    fn compiles_character_literals_to_i32_constants() {
        let root = temp_dir("yar-rust-char-literals");
        write_source(
            &root.join("main.yar"),
            r#"package main

fn main() i32 {
    if 'a' != 97 { return 1 }
    if '\n' != 10 { return 2 }
    if '\t' != 9 { return 3 }
    if '\\' != 92 { return 4 }
    if '\'' != 39 { return 5 }
    var x i32 = 'z'
    if x != 122 { return 6 }
    return 0
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert_eq!(diagnostics, Vec::new());
        let ir = unit.unwrap().ir;
        assert!(ir.contains("icmp ne i32 97, 97"));
        assert!(ir.contains("icmp ne i32 10, 10"));
        assert!(ir.contains("icmp ne i32 9, 9"));
        assert!(ir.contains("icmp ne i32 92, 92"));
        assert!(ir.contains("icmp ne i32 39, 39"));
        assert!(ir.contains("store i32 122"));
    }

    #[test]
    fn compile_path_rejects_unexported_imported_function() {
        let root = temp_dir("yar-rust-unexported-function");
        write_source(
            &root.join("main.yar"),
            r#"package main

import "lib"

fn main() i32 {
    return lib.hidden()
}
"#,
        );
        write_source(
            &root.join("lib/lib.yar"),
            r#"package lib

fn hidden() i32 {
    return 0
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(
            &diagnostics,
            "package \"lib\" does not export function \"hidden\"",
        );
    }

    #[test]
    fn compile_path_rejects_spawning_embedded_resource_values() {
        let root = temp_dir("yar-rust-spawn-resource");
        write_source(
            &root.join("main.yar"),
            r#"package main

import "std/fs"

fn consume(file fs.File) void {
    return
}

fn main() i32 {
    file := fs.open_read("missing") or |_| {
        return 0
    }
    taskgroup []void {
        spawn consume(file)
    }
    return 0
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(
            &diagnostics,
            "spawn argument 1 cannot cross a task boundary: resource type \"fs.File\" is not share-safe",
        );
    }

    #[test]
    fn compile_path_allows_share_safe_local_stdlib_shadows() {
        let root = temp_dir("yar-rust-spawn-local-stdlib-shadow");
        write_source(
            &root.join("main.yar"),
            r#"package main

import "fs"

fn inspect(file fs.File) i64 {
    return file.handle
}

fn main() i32 {
    results := taskgroup []i64 {
        spawn inspect(fs.make_file(7))
    }
    numbers := taskgroup []i32 {
        spawn fs.read_file(8)
    }
    if results[0] != 7 || numbers[0] != 9 {
        return 1
    }
    return 0
}
"#,
        );
        write_source(
            &root.join("fs/fs.yar"),
            r#"package fs

pub struct File {
    handle i64
}

pub fn make_file(handle i64) File {
    return File{handle: handle}
}

pub fn read_file(value i32) i32 {
    return value + 1
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        let unit = unit.unwrap_or_else(|| panic!("unexpected diagnostics: {diagnostics:?}"));
        assert_eq!(diagnostics, Vec::new());
        assert!(unit.ir.contains(".fs.read_file(i32 %arg.0)"));
        assert!(!unit.ir.contains("call i32 @yar.fs.read_file(i32 %arg.0)"));
        assert!(!unit.ir.contains("call i32 @yar_fs_read_file"));
    }

    #[test]
    fn compile_path_rejects_spawning_host_intrinsic_without_task_wrapper() {
        let root = temp_dir("yar-rust-spawn-host-intrinsic");
        write_source(
            &root.join("main.yar"),
            r#"package main

import "std/fs"

fn main() i32 {
    taskgroup []!void {
        spawn fs.write_file("output.txt", "data")
    }
    return 0
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(
            &diagnostics,
            "spawn does not currently support host intrinsic \"fs.write_file\" directly; wrap it in an inline function literal",
        );
    }

    #[test]
    fn compile_path_rejects_unexported_imported_method() {
        let root = temp_dir("yar-rust-unexported-method");
        write_source(
            &root.join("main.yar"),
            r#"package main

import "lib"

fn main() i32 {
    box := lib.make_box(3)
    return box.secret()
}
"#,
        );
        write_source(
            &root.join("lib/lib.yar"),
            r#"package lib

pub struct Box {
    value i32
}

pub fn make_box(value i32) Box {
    return Box{value: value}
}

fn (b Box) secret() i32 {
    return b.value
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(
            &diagnostics,
            "package \"lib\" does not export method \"secret\"",
        );
    }

    #[test]
    fn compile_path_rejects_exported_api_with_hidden_types() {
        let root = temp_dir("yar-rust-exported-hidden-types");
        write_source(
            &root.join("main.yar"),
            r#"package main

import "lib"

fn main() i32 {
    return 0
}
"#,
        );
        write_source(
            &root.join("lib/lib.yar"),
            r#"package lib

struct hidden {
    id i32
}

enum hiddenEnum {
    A
}

pub struct Wrapper {
    inner hidden
}

pub fn make() hidden {
    return hidden{id: 1}
}

pub fn make_enum() hiddenEnum {
    return hiddenEnum.A
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(
            &diagnostics,
            "exported struct \"Wrapper\" cannot use non-exported type \"hidden\"",
        );
        assert_diag_contains(
            &diagnostics,
            "exported function \"make\" cannot use non-exported type \"hidden\"",
        );
        assert_diag_contains(
            &diagnostics,
            "exported function \"make_enum\" cannot use non-exported type \"hiddenEnum\"",
        );
    }

    #[test]
    fn compile_path_rejects_builtin_function_shadowing() {
        let root = temp_dir("yar-rust-builtin-shadowing");
        write_source(
            &root.join("main.yar"),
            r#"package main

fn append(values []i32, value i32) []i32 {
    return values
}

fn len(value i32) i32 {
    return value
}

fn main() i32 {
    return 0
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(&diagnostics, "function \"append\" is already declared");
        assert_diag_contains(&diagnostics, "function \"len\" is already declared");
    }

    #[test]
    fn compile_path_reports_source_names_for_local_generic_function_errors() {
        let root = temp_dir("yar-rust-generic-diagnostics");
        write_source(
            &root.join("main.yar"),
            r#"package main

fn id[T](value T) T {
    return value
}

fn main() i32 {
    missing := id(1)
    wrong := id[i32, i64](1)
    return missing + wrong
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(
            &diagnostics,
            "generic function \"id\" requires explicit type arguments",
        );
        assert_diag_contains(
            &diagnostics,
            "generic function \"id\" expects 1 type arguments, got 2",
        );
    }

    #[test]
    fn compile_path_hides_internal_package_identity_in_imported_diagnostics() {
        let root = temp_dir("yar-rust-imported-package-diagnostics");
        write_source(
            &root.join("main.yar"),
            r#"package main

import "users"

fn main() i32 {
    return users.value()
}
"#,
        );
        write_source(
            &root.join("users/users.yar"),
            r#"package users

pub struct User {}

pub fn value() i32 {
    return User{}
}

pub fn incomplete() User {
    if true {
        return User{}
    }
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(
            &diagnostics,
            "cannot return users.User from function returning i32",
        );
        assert_diag_contains(
            &diagnostics,
            "function \"users.incomplete\" must return a value on all paths",
        );
        assert_diag_not_contains(&diagnostics, "$yar$");
    }

    #[test]
    fn compile_path_reports_source_names_for_entry_package_method_errors() {
        let root = temp_dir("yar-rust-method-diagnostics");
        write_source(
            &root.join("main.yar"),
            r#"package main

struct Counter {
    value i32
}

fn (c Counter) current() i32 {
    return c.value
}

fn main() i32 {
    counter := &Counter{value: 2}
    return counter.current()
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(&diagnostics, "type \"*Counter\" has no method \"current\"");
        assert_diag_not_contains(
            &diagnostics,
            "type \"*main.Counter\" has no method \"current\"",
        );
    }

    #[test]
    fn compile_path_reports_source_names_for_entry_package_interface_errors() {
        let root = temp_dir("yar-rust-interface-diagnostics");
        write_source(
            &root.join("main.yar"),
            r#"package main

interface Writer {
    write(msg str) !void
}

struct Buffer {
    prefix str
}

fn main() !i32 {
    writer := Buffer{prefix: "hi "}
    consume(writer)?
    return 0
}

fn consume(w Writer) !void {
    return w.write("yar")
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(
            &diagnostics,
            "type \"Buffer\" does not satisfy interface \"Writer\" (missing method \"write\")",
        );
        assert_diag_not_contains(
            &diagnostics,
            "type \"main.Buffer\" does not satisfy interface \"main.Writer\" (missing method \"write\")",
        );
    }

    #[test]
    fn compile_path_reports_source_names_for_entry_package_type_errors() {
        let root = temp_dir("yar-rust-type-diagnostics");
        write_source(
            &root.join("main.yar"),
            r#"package main

struct User {
    id i32
}

fn main() i32 {
    user := User{id: 1}
    var other User = true
    users := []User{}
    append(users, true)
    map[str]User{"ok": true}
    return user.name
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(&diagnostics, "cannot assign bool to User");
        assert_diag_contains(
            &diagnostics,
            "argument 2 to \"append\" must be User, got bool",
        );
        assert_diag_contains(&diagnostics, "struct \"User\" has no field \"name\"");
        assert_diag_not_contains(
            &diagnostics,
            "argument 2 to \"append\" must be main.User, got bool",
        );
        assert_diag_not_contains(&diagnostics, "cannot assign bool to main.User");
        assert_diag_not_contains(&diagnostics, "struct \"main.User\" has no field \"name\"");
    }

    #[test]
    fn compile_path_reports_source_names_for_recursive_type_errors() {
        let root = temp_dir("yar-rust-recursive-type-diagnostics");
        write_source(
            &root.join("main.yar"),
            r#"package main

struct Bad {
    next Bad
}

fn main() i32 {
    return 0
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(
            &diagnostics,
            "struct \"Bad\" cannot contain itself recursively",
        );
        assert_diag_not_contains(
            &diagnostics,
            "struct \"main.Bad\" cannot contain itself recursively",
        );
    }

    #[test]
    fn compile_path_reports_source_names_for_entry_package_function_arity_errors() {
        let root = temp_dir("yar-rust-function-arity-diagnostics");
        write_source(
            &root.join("main.yar"),
            r#"package main

struct User {
    id i32
}

fn consume(user User, n i32) i32 {
    return n
}

fn main() i32 {
    return consume(User{id: 1})
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(
            &diagnostics,
            "function \"consume\" expects 2 arguments, got 1",
        );
        assert_diag_not_contains(
            &diagnostics,
            "function \"main.consume\" expects 2 arguments, got 1",
        );
    }

    #[test]
    fn compile_path_reports_source_names_for_nested_entry_package_errors() {
        let root = temp_dir("yar-rust-nested-entry-diagnostics");
        write_source(&root.join("yar.toml"), "[package]\nname = \"project\"\n");
        write_source(
            &root.join("cmd/app/main.yar"),
            r#"package main

struct User {
    id i32
}

fn consume(user User, n i32) i32 {
    return n
}

fn main() i32 {
    var user User = true
    return consume(user)
}
"#,
        );

        let (unit, diagnostics) =
            compile_path(root.join("cmd/app/main.yar"), &CompileOptions::default()).unwrap();

        assert!(unit.is_none());
        assert_diag_contains(&diagnostics, "cannot assign bool to User");
        assert_diag_contains(
            &diagnostics,
            "function \"consume\" expects 2 arguments, got 1",
        );
        assert_diag_not_contains(&diagnostics, "cannot assign bool to main.User");
        assert_diag_not_contains(
            &diagnostics,
            "function \"main.consume\" expects 2 arguments, got 1",
        );
    }

    fn repo_root() -> std::path::PathBuf {
        let mut dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        dir.pop();
        dir.pop();
        dir
    }

    fn write_source(path: &std::path::Path, source: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, source).unwrap();
    }

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn assert_diag_contains(diagnostics: &[Diagnostic], expected: &str) {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message == expected),
            "missing diagnostic {expected:?}; got {diagnostics:?}",
        );
    }

    fn assert_diag_not_contains(diagnostics: &[Diagnostic], unexpected: &str) {
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.message != unexpected),
            "unexpected diagnostic {unexpected:?}; got {diagnostics:?}",
        );
    }
}
