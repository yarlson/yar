use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
};

use crate::{
    ast::{Package, PackageGraph, PackageId, PackageImport, Program, SourceId},
    diag::{Diagnostic, List},
    lock_graph::{reconcile_lock_entries, requires_lock, verify_locked_dependency_manifest},
    manifest::{
        Dependency, LOCK_FILE, LockDependency, MANIFEST_FILE, ManifestError, STDLIB_IMPORT_ROOT,
        cache_dir_from_env, cache_path, clean_path, read_lock_file, read_manifest,
        verify_cached_dependency,
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
    include_entry_tests: bool,
) -> Result<(PackageGraph, Vec<Diagnostic>), LoadError> {
    load_package_graph_with_root(path, None, include_entry_tests)
}

pub(crate) fn load_package_graph_with_root(
    path: impl AsRef<Path>,
    project_root: Option<&Path>,
    include_entry_tests: bool,
) -> Result<(PackageGraph, Vec<Diagnostic>), LoadError> {
    let entry_dir = resolve_entry_dir(path.as_ref())?;
    let (root_dir, entry_subpath) = resolve_project_root(&entry_dir, project_root)?;
    let dependency_index = DependencyIndex::load(&root_dir)?;
    let entry_location = PackageLocation {
        id: PackageId {
            source: SourceId::Entry,
            subpath: entry_subpath,
        },
        dir: Some(entry_dir),
        import_path: String::new(),
    };
    let mut loader = PackageLoader {
        packages: BTreeMap::new(),
        diag: List::default(),
        test_package: include_entry_tests.then(|| entry_location.id.clone()),
        dependency_index,
    };
    match loader.load_package(entry_location)? {
        Some(entry) => {
            if let Some(pkg) = loader.packages.get(&entry)
                && pkg.name != "main"
                && !include_entry_tests
                && let Some(file) = pkg.files.first()
            {
                loader
                    .diag
                    .add(file.package_pos.clone(), "package must be main");
            }
            let mut graph = PackageGraph {
                entry,
                packages: loader.packages,
            };
            check_import_cycles(&mut graph, &mut loader.diag);
            Ok((graph, loader.diag.items()))
        }
        None => Ok((PackageGraph::default(), loader.diag.items())),
    }
}

fn resolve_entry_dir(path: &Path) -> Result<PathBuf, LoadError> {
    let clean_path = if path == Path::new(".") {
        std::env::current_dir()?
    } else {
        path.to_path_buf()
    };
    let metadata = fs::metadata(&clean_path)?;
    if metadata.is_dir() {
        return Ok(clean_path);
    }
    let parent = clean_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf();
    Ok(parent)
}

fn resolve_project_root(
    entry_dir: &Path,
    explicit_root: Option<&Path>,
) -> Result<(PathBuf, String), LoadError> {
    let entry_dir = fs::canonicalize(absolute_path(entry_dir)?)?;
    let root_dir = match explicit_root {
        Some(root) => {
            let root = absolute_path(root)?;
            let metadata = fs::metadata(&root)?;
            if !metadata.is_dir() {
                return Err(invalid_project_root(format!(
                    "project root {} is not a directory",
                    root.display()
                )));
            }
            root
        }
        None => discover_project_root(&entry_dir)?.unwrap_or_else(|| entry_dir.clone()),
    };

    let root_dir = fs::canonicalize(root_dir)?;
    let relative = entry_dir.strip_prefix(&root_dir).map_err(|_| {
        invalid_project_root(format!(
            "entry package {} is outside project root {}",
            entry_dir.display(),
            root_dir.display()
        ))
    })?;
    let subpath = package_subpath(relative)?;
    Ok((root_dir, subpath))
}

fn absolute_path(path: &Path) -> Result<PathBuf, LoadError> {
    if path.is_absolute() {
        return Ok(clean_path(path));
    }
    Ok(clean_path(&std::env::current_dir()?.join(path)))
}

fn discover_project_root(entry_dir: &Path) -> Result<Option<PathBuf>, LoadError> {
    for ancestor in entry_dir.ancestors() {
        let manifest = ancestor.join(MANIFEST_FILE);
        match fs::symlink_metadata(&manifest) {
            Ok(metadata) if metadata.file_type().is_file() => {
                return Ok(Some(ancestor.to_path_buf()));
            }
            Ok(_) => {
                return Err(invalid_project_root(format!(
                    "manifest candidate {} is not a regular file",
                    manifest.display()
                )));
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(None)
}

fn package_subpath(path: &Path) -> Result<String, LoadError> {
    let mut segments = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err(invalid_project_root(format!(
                "entry package path {} is not relative to its project root",
                path.display()
            )));
        };
        let Some(segment) = segment.to_str() else {
            return Err(invalid_project_root(format!(
                "entry package path {} is not valid UTF-8",
                path.display()
            )));
        };
        segments.push(segment);
    }
    Ok(segments.join("/"))
}

fn invalid_project_root(message: String) -> LoadError {
    LoadError::Manifest(ManifestError::Invalid(message))
}

struct PackageLoader {
    packages: BTreeMap<PackageId, Package>,
    diag: List,
    test_package: Option<PackageId>,
    dependency_index: DependencyIndex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PackageLocation {
    id: PackageId,
    dir: Option<PathBuf>,
    import_path: String,
}

impl PackageLoader {
    fn load_package(&mut self, location: PackageLocation) -> Result<Option<PackageId>, LoadError> {
        if self.packages.contains_key(&location.id) {
            return Ok(Some(location.id));
        }

        let package = match location.dir.as_deref() {
            Some(dir) => self.read_local_package(&location.id, dir)?,
            None => match read_stdlib_package(&location.import_path) {
                Some((package, diagnostics)) => {
                    self.diag.append(&diagnostics);
                    Some(package)
                }
                None => None,
            },
        };
        let Some(package) = package else {
            return Ok(None);
        };

        let id = location.id;
        self.packages.insert(id.clone(), package);
        self.resolve_imports(&id)?;
        Ok(Some(id))
    }

    fn read_local_package(
        &mut self,
        id: &PackageId,
        dir: &Path,
    ) -> Result<Option<Package>, LoadError> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if !id.subpath.is_empty() && err.kind() == std::io::ErrorKind::NotFound => {
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
            if name.ends_with("_test.yar") && self.test_package.as_ref() != Some(id) {
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
            id.clone(),
            package_name,
            false,
            files,
        )))
    }

    fn resolve_imports(&mut self, package_id: &PackageId) -> Result<(), LoadError> {
        let Some(package) = self.packages.get(package_id).cloned() else {
            return Ok(());
        };

        let mut seen_imports = BTreeSet::new();
        let mut import_names = BTreeMap::<String, String>::new();
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
                let import_name = last_import_segment(&decl.path);
                if let Some(existing) =
                    import_names.insert(import_name.to_owned(), decl.path.clone())
                    && existing != decl.path
                {
                    self.diag.add(
                        decl.path_pos.clone(),
                        format!(
                            "import qualifier {import_name:?} is ambiguous between {existing:?} and {:?}",
                            decl.path,
                        ),
                    );
                    continue;
                }

                let location = self
                    .dependency_index
                    .resolve(&package.id.source, &decl.path)?;
                let Some(location) = location else {
                    let message = if stdlib_entries(&decl.path).is_some() {
                        format!(
                            "standard library package {:?} must be imported as {:?}",
                            decl.path,
                            format!("{STDLIB_IMPORT_ROOT}/{}", decl.path),
                        )
                    } else if self
                        .dependency_index
                        .is_reachable_but_undeclared(&package.id.source, &decl.path)
                    {
                        let alias = decl
                            .path
                            .split_once('/')
                            .map_or(decl.path.as_str(), |(alias, _)| alias);
                        format!(
                            "dependency alias {alias:?} is reachable but not declared by this package origin"
                        )
                    } else {
                        format!("import {:?} could not be loaded", decl.path)
                    };
                    self.diag.add(decl.path_pos.clone(), message);
                    continue;
                };
                let selected_stdlib = matches!(location.id.source, SourceId::Stdlib);
                let Some(target_id) = self.load_package(location)? else {
                    let message = if selected_stdlib && decl.path == STDLIB_IMPORT_ROOT {
                        format!(
                            "standard library import {STDLIB_IMPORT_ROOT:?} must name a package as {:?}",
                            format!("{STDLIB_IMPORT_ROOT}/<package>"),
                        )
                    } else if selected_stdlib {
                        format!("standard library package {:?} does not exist", decl.path)
                    } else {
                        format!("import {:?} could not be loaded", decl.path)
                    };
                    self.diag.add(decl.path_pos.clone(), message);
                    continue;
                };
                let Some(target) = self.packages.get(&target_id) else {
                    continue;
                };
                if target.name == "main" {
                    self.diag
                        .add(decl.path_pos.clone(), "cannot import package main");
                    continue;
                }
                let want = import_name;
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
                    target: target_id,
                    decl: decl.clone(),
                });
            }
        }

        if let Some(package) = self.packages.get_mut(package_id) {
            package.imports = imports;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
struct DependencyIndex {
    sources: BTreeMap<SourceId, DependencySource>,
    aliases: BTreeMap<String, SourceId>,
    verified: BTreeSet<SourceId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct DependencySource {
    root: PathBuf,
    self_aliases: BTreeSet<String>,
    dependencies: BTreeMap<String, SourceId>,
    locked: Option<LockedSource>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LockedSource {
    cache_dir: PathBuf,
    git: String,
    commit: String,
    hash: String,
    dependencies: Vec<LockDependency>,
}

impl DependencyIndex {
    fn load(root_dir: &Path) -> Result<Self, LoadError> {
        let mut index = Self::default();
        index.sources.insert(
            SourceId::Entry,
            DependencySource {
                root: root_dir.to_path_buf(),
                ..DependencySource::default()
            },
        );

        let manifest_path = root_dir.join(MANIFEST_FILE);
        match fs::symlink_metadata(&manifest_path) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => {
                return Err(invalid_dependency_graph(format!(
                    "manifest {} is not a regular file",
                    manifest_path.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(index),
            Err(error) => return Err(error.into()),
        }

        let manifest = read_manifest(&manifest_path).map_err(|error| {
            invalid_dependency_graph(format!(
                "reading manifest {}: {error}",
                manifest_path.display()
            ))
        })?;
        let lock_path = root_dir.join(LOCK_FILE);
        let needs_lock = requires_lock(root_dir, &manifest)?;
        let lock_entries = match read_lock_file(&lock_path) {
            Ok(entries) => Some(entries),
            Err(ManifestError::Io(err))
                if err.kind() == std::io::ErrorKind::NotFound && !needs_lock =>
            {
                None
            }
            Err(err) => {
                return Err(LoadError::Manifest(ManifestError::Invalid(format!(
                    "yar.toml and yar.lock do not describe one valid dependency graph (run 'yar lock'): {err}"
                ))));
            }
        };
        if let Some(lock_entries) = lock_entries {
            reconcile_lock_entries(root_dir, &manifest, &lock_entries).map_err(|err| {
                LoadError::Manifest(ManifestError::Invalid(format!(
                    "yar.toml and yar.lock do not describe one valid dependency graph (run 'yar lock'): {err}"
                )))
            })?;
            index.add_locked_sources(lock_entries)?;
        }

        index.add_manifest_scope(&SourceId::Entry, root_dir, &manifest)?;
        Ok(index)
    }

    fn add_locked_sources(
        &mut self,
        lock_entries: Vec<crate::manifest::LockEntry>,
    ) -> Result<(), LoadError> {
        if lock_entries.is_empty() {
            return Ok(());
        }
        let cache_dir = cache_dir_from_env()?;
        for entry in &lock_entries {
            let source = SourceId::Git {
                git: entry.git.clone(),
                commit: entry.commit.clone(),
            };
            self.register_alias(&entry.name, &source)?;

            let mut dependencies = entry.dependencies.clone();
            dependencies.sort_by(|left, right| left.name.cmp(&right.name));
            let locked = LockedSource {
                cache_dir: cache_dir.clone(),
                git: entry.git.clone(),
                commit: entry.commit.clone(),
                hash: entry.hash.clone(),
                dependencies,
            };
            let source_entry = self
                .sources
                .entry(source)
                .or_insert_with(|| DependencySource {
                    root: cache_path(&cache_dir, &entry.git, &entry.commit),
                    locked: Some(locked.clone()),
                    ..DependencySource::default()
                });
            if source_entry.locked.as_ref() != Some(&locked) {
                return Err(invalid_dependency_graph(format!(
                    "yar.lock describes git source {:?} at commit {:?} with conflicting hashes or dependency edges",
                    entry.git, entry.commit
                )));
            }
            source_entry.self_aliases.insert(entry.name.clone());
        }

        for entry in &lock_entries {
            let owner = SourceId::Git {
                git: entry.git.clone(),
                commit: entry.commit.clone(),
            };
            for dependency in &entry.dependencies {
                let Some(target) = self.aliases.get(&dependency.name).cloned() else {
                    return Err(invalid_dependency_graph(format!(
                        "yar.lock dependency {:?} has no package entry",
                        dependency.name
                    )));
                };
                self.insert_binding(&owner, &dependency.name, &target)?;
            }
        }
        Ok(())
    }

    fn add_manifest_scope(
        &mut self,
        owner: &SourceId,
        owner_root: &Path,
        manifest: &crate::manifest::Manifest,
    ) -> Result<(), LoadError> {
        let mut path_sources = BTreeSet::new();
        for (alias, dependency) in &manifest.dependencies {
            let target = if dependency.is_local() {
                let root = dependency_dir(owner_root, dependency);
                let source = SourceId::Path {
                    manifest_path: path_source_identity(dependency),
                };
                let entry =
                    self.sources
                        .entry(source.clone())
                        .or_insert_with(|| DependencySource {
                            root,
                            ..DependencySource::default()
                        });
                entry.self_aliases.insert(alias.clone());
                path_sources.insert(source.clone());
                source
            } else {
                self.locked_source_for_alias(alias).ok_or_else(|| {
                    invalid_dependency_graph(format!(
                        "dependency {alias:?} has no reconciled yar.lock package entry"
                    ))
                })?
            };
            self.register_alias(alias, &target)?;
            self.insert_binding(owner, alias, &target)?;
        }

        for source in path_sources {
            let root = self
                .sources
                .get(&source)
                .map(|entry| entry.root.clone())
                .ok_or_else(|| {
                    invalid_dependency_graph(format!("missing package source scope for {source:?}"))
                })?;
            let manifest_path = root.join(MANIFEST_FILE);
            let manifest = match read_manifest(&manifest_path) {
                Ok(manifest) => manifest,
                Err(ManifestError::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {
                    crate::manifest::Manifest::default()
                }
                Err(err) => return Err(err.into()),
            };
            for (alias, dependency) in &manifest.dependencies {
                if dependency.is_local() {
                    return Err(invalid_dependency_graph(format!(
                        "dependency {alias:?} uses a path inside live dependency {}: path dependencies are supported only in the root yar.toml",
                        root.display()
                    )));
                }
                let target = self.locked_source_for_alias(alias).ok_or_else(|| {
                    invalid_dependency_graph(format!(
                        "dependency {alias:?} has no reconciled yar.lock package entry"
                    ))
                })?;
                self.insert_binding(&source, alias, &target)?;
            }
        }
        Ok(())
    }

    fn locked_source_for_alias(&self, alias: &str) -> Option<SourceId> {
        self.aliases
            .get(alias)
            .filter(|source| matches!(source, SourceId::Git { .. }))
            .cloned()
    }

    fn register_alias(&mut self, alias: &str, source: &SourceId) -> Result<(), LoadError> {
        if let Some(existing) = self.aliases.insert(alias.to_owned(), source.clone())
            && existing != *source
        {
            return Err(invalid_dependency_graph(format!(
                "dependency alias {alias:?} maps to conflicting package sources"
            )));
        }
        Ok(())
    }

    fn insert_binding(
        &mut self,
        owner: &SourceId,
        alias: &str,
        target: &SourceId,
    ) -> Result<(), LoadError> {
        let Some(scope) = self.sources.get_mut(owner) else {
            return Err(invalid_dependency_graph(format!(
                "missing package source scope for {owner:?}"
            )));
        };
        if let Some(existing) = scope.dependencies.insert(alias.to_owned(), target.clone())
            && existing != *target
        {
            return Err(invalid_dependency_graph(format!(
                "dependency alias {alias:?} maps to conflicting package sources"
            )));
        }
        Ok(())
    }

    fn resolve(
        &mut self,
        owner: &SourceId,
        import_path: &str,
    ) -> Result<Option<PackageLocation>, LoadError> {
        if import_path == STDLIB_IMPORT_ROOT {
            return Ok(Some(stdlib_location("")));
        }
        if let Some(subpath) = import_path
            .strip_prefix(STDLIB_IMPORT_ROOT)
            .and_then(|suffix| suffix.strip_prefix('/'))
        {
            return Ok(Some(stdlib_location(subpath)));
        }
        if matches!(owner, SourceId::Stdlib) {
            return Ok(None);
        }

        let Some(scope) = self.sources.get(owner).cloned() else {
            return Err(invalid_dependency_graph(format!(
                "missing package source scope for {owner:?}"
            )));
        };
        let owner_root = self.verify_source(owner, None)?;
        let (alias, subpath) = import_path.split_once('/').unwrap_or((import_path, ""));

        if scope.self_aliases.contains(alias) {
            return Ok(Some(filesystem_location(
                owner.clone(),
                subpath,
                owner_root,
                import_path,
            )));
        }

        let local_dir = package_dir(&owner_root, import_path);
        if local_dir.exists() {
            return Ok(Some(PackageLocation {
                id: PackageId {
                    source: owner.clone(),
                    subpath: import_path.to_owned(),
                },
                dir: Some(local_dir),
                import_path: import_path.to_owned(),
            }));
        }

        if let Some(target) = scope.dependencies.get(alias) {
            let target_root = self.verify_source(target, Some(alias))?;
            return Ok(Some(filesystem_location(
                target.clone(),
                subpath,
                target_root,
                import_path,
            )));
        }

        Ok(None)
    }

    fn verify_source(
        &mut self,
        source: &SourceId,
        selected_alias: Option<&str>,
    ) -> Result<PathBuf, LoadError> {
        let Some(entry) = self.sources.get(source).cloned() else {
            return Err(invalid_dependency_graph(format!(
                "missing package source scope for {source:?}"
            )));
        };
        if self.verified.contains(source) || matches!(source, SourceId::Entry) {
            return Ok(entry.root);
        }
        let alias = selected_alias
            .or_else(|| entry.self_aliases.iter().next().map(String::as_str))
            .unwrap_or("<unknown>");

        if let Some(locked) = entry.locked {
            let dependency_dir = verify_cached_dependency(
                &locked.cache_dir,
                &locked.git,
                &locked.commit,
                &locked.hash,
            )
            .map_err(|err| {
                LoadError::Manifest(ManifestError::Invalid(format!(
                    "dependency {alias:?} cache verification failed: {err}; remove or repair the reported cache path, then run 'yar fetch'"
                )))
            })?;
            verify_locked_dependency_manifest(&dependency_dir, &locked.dependencies).map_err(
                |err| {
                    LoadError::Manifest(ManifestError::Invalid(format!(
                        "dependency {alias:?} manifest does not match yar.lock (run 'yar lock'): {err}"
                    )))
                },
            )?;
        } else {
            match fs::metadata(&entry.root) {
                Ok(metadata) if metadata.is_dir() => {}
                Ok(_) => {
                    return Err(LoadError::Manifest(ManifestError::Invalid(format!(
                        "local dependency {alias:?} path {} is not a directory",
                        entry.root.display()
                    ))));
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    return Err(LoadError::Manifest(ManifestError::Invalid(format!(
                        "local dependency {alias:?} path {} does not exist",
                        entry.root.display()
                    ))));
                }
                Err(err) => return Err(err.into()),
            }
        }
        self.verified.insert(source.clone());
        Ok(entry.root)
    }

    fn is_reachable_but_undeclared(&self, owner: &SourceId, import_path: &str) -> bool {
        let alias = import_path
            .split_once('/')
            .map_or(import_path, |(alias, _)| alias);
        self.aliases.contains_key(alias)
            && self.sources.get(owner).is_some_and(|scope| {
                !scope.dependencies.contains_key(alias) && !scope.self_aliases.contains(alias)
            })
    }
}

fn invalid_dependency_graph(message: String) -> LoadError {
    LoadError::Manifest(ManifestError::Invalid(message))
}

fn package_dir(root: &Path, subpath: &str) -> PathBuf {
    if subpath.is_empty() {
        root.to_path_buf()
    } else {
        root.join(subpath.replace('/', std::path::MAIN_SEPARATOR_STR))
    }
}

fn filesystem_location(
    source: SourceId,
    subpath: &str,
    root: PathBuf,
    import_path: &str,
) -> PackageLocation {
    PackageLocation {
        id: PackageId {
            source,
            subpath: subpath.to_owned(),
        },
        dir: Some(package_dir(&root, subpath)),
        import_path: import_path.to_owned(),
    }
}

fn stdlib_location(import_path: &str) -> PackageLocation {
    PackageLocation {
        id: PackageId {
            source: SourceId::Stdlib,
            subpath: import_path.to_owned(),
        },
        dir: None,
        import_path: import_path.to_owned(),
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

fn path_source_identity(dependency: &Dependency) -> String {
    clean_path(Path::new(&dependency.path))
        .to_string_lossy()
        .into_owned()
}

fn package_from_files(id: PackageId, name: String, stdlib: bool, files: Vec<Program>) -> Package {
    let mut package = Package {
        id,
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
        let (mut program, diags) = parse_file(&synthetic_path, src);
        mark_stdlib_metadata(import_path, &mut program);
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
        package_from_files(
            PackageId {
                source: SourceId::Stdlib,
                subpath: import_path.to_owned(),
            },
            package_name,
            true,
            files,
        ),
        diagnostics,
    ))
}

fn mark_stdlib_metadata(import_path: &str, program: &mut Program) {
    for decl in &mut program.structs {
        decl.opaque = matches!(
            (import_path, decl.name.as_str()),
            ("fs", "File") | ("net", "Conn" | "Listener")
        );
        decl.resource = matches!((import_path, decl.name.as_str()), ("fs", "File"));
    }
    for function in &mut program.functions {
        function.host_intrinsic = match import_path {
            "fs" => matches!(
                function.name.as_str(),
                "read_file"
                    | "write_file"
                    | "read_dir"
                    | "stat"
                    | "mkdir_all"
                    | "remove_all"
                    | "temp_dir"
                    | "open_read_handle"
                    | "open_write_handle"
                    | "read_handle"
                    | "write_handle"
                    | "close_handle"
            ),
            "process" => matches!(function.name.as_str(), "args" | "run" | "run_inherit"),
            "env" => function.name == "lookup",
            "stdio" => function.name == "eprint",
            "net" => matches!(
                function.name.as_str(),
                "listen"
                    | "accept"
                    | "listener_addr"
                    | "close_listener"
                    | "connect"
                    | "read"
                    | "write"
                    | "close"
                    | "local_addr"
                    | "remote_addr"
                    | "set_read_deadline"
                    | "set_write_deadline"
                    | "resolve"
            ),
            _ => false,
        };
    }
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
    package_id: &PackageId,
    visited: &mut BTreeSet<PackageId>,
    active: &mut BTreeSet<PackageId>,
    stack: &mut Vec<PackageId>,
    diagnostics: &mut List,
) {
    if visited.contains(package_id) {
        return;
    }
    let Some(package) = graph.packages.get(package_id) else {
        return;
    };

    visited.insert(package_id.clone());
    active.insert(package_id.clone());
    stack.push(package_id.clone());

    for import in &package.imports {
        let target_id = &import.target;
        if !graph.packages.contains_key(target_id) {
            continue;
        }
        if active.contains(target_id) {
            let cycle = append_cycle(graph, stack, target_id);
            diagnostics.add(
                import.decl.path_pos.clone(),
                format!("import cycle: {}", cycle.join(" -> ")),
            );
            continue;
        }
        visit_package(graph, target_id, visited, active, stack, diagnostics);
    }

    stack.pop();
    active.remove(package_id);
}

fn append_cycle(graph: &PackageGraph, stack: &[PackageId], target: &PackageId) -> Vec<String> {
    let start = stack.iter().position(|id| id == target).unwrap_or(0);
    stack[start..]
        .iter()
        .chain(std::iter::once(target))
        .map(|id| package_display_path(graph, id))
        .collect()
}

fn package_display_path(graph: &PackageGraph, id: &PackageId) -> String {
    if id == &graph.entry {
        return "main".to_owned();
    }
    let subpath = if id.subpath.is_empty() {
        graph
            .packages
            .get(id)
            .map_or("<root>", |package| package.name.as_str())
    } else {
        &id.subpath
    };
    match &id.source {
        SourceId::Entry => subpath.to_owned(),
        SourceId::Path { manifest_path } => format!("{manifest_path}:{subpath}"),
        SourceId::Git { git, commit } => format!("{git}@{commit}:{subpath}"),
        SourceId::Stdlib => format!("<stdlib>/{subpath}"),
    }
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
        assert_eq!(graph.entry, PackageId::default());
        assert!(graph.packages.contains_key(&PackageId::default()));
        assert!(graph.packages.contains_key(&entry_package_id("lexer")));
        assert!(graph.packages.contains_key(&entry_package_id("token")));
        assert_eq!(graph.packages[&PackageId::default()].name, "main");
        assert_eq!(graph.packages[&entry_package_id("lexer")].name, "lexer");
    }

    #[test]
    fn discovers_ancestor_manifest_and_assigns_the_entry_subpath() {
        let root = temp_dir("yar-package-nested-entry");
        let entry = root.join("cmd/app");
        fs::create_dir_all(&entry).unwrap();
        fs::write(
            root.join(MANIFEST_FILE),
            r#"[package]
name = "project"
"#,
        )
        .unwrap();
        fs::write(
            entry.join("main.yar"),
            r#"package main

import "cmd/app"

fn main() i32 {
    return 0
}
"#,
        )
        .unwrap();

        let (graph, diagnostics) = load_package_graph(&entry, false).unwrap();
        let entry_id = entry_package_id("cmd/app");

        assert_eq!(graph.entry, entry_id);
        assert_eq!(graph.packages.len(), 1, "entry directory loaded twice");
        assert_eq!(graph.packages[&graph.entry].name, "main");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("cannot import package main") })
        );
    }

    #[test]
    fn explicit_project_root_overrides_the_nearest_manifest() {
        let root = temp_dir("yar-package-explicit-root");
        let inner = root.join("inner");
        let entry = inner.join("app");
        let dependency = root.join("dep");
        fs::create_dir_all(&entry).unwrap();
        fs::create_dir_all(&dependency).unwrap();
        fs::write(
            root.join(MANIFEST_FILE),
            r#"[package]
name = "outer"

[dependencies]
dep = { path = "dep" }
"#,
        )
        .unwrap();
        fs::write(inner.join(MANIFEST_FILE), "[package]\nname = \"inner\"\n").unwrap();
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

        let (_, automatic_diagnostics) = load_package_graph(&entry, false).unwrap();
        assert!(
            automatic_diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("could not be loaded")),
            "nearest manifest was not selected: {automatic_diagnostics:?}"
        );

        let (graph, diagnostics) =
            load_package_graph_with_root(&entry, Some(&root), false).unwrap();
        assert_eq!(diagnostics, Vec::new());
        assert_eq!(graph.entry, entry_package_id("inner/app"));
    }

    #[test]
    fn rejects_an_explicit_project_root_outside_the_entry_tree() {
        let dir = temp_dir("yar-package-outside-explicit-root");
        let project = dir.join("project");
        let entry = dir.join("entry");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&entry).unwrap();
        fs::write(
            entry.join("main.yar"),
            "package main\n\nfn main() i32 { return 0 }\n",
        )
        .unwrap();

        let error = load_package_graph_with_root(&entry, Some(&project), false)
            .expect_err("accepted an entry outside the explicit project root");

        assert!(
            error.to_string().contains("is outside project root"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_a_non_file_nearest_manifest() {
        let root = temp_dir("yar-package-non-file-manifest");
        let entry = root.join("app");
        fs::create_dir_all(&entry).unwrap();
        fs::create_dir(root.join(MANIFEST_FILE)).unwrap();
        fs::write(
            entry.join("main.yar"),
            "package main\n\nfn main() i32 { return 0 }\n",
        )
        .unwrap();

        let error =
            load_package_graph(&entry, false).expect_err("skipped a non-file manifest candidate");

        assert!(
            error.to_string().contains("is not a regular file"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn loads_reserved_stdlib_graph() {
        let root = repo_root();
        let (graph, diagnostics) =
            load_package_graph(root.join("testdata/stdlib_io/main.yar"), false).unwrap();

        assert_eq!(diagnostics, Vec::new());
        let entry_import = &graph.packages[&PackageId::default()].imports[0];
        assert_eq!(entry_import.name, "fs");
        assert_eq!(entry_import.path, "std/fs");
        assert_eq!(entry_import.target, stdlib_package_id("fs"));
        assert!(graph.packages[&stdlib_package_id("fs")].stdlib);
        assert!(graph.packages[&stdlib_package_id("io")].stdlib);
        assert!(graph.packages[&stdlib_package_id("conv")].stdlib);
    }

    #[test]
    fn marks_only_embedded_stdlib_resources_and_host_intrinsics() {
        let (fs_package, diagnostics) = read_stdlib_package("fs").unwrap();
        assert_eq!(diagnostics, Vec::new());
        assert_eq!(
            fs_package
                .structs
                .iter()
                .filter(|decl| decl.resource)
                .map(|decl| decl.name.as_str())
                .collect::<Vec<_>>(),
            vec!["File"],
        );
        assert!(
            fs_package
                .structs
                .iter()
                .find(|decl| decl.name == "File")
                .is_some_and(|decl| decl.opaque)
        );
        assert!(
            fs_package
                .functions
                .iter()
                .find(|decl| decl.name == "read_file")
                .is_some_and(|decl| decl.host_intrinsic)
        );
        assert!(
            fs_package
                .functions
                .iter()
                .find(|decl| decl.name == "open_read")
                .is_some_and(|decl| !decl.host_intrinsic)
        );

        let (net_package, diagnostics) = read_stdlib_package("net").unwrap();
        assert_eq!(diagnostics, Vec::new());
        assert!(net_package.structs.iter().all(|decl| !decl.resource));
        assert_eq!(
            net_package
                .structs
                .iter()
                .filter(|decl| decl.opaque)
                .map(|decl| decl.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Conn", "Listener"],
        );
        for name in [
            "listen",
            "accept",
            "listener_addr",
            "close_listener",
            "connect",
            "read",
            "write",
            "close",
            "local_addr",
            "remote_addr",
            "set_read_deadline",
            "set_write_deadline",
        ] {
            assert!(
                net_package
                    .functions
                    .iter()
                    .find(|decl| decl.name == name)
                    .is_some_and(|decl| decl.host_intrinsic && !decl.exported),
                "raw network intrinsic {name:?} must remain package-private",
            );
        }
        for name in ["listen_stream", "connect_stream", "resolve"] {
            assert!(
                net_package
                    .functions
                    .iter()
                    .find(|decl| decl.name == name)
                    .is_some_and(|decl| decl.exported),
                "typed network entry point {name:?} must remain exported",
            );
        }
    }

    #[test]
    fn bare_local_package_with_stdlib_name_remains_user_owned() {
        let dir = temp_dir("yar-rust-bare-local-stdlib-name");
        fs::create_dir_all(dir.join("fs")).unwrap();
        fs::write(
            dir.join("main.yar"),
            r#"package main

import "fs"

fn main() i32 {
    return 0
}
"#,
        )
        .unwrap();
        fs::write(
            dir.join("fs").join("fs.yar"),
            r#"package fs

pub struct File {
    handle i64
}

pub fn read_file(path str) str {
    return path
}
"#,
        )
        .unwrap();

        let (graph, diagnostics) = load_package_graph(&dir, false).unwrap();

        assert_eq!(diagnostics, Vec::new());
        assert!(!graph.packages[&entry_package_id("fs")].stdlib);
        assert!(
            graph.packages[&entry_package_id("fs")]
                .structs
                .iter()
                .all(|decl| !decl.resource && !decl.opaque)
        );
        assert!(
            graph.packages[&entry_package_id("fs")]
                .functions
                .iter()
                .all(|decl| !decl.host_intrinsic)
        );
    }

    #[test]
    fn reserved_stdlib_import_ignores_local_package() {
        let dir = temp_dir("yar-rust-reserved-stdlib");
        fs::create_dir_all(dir.join("std").join("fs")).unwrap();
        fs::write(
            dir.join("main.yar"),
            r#"package main

import "std/fs"

fn main() i32 {
    return 0
}
"#,
        )
        .unwrap();
        fs::write(
            dir.join("std").join("fs").join("fs.yar"),
            r#"package fs

pub fn local_value() i32 {
    return 1
}
"#,
        )
        .unwrap();

        let (graph, diagnostics) = load_package_graph(&dir, false).unwrap();

        assert_eq!(diagnostics, Vec::new());
        assert!(graph.packages[&stdlib_package_id("fs")].stdlib);
        assert!(!graph.packages.contains_key(&entry_package_id("std/fs")));
        assert_eq!(
            graph.packages[&PackageId::default()].imports[0].target,
            stdlib_package_id("fs")
        );
    }

    #[test]
    fn bare_stdlib_import_reports_migration_path() {
        let dir = temp_dir("yar-rust-bare-stdlib-migration");
        fs::write(
            dir.join("main.yar"),
            r#"package main

import "strings"

fn main() i32 {
    return 0
}
"#,
        )
        .unwrap();

        let (_, diagnostics) = load_package_graph(&dir, false).unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "standard library package \"strings\" must be imported as \"std/strings\""
        );
        assert_eq!(diagnostics[0].pos.line, 3);
    }

    #[test]
    fn withdrawn_http_stdlib_import_does_not_fall_through() {
        let dir = temp_dir("yar-rust-withdrawn-http-stdlib");
        fs::create_dir_all(dir.join("std").join("http")).unwrap();
        fs::write(
            dir.join("main.yar"),
            r#"package main

import "std/http"

fn main() i32 {
    return 0
}
"#,
        )
        .unwrap();
        fs::write(
            dir.join("std").join("http").join("http.yar"),
            r#"package http

pub fn local_value() i32 {
    return 1
}
"#,
        )
        .unwrap();

        let (graph, diagnostics) = load_package_graph(&dir, false).unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "standard library package \"std/http\" does not exist"
        );
        assert!(!graph.packages.contains_key(&entry_package_id("std/http")));
    }

    #[test]
    fn reserved_stdlib_root_does_not_fall_through() {
        let dir = temp_dir("yar-rust-reserved-stdlib-root");
        fs::create_dir_all(dir.join("std")).unwrap();
        fs::write(
            dir.join("main.yar"),
            r#"package main

import "std"

fn main() i32 {
    return 0
}
"#,
        )
        .unwrap();
        fs::write(
            dir.join("std").join("std.yar"),
            r#"package std

pub fn local_value() i32 {
    return 1
}
"#,
        )
        .unwrap();

        let (graph, diagnostics) = load_package_graph(&dir, false).unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "standard library import \"std\" must name a package as \"std/<package>\""
        );
        assert!(!graph.packages.contains_key(&entry_package_id("std")));
    }

    #[test]
    fn declared_dependency_with_stdlib_name_remains_user_owned() {
        let dir = temp_dir("yar-rust-dependency-stdlib-name");
        let app_dir = dir.join("app");
        let dep_dir = dir.join("dependency");
        fs::create_dir_all(&app_dir).unwrap();
        fs::create_dir_all(&dep_dir).unwrap();
        fs::write(
            app_dir.join("yar.toml"),
            r#"[package]
name = "app"

[dependencies]
fs = { path = "../dependency" }
"#,
        )
        .unwrap();
        fs::write(
            app_dir.join("main.yar"),
            r#"package main

import "fs"

fn main() i32 {
    return fs.local_value()
}
"#,
        )
        .unwrap();
        fs::write(dep_dir.join("yar.toml"), "[package]\nname = \"fs\"\n").unwrap();
        fs::write(
            dep_dir.join("fs.yar"),
            r#"package fs

pub fn local_value() i32 {
    return 0
}
"#,
        )
        .unwrap();

        let (graph, diagnostics) = load_package_graph(&app_dir, false).unwrap();
        let dependency_id = PackageId {
            source: SourceId::Path {
                manifest_path: "../dependency".to_owned(),
            },
            subpath: String::new(),
        };

        assert_eq!(diagnostics, Vec::new());
        assert!(!graph.packages[&dependency_id].stdlib);
        assert_eq!(
            graph.packages[&PackageId::default()].imports[0].target,
            dependency_id
        );
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
        let mathlib_id = PackageId {
            source: SourceId::Path {
                manifest_path: "../deps/vendor".to_owned(),
            },
            subpath: "mathlib".to_owned(),
        };
        assert_eq!(graph.packages[&mathlib_id].name, "mathlib");
        assert_eq!(
            graph.packages[&PackageId::default()].imports[0].path,
            "vendor/mathlib"
        );
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
            if graph
                .packages
                .get(&PackageId::default())
                .is_none_or(|pkg| pkg.name != "main")
            {
                failures.push(format!("{}: missing main entry package", entry.display()));
            }
        }

        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn includes_test_files_only_for_the_entry_package() {
        let root = temp_dir("yar-package-entry-tests-only");
        let entry = root.join("cmd/app");
        let shared = root.join("shared");
        fs::create_dir_all(&entry).unwrap();
        fs::create_dir_all(&shared).unwrap();
        fs::write(
            root.join(MANIFEST_FILE),
            "[package]\nname = \"entry_tests_only\"\n",
        )
        .unwrap();
        fs::write(
            entry.join("main.yar"),
            "package main\n\nimport \"shared\"\n\nfn main() i32 { return shared.value() }\n",
        )
        .unwrap();
        fs::write(
            entry.join("main_test.yar"),
            "package main\n\nfn test_entry() void { return }\n",
        )
        .unwrap();
        fs::write(
            shared.join("shared.yar"),
            "package shared\n\npub fn value() i32 { return 0 }\n",
        )
        .unwrap();
        fs::write(
            shared.join("shared_test.yar"),
            "this imported test file must not be parsed\n",
        )
        .unwrap();

        let (production_graph, production_diagnostics) = load_package_graph(&entry, false).unwrap();
        assert_eq!(production_diagnostics, Vec::new());
        assert!(
            production_graph.packages[&entry_package_id("cmd/app")]
                .files
                .iter()
                .all(|file| !file.package_pos.file.ends_with("_test.yar"))
        );

        let (graph, diagnostics) = load_package_graph(&entry, true).unwrap();

        assert_eq!(diagnostics, Vec::new());
        let entry_files = &graph.packages[&entry_package_id("cmd/app")].files;
        assert!(
            entry_files
                .iter()
                .any(|file| file.package_pos.file.ends_with("main_test.yar"))
        );
        let shared_files = &graph.packages[&entry_package_id("shared")].files;
        assert!(
            shared_files
                .iter()
                .all(|file| !file.package_pos.file.ends_with("_test.yar"))
        );
    }

    #[test]
    fn excludes_test_files_from_path_dependencies() {
        let root = temp_dir("yar-package-path-dependency-tests");
        let app = root.join("app");
        let dependency = root.join("dependency");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&dependency).unwrap();
        fs::write(
            app.join(MANIFEST_FILE),
            r#"[package]
name = "app"

[dependencies]
dependency = { path = "../dependency" }
"#,
        )
        .unwrap();
        fs::write(
            app.join("main.yar"),
            "package main\n\nimport \"dependency\"\n\nfn main() i32 { return dependency.value() }\n",
        )
        .unwrap();
        fs::write(
            app.join("main_test.yar"),
            "package main\n\nfn test_entry() void { return }\n",
        )
        .unwrap();
        fs::write(
            dependency.join(MANIFEST_FILE),
            "[package]\nname = \"dependency\"\n",
        )
        .unwrap();
        fs::write(
            dependency.join("dependency.yar"),
            "package dependency\n\npub fn value() i32 { return 0 }\n",
        )
        .unwrap();
        fs::write(dependency.join("dependency_test.yar"), "package wrong\n").unwrap();

        let (graph, diagnostics) = load_package_graph(&app, true).unwrap();
        let dependency_id = PackageId {
            source: SourceId::Path {
                manifest_path: "../dependency".to_owned(),
            },
            subpath: String::new(),
        };

        assert_eq!(diagnostics, Vec::new());
        assert!(
            graph.packages[&dependency_id]
                .files
                .iter()
                .all(|file| !file.package_pos.file.ends_with("_test.yar"))
        );
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("crate is nested under crates/yar-compiler")
            .to_path_buf()
    }

    fn entry_package_id(subpath: &str) -> PackageId {
        PackageId {
            source: SourceId::Entry,
            subpath: subpath.to_owned(),
        }
    }

    fn stdlib_package_id(subpath: &str) -> PackageId {
        PackageId {
            source: SourceId::Stdlib,
            subpath: subpath.to_owned(),
        }
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
