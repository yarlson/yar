use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

use crate::{
    ast::{Package, PackageGraph, PackageImport, Program},
    diag::{Diagnostic, List},
    manifest::{
        Dependency, LOCK_FILE, MANIFEST_FILE, ManifestError, cache_dir_from_env, cache_path,
        read_lock_file, read_manifest,
    },
    parser::parse_file,
    token::Position,
};

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Manifest(ManifestError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(err) => write!(f, "{err}"),
            LoadError::Manifest(err) => write!(f, "{err}"),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LoadError::Io(err) => Some(err),
            LoadError::Manifest(err) => Some(err),
        }
    }
}

impl From<std::io::Error> for LoadError {
    fn from(value: std::io::Error) -> Self {
        LoadError::Io(value)
    }
}

impl From<ManifestError> for LoadError {
    fn from(value: ManifestError) -> Self {
        LoadError::Manifest(value)
    }
}

pub fn load_package_graph(
    path: impl AsRef<Path>,
    include_tests: bool,
) -> Result<(PackageGraph, Vec<Diagnostic>), LoadError> {
    let (root_dir, entry_dir) = resolve_entry_dirs(path.as_ref())?;
    let dependency_index = DependencyIndex::load(&root_dir)?;
    let mut loader = PackageLoader {
        root_dir,
        packages: BTreeMap::new(),
        diag: List::default(),
        include_tests,
        dependency_index,
    };

    match loader.load_package("", &entry_dir)? {
        Some(entry) => {
            if let Some(pkg) = loader.packages.get(&entry)
                && pkg.name != "main"
                && !include_tests
                && let Some(file) = pkg.files.first()
            {
                loader
                    .diag
                    .add(file.package_pos.clone(), "package must be main");
            }
            let mut graph = PackageGraph {
                entry_path: String::new(),
                entry,
                packages: loader.packages,
            };
            check_import_cycles(&mut graph, &mut loader.diag);
            Ok((graph, loader.diag.items()))
        }
        None => Ok((
            PackageGraph {
                entry_path: String::new(),
                entry: String::new(),
                packages: loader.packages,
            },
            loader.diag.items(),
        )),
    }
}

fn resolve_entry_dirs(path: &Path) -> Result<(PathBuf, PathBuf), LoadError> {
    let clean_path = if path == Path::new(".") {
        std::env::current_dir()?
    } else {
        path.to_path_buf()
    };
    let metadata = fs::metadata(&clean_path)?;
    if metadata.is_dir() {
        return Ok((clean_path.clone(), clean_path));
    }
    let parent = clean_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    Ok((parent.clone(), parent))
}

struct PackageLoader {
    root_dir: PathBuf,
    packages: BTreeMap<String, Package>,
    diag: List,
    include_tests: bool,
    dependency_index: DependencyIndex,
}

impl PackageLoader {
    fn load_package(&mut self, import_path: &str, dir: &Path) -> Result<Option<String>, LoadError> {
        if self.packages.contains_key(import_path) {
            return Ok(Some(import_path.to_owned()));
        }

        if !import_path.is_empty()
            && !dir.exists()
            && let Some(dependency_dir) = self.dependency_index.resolve(import_path)
            && dependency_dir != dir
        {
            return self.load_package(import_path, &dependency_dir);
        }

        let package = match self.read_local_package(import_path, dir)? {
            Some(package) => package,
            None if !import_path.is_empty() => match read_stdlib_package(import_path) {
                Some((package, diagnostics)) => {
                    self.diag.append(&diagnostics);
                    package
                }
                None => return Ok(None),
            },
            None => return Ok(None),
        };

        self.packages.insert(import_path.to_owned(), package);
        self.resolve_imports(import_path)?;
        Ok(Some(import_path.to_owned()))
    }

    fn read_local_package(
        &mut self,
        import_path: &str,
        dir: &Path,
    ) -> Result<Option<Package>, LoadError> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if !import_path.is_empty() && err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(None);
            }
            Err(err) => return Err(err.into()),
        };

        let mut file_names = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || path.extension().is_none_or(|ext| ext != "yar") {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if !self.include_tests && name.ends_with("_test.yar") {
                continue;
            }
            file_names.push(name);
        }
        file_names.sort();

        if file_names.is_empty() {
            self.diag.add(
                Position {
                    file: dir.display().to_string(),
                    line: 1,
                    column: 1,
                },
                format!("package directory {:?} has no .yar files", dir),
            );
            return Ok(None);
        }

        let mut files = Vec::new();
        let mut package_name = String::new();
        for name in file_names {
            let file_path = dir.join(&name);
            let src = fs::read_to_string(&file_path)?;
            let file_name = file_path.to_string_lossy();
            let (program, diagnostics) = parse_file(&file_name, &src);
            self.diag.append(&diagnostics);
            if package_name.is_empty() {
                package_name = program.package_name.clone();
            } else if program.package_name != package_name {
                self.diag.add(
                    program.package_pos.clone(),
                    format!(
                        "package {:?} does not match package {:?} in {:?}",
                        program.package_name, package_name, dir
                    ),
                );
            }
            files.push(program);
        }

        Ok(Some(package_from_files(
            import_path.to_owned(),
            package_name,
            false,
            files,
        )))
    }

    fn resolve_imports(&mut self, package_path: &str) -> Result<(), LoadError> {
        let Some(package) = self.packages.get(package_path).cloned() else {
            return Ok(());
        };

        let mut seen_imports = BTreeSet::new();
        let mut imports = Vec::new();
        for file in &package.files {
            for decl in &file.imports {
                if !valid_import_path(&decl.path) {
                    self.diag.add(
                        decl.path_pos.clone(),
                        format!("invalid import path {:?}", decl.path),
                    );
                    continue;
                }
                if !seen_imports.insert(decl.path.clone()) {
                    continue;
                }
                let target_dir = self
                    .root_dir
                    .join(decl.path.replace('/', std::path::MAIN_SEPARATOR_STR));
                let Some(target_path) = self.load_package(&decl.path, &target_dir)? else {
                    self.diag.add(
                        decl.path_pos.clone(),
                        format!("import {:?} could not be loaded", decl.path),
                    );
                    continue;
                };
                let Some(target) = self.packages.get(&target_path) else {
                    continue;
                };
                if target.name == "main" {
                    self.diag
                        .add(decl.path_pos.clone(), "cannot import package main");
                    continue;
                }
                let want = last_import_segment(&decl.path);
                if want != target.name {
                    self.diag.add(
                        decl.path_pos.clone(),
                        format!(
                            "import {:?} must declare package {:?}, got {:?}",
                            decl.path, want, target.name
                        ),
                    );
                    continue;
                }
                imports.push(PackageImport {
                    name: target.name.clone(),
                    path: decl.path.clone(),
                    decl: decl.clone(),
                });
            }
        }

        if let Some(package) = self.packages.get_mut(package_path) {
            package.imports = imports;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct DependencyIndex {
    entries: BTreeMap<String, PathBuf>,
}

impl DependencyIndex {
    fn load(root_dir: &Path) -> Result<Self, LoadError> {
        let manifest_path = root_dir.join(MANIFEST_FILE);
        if !manifest_path.exists() {
            return Ok(Self::default());
        }

        let manifest = read_manifest(&manifest_path)?;
        if manifest.dependencies.is_empty() {
            return Ok(Self::default());
        }

        let mut entries = BTreeMap::new();
        let has_git_dependencies = manifest
            .dependencies
            .values()
            .any(|dependency| !dependency.is_local());
        if has_git_dependencies {
            let lock_path = root_dir.join(LOCK_FILE);
            let lock_entries = read_lock_file(&lock_path).map_err(|err| {
                LoadError::Manifest(ManifestError::Invalid(format!(
                    "yar.toml exists but yar.lock is missing or invalid (run 'yar lock'): {err}"
                )))
            })?;
            let cache_dir = cache_dir_from_env()?;
            for entry in lock_entries {
                entries.insert(
                    entry.name,
                    cache_path(&cache_dir, &entry.git, &entry.commit),
                );
            }
        }

        for (alias, dependency) in manifest.dependencies {
            if dependency.is_local() {
                entries.insert(alias, dependency_dir(root_dir, &dependency));
            }
        }

        Ok(Self { entries })
    }

    fn resolve(&self, import_path: &str) -> Option<PathBuf> {
        let (alias, sub_path) = import_path.split_once('/').unwrap_or((import_path, ""));
        let dir = self.entries.get(alias)?;
        if sub_path.is_empty() {
            return Some(dir.clone());
        }
        Some(dir.join(sub_path.replace('/', std::path::MAIN_SEPARATOR_STR)))
    }
}

fn dependency_dir(root_dir: &Path, dependency: &Dependency) -> PathBuf {
    let path = PathBuf::from(&dependency.path);
    let dir = if path.is_absolute() {
        path
    } else {
        root_dir.join(path)
    };
    clean_path(&dir)
}

fn clean_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn package_from_files(path: String, name: String, stdlib: bool, files: Vec<Program>) -> Package {
    let mut package = Package {
        path,
        name,
        stdlib,
        files,
        ..Package::default()
    };
    for file in &package.files {
        package.structs.extend(file.structs.clone());
        package.interfaces.extend(file.interfaces.clone());
        package.enums.extend(file.enums.clone());
        package.functions.extend(file.functions.clone());
    }
    package
}

fn read_stdlib_package(import_path: &str) -> Option<(Package, Vec<Diagnostic>)> {
    let entries = stdlib_entries(import_path)?;
    let mut files = Vec::new();
    let mut diagnostics = Vec::new();
    let mut package_name = String::new();
    for (name, src) in entries {
        let synthetic_path = format!("<stdlib>/{import_path}/{name}");
        let (program, diags) = parse_file(&synthetic_path, src);
        diagnostics.extend(diags);
        if package_name.is_empty() {
            package_name = program.package_name.clone();
        } else if program.package_name != package_name {
            diagnostics.push(Diagnostic {
                pos: program.package_pos.clone(),
                message: format!(
                    "package {:?} does not match package {:?} in stdlib {:?}",
                    program.package_name, package_name, import_path
                ),
            });
        }
        files.push(program);
    }
    Some((
        package_from_files(import_path.to_owned(), package_name, true, files),
        diagnostics,
    ))
}

fn stdlib_entries(import_path: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match import_path {
        "conv" => Some(&[(
            "conv.yar",
            include_str!("../../../stdlib/packages/conv/conv.yar"),
        )]),
        "env" => Some(&[(
            "env.yar",
            include_str!("../../../stdlib/packages/env/env.yar"),
        )]),
        "fs" => Some(&[("fs.yar", include_str!("../../../stdlib/packages/fs/fs.yar"))]),
        "http" => Some(&[(
            "http.yar",
            include_str!("../../../stdlib/packages/http/http.yar"),
        )]),
        "io" => Some(&[("io.yar", include_str!("../../../stdlib/packages/io/io.yar"))]),
        "net" => Some(&[(
            "net.yar",
            include_str!("../../../stdlib/packages/net/net.yar"),
        )]),
        "path" => Some(&[(
            "path.yar",
            include_str!("../../../stdlib/packages/path/path.yar"),
        )]),
        "process" => Some(&[(
            "process.yar",
            include_str!("../../../stdlib/packages/process/process.yar"),
        )]),
        "sort" => Some(&[(
            "sort.yar",
            include_str!("../../../stdlib/packages/sort/sort.yar"),
        )]),
        "stdio" => Some(&[(
            "stdio.yar",
            include_str!("../../../stdlib/packages/stdio/stdio.yar"),
        )]),
        "strings" => Some(&[(
            "strings.yar",
            include_str!("../../../stdlib/packages/strings/strings.yar"),
        )]),
        "testing" => Some(&[(
            "testing.yar",
            include_str!("../../../stdlib/packages/testing/testing.yar"),
        )]),
        "utf8" => Some(&[(
            "utf8.yar",
            include_str!("../../../stdlib/packages/utf8/utf8.yar"),
        )]),
        _ => None,
    }
}

fn valid_import_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.starts_with('.') || path.contains("//") {
        return false;
    }
    path.split('/').all(valid_import_segment)
}

fn valid_import_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '_' && !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn last_import_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn check_import_cycles(graph: &mut PackageGraph, diagnostics: &mut List) {
    let mut visited = BTreeSet::new();
    let mut active = BTreeSet::new();
    let mut stack = Vec::new();
    let entry = graph.entry.clone();
    visit_package(
        graph,
        &entry,
        &mut visited,
        &mut active,
        &mut stack,
        diagnostics,
    );
}

fn visit_package(
    graph: &PackageGraph,
    package_path: &str,
    visited: &mut BTreeSet<String>,
    active: &mut BTreeSet<String>,
    stack: &mut Vec<String>,
    diagnostics: &mut List,
) {
    if visited.contains(package_path) {
        return;
    }
    let Some(package) = graph.packages.get(package_path) else {
        return;
    };

    visited.insert(package_path.to_owned());
    active.insert(package_path.to_owned());
    stack.push(package_path.to_owned());

    for import in &package.imports {
        let target_path = &import.path;
        if !graph.packages.contains_key(target_path) {
            continue;
        }
        if active.contains(target_path) {
            let cycle = append_cycle(stack, target_path);
            diagnostics.add(
                import.decl.path_pos.clone(),
                format!("import cycle: {}", cycle.join(" -> ")),
            );
            continue;
        }
        visit_package(graph, target_path, visited, active, stack, diagnostics);
    }

    stack.pop();
    active.remove(package_path);
}

fn append_cycle(stack: &[String], target: &str) -> Vec<String> {
    let start = stack.iter().position(|path| path == target).unwrap_or(0);
    let mut cycle = stack[start..].to_vec();
    cycle.push(target.to_owned());
    for path in &mut cycle {
        if path.is_empty() {
            *path = "main".to_owned();
        }
    }
    cycle
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn loads_local_import_graph() {
        let root = repo_root();
        let (graph, diagnostics) =
            load_package_graph(root.join("testdata/imports_ok/main.yar"), false).unwrap();

        assert_eq!(diagnostics, Vec::new());
        assert_eq!(graph.entry, "");
        assert!(graph.packages.contains_key(""));
        assert!(graph.packages.contains_key("lexer"));
        assert!(graph.packages.contains_key("token"));
        assert_eq!(graph.packages[""].name, "main");
        assert_eq!(graph.packages["lexer"].name, "lexer");
    }

    #[test]
    fn loads_stdlib_fallback_graph() {
        let root = repo_root();
        let (graph, diagnostics) =
            load_package_graph(root.join("testdata/stdlib_http/main.yar"), false).unwrap();

        assert_eq!(diagnostics, Vec::new());
        assert!(graph.packages["http"].stdlib);
        assert!(graph.packages["net"].stdlib);
        assert!(graph.packages["strings"].stdlib);
    }

    #[test]
    fn loads_local_path_dependency_from_manifest() {
        let dir = temp_dir("yar-rust-local-dep");
        let app_dir = dir.join("app");
        let dep_dir = dir.join("deps").join("vendor").join("mathlib");
        fs::create_dir_all(&app_dir).unwrap();
        fs::create_dir_all(&dep_dir).unwrap();
        fs::write(
            app_dir.join("yar.toml"),
            r#"[package]
name = "app"

[dependencies]
vendor = { path = "../deps/vendor" }
"#,
        )
        .unwrap();
        fs::write(
            app_dir.join("main.yar"),
            r#"package main

import "vendor/mathlib"

fn main() i32 {
    print(to_str(mathlib.add(2, 3)))
    return 0
}
"#,
        )
        .unwrap();
        fs::write(
            dep_dir.join("mathlib.yar"),
            r#"package mathlib

pub fn add(a i32, b i32) i32 {
    return a + b
}
"#,
        )
        .unwrap();

        let (graph, diagnostics) = load_package_graph(&app_dir, false).unwrap();

        assert_eq!(diagnostics, Vec::new());
        assert_eq!(graph.packages["vendor/mathlib"].name, "mathlib");
        assert_eq!(graph.packages[""].imports[0].path, "vendor/mathlib");
    }

    #[test]
    fn loads_every_testdata_entry_graph_without_diagnostics() {
        let root = repo_root();
        let mut entries = Vec::new();
        collect_main_files(&root.join("testdata"), &mut entries);
        entries.sort();

        assert!(
            !entries.is_empty(),
            "expected checked-in Yar entry fixtures"
        );

        let mut failures = Vec::new();
        for entry in entries {
            let (graph, diagnostics) = load_package_graph(&entry, false).unwrap_or_else(|err| {
                panic!("load {}: {err}", entry.display());
            });
            if !diagnostics.is_empty() {
                failures.push(format!("{}: {:?}", entry.display(), diagnostics));
                continue;
            }
            if graph.packages.get("").is_none_or(|pkg| pkg.name != "main") {
                failures.push(format!("{}: missing main entry package", entry.display()));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate is nested under crates/yar-compiler")
            .to_path_buf()
    }

    fn collect_main_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in
            std::fs::read_dir(dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        {
            let path = entry
                .unwrap_or_else(|err| panic!("read entry in {}: {err}", dir.display()))
                .path();
            if path.is_dir() {
                collect_main_files(&path, out);
                continue;
            }
            if path.file_name().is_some_and(|name| name == "main.yar") {
                out.push(path);
            }
        }
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
}
