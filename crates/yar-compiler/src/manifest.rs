use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MANIFEST_FILE: &str = "yar.toml";
pub const LOCK_FILE: &str = "yar.lock";
pub const LOCK_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub package: PackageInfo,
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub git: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub branch: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockFile {
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub package: Vec<LockEntry>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockDependency {
    pub name: String,
    pub git: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub branch: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockEntry {
    pub name: String,
    pub git: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub branch: String,
    pub commit: String,
    pub hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<LockDependency>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedDep {
    pub name: String,
    pub dependency: Dependency,
    pub commit: String,
    pub hash: String,
    pub dir: PathBuf,
    pub dependencies: Vec<LockDependency>,
}

impl Dependency {
    pub fn is_local(&self) -> bool {
        !self.path.is_empty()
    }
}

impl LockEntry {
    pub fn dependency(&self) -> Dependency {
        Dependency {
            git: self.git.clone(),
            tag: self.tag.clone(),
            rev: self.rev.clone(),
            branch: self.branch.clone(),
            path: String::new(),
        }
    }
}

impl LockDependency {
    pub fn new(name: impl Into<String>, dependency: &Dependency) -> Self {
        Self {
            name: name.into(),
            git: dependency.git.clone(),
            tag: dependency.tag.clone(),
            rev: dependency.rev.clone(),
            branch: dependency.branch.clone(),
        }
    }

    pub fn dependency(&self) -> Dependency {
        Dependency {
            git: self.git.clone(),
            tag: self.tag.clone(),
            rev: self.rev.clone(),
            branch: self.branch.clone(),
            path: String::new(),
        }
    }
}

#[derive(Debug)]
pub enum ManifestError {
    Io(std::io::Error),
    TomlDeserialize(toml::de::Error),
    TomlSerialize(toml::ser::Error),
    Invalid(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Io(err) => write!(f, "{err}"),
            ManifestError::TomlDeserialize(err) => write!(f, "parsing yar.toml: {err}"),
            ManifestError::TomlSerialize(err) => write!(f, "encoding yar.toml: {err}"),
            ManifestError::Invalid(message) => f.write_str(message),
        }
    }
}

impl Error for ManifestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            ManifestError::Io(err) => Some(err),
            ManifestError::TomlDeserialize(err) => Some(err),
            ManifestError::TomlSerialize(err) => Some(err),
            ManifestError::Invalid(_) => None,
        }
    }
}

impl From<std::io::Error> for ManifestError {
    fn from(value: std::io::Error) -> Self {
        ManifestError::Io(value)
    }
}

impl From<toml::de::Error> for ManifestError {
    fn from(value: toml::de::Error) -> Self {
        ManifestError::TomlDeserialize(value)
    }
}

impl From<toml::ser::Error> for ManifestError {
    fn from(value: toml::ser::Error) -> Self {
        ManifestError::TomlSerialize(value)
    }
}

pub fn read_manifest(path: impl AsRef<Path>) -> Result<Manifest, ManifestError> {
    let content = fs::read_to_string(path)?;
    parse_manifest(&content)
}

pub fn parse_manifest(content: &str) -> Result<Manifest, ManifestError> {
    let manifest = toml::from_str::<Manifest>(content)?;
    validate_manifest(manifest)
}

pub fn write_manifest(manifest: &Manifest) -> Result<String, ManifestError> {
    validate_manifest(manifest.clone())?;
    Ok(toml::to_string(manifest)?)
}

pub fn read_lock_file(path: impl AsRef<Path>) -> Result<Vec<LockEntry>, ManifestError> {
    let content = fs::read_to_string(path)?;
    parse_lock_file(&content)
}

pub fn parse_lock_file(content: &str) -> Result<Vec<LockEntry>, ManifestError> {
    let raw = toml::from_str::<LockFile>(content)
        .map_err(|err| invalid(format!("parsing yar.lock: {err}")))?;
    if raw.version != Some(LOCK_VERSION) {
        return Err(invalid(format!(
            "yar.lock: unsupported or missing version (expected {LOCK_VERSION}; run 'yar lock')"
        )));
    }
    validate_lock_entries(&raw.package)?;
    Ok(raw.package)
}

pub fn write_lock_file(entries: &[LockEntry]) -> String {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|left, right| left.name.cmp(&right.name));
    for entry in &mut sorted {
        entry
            .dependencies
            .sort_by(|left, right| left.name.cmp(&right.name));
    }

    let mut content =
        format!("# Auto-generated by yar. Do not edit.\n\nversion = {LOCK_VERSION}\n");
    for entry in &sorted {
        content.push('\n');
        content.push_str("[[package]]\n");
        push_toml_string(&mut content, "name", &entry.name);
        push_toml_string(&mut content, "git", &entry.git);
        if !entry.tag.is_empty() {
            push_toml_string(&mut content, "tag", &entry.tag);
        }
        if !entry.rev.is_empty() {
            push_toml_string(&mut content, "rev", &entry.rev);
        }
        if !entry.branch.is_empty() {
            push_toml_string(&mut content, "branch", &entry.branch);
        }
        push_toml_string(&mut content, "commit", &entry.commit);
        push_toml_string(&mut content, "hash", &entry.hash);
        for dependency in &entry.dependencies {
            content.push_str("\n[[package.dependencies]]\n");
            push_toml_string(&mut content, "name", &dependency.name);
            push_toml_string(&mut content, "git", &dependency.git);
            if !dependency.tag.is_empty() {
                push_toml_string(&mut content, "tag", &dependency.tag);
            }
            if !dependency.rev.is_empty() {
                push_toml_string(&mut content, "rev", &dependency.rev);
            }
            if !dependency.branch.is_empty() {
                push_toml_string(&mut content, "branch", &dependency.branch);
            }
        }
    }
    content
}

pub fn cache_path(cache_dir: impl AsRef<Path>, git_url: &str, commit: &str) -> std::path::PathBuf {
    cache_bucket_path(cache_dir.as_ref(), git_url).join(commit)
}

fn cache_bucket_path(cache_dir: &Path, git_url: &str) -> PathBuf {
    let digest = Sha256::digest(git_url.as_bytes());
    let url_hash = hex_lower(&digest);
    cache_dir.join(&url_hash[..16])
}

pub fn cache_dir_from_env() -> Result<std::path::PathBuf, ManifestError> {
    if let Some(cache) = std::env::var_os("YAR_CACHE") {
        return Ok(cache.into());
    }
    let Some(home) = std::env::var_os("HOME") else {
        return Err(invalid("determining cache directory: HOME is not set"));
    };
    let home = std::path::PathBuf::from(home);
    let base = if cfg!(target_os = "macos") {
        home.join("Library").join("Caches")
    } else {
        home.join(".cache")
    };
    Ok(base.join("yar").join("deps"))
}

pub fn is_cached(cache_dir: impl AsRef<Path>, git_url: &str, commit: &str) -> bool {
    if !is_lower_hex(commit, 40) {
        return false;
    }
    cache_path(cache_dir, git_url, commit).is_dir()
}

pub fn fetch_locked_dependency<F>(
    cache_dir: impl AsRef<Path>,
    dependency: &Dependency,
    commit: &str,
    expected_hash: &str,
    validate: F,
) -> Result<PathBuf, ManifestError>
where
    F: Fn(&Path) -> Result<(), ManifestError>,
{
    validate_cache_key(commit, expected_hash)?;
    let cache_dir = cache_dir.as_ref();
    let cache_bucket = prepare_cache_bucket(cache_dir, &dependency.git)?;
    let dest_dir = cache_bucket.join(commit);
    match fs::symlink_metadata(&dest_dir) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            verify_hash(&dest_dir, expected_hash)?;
            validate(&dest_dir)?;
            return Ok(dest_dir);
        }
        Ok(_) => {
            return Err(invalid(format!(
                "dependency cache path {} is not a directory",
                dest_dir.display()
            )));
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(with_context(
                format!("inspecting dependency cache {}", dest_dir.display()),
                err,
            ));
        }
    }

    let tmp_dir = TempDir::new(cache_dir, "fetch")?;
    let clone_dir = tmp_dir.path().join("repo");
    git_clone(dependency, &clone_dir)?;

    let actual_commit = git_rev_parse(&clone_dir)?;
    if !commit.is_empty() && actual_commit != commit {
        return Err(invalid(format!(
            "commit mismatch for {}: expected {}, got {}",
            dependency.git, commit, actual_commit
        )));
    }

    fs::remove_dir_all(clone_dir.join(".git"))
        .map_err(|err| with_context(format!("removing .git from {}", clone_dir.display()), err))?;
    verify_hash(&clone_dir, expected_hash)?;
    validate(&clone_dir)?;
    fs::rename(&clone_dir, &dest_dir)
        .map_err(|err| with_context("moving dependency to cache", err))?;
    Ok(dest_dir)
}

pub fn fetch_and_resolve_commit(
    cache_dir: impl AsRef<Path>,
    dependency: &Dependency,
) -> Result<(String, String), ManifestError> {
    let cache_dir = cache_dir.as_ref();
    let cache_bucket = prepare_cache_bucket(cache_dir, &dependency.git)?;
    let tmp_dir = TempDir::new(cache_dir, "fetch")?;
    let clone_dir = tmp_dir.path().join("repo");
    git_clone(dependency, &clone_dir)?;

    let commit = git_rev_parse(&clone_dir)?;
    if !is_lower_hex(&commit, 40) {
        return Err(invalid(format!(
            "git returned invalid commit {commit:?}: expected 40 lowercase hex characters"
        )));
    }
    fs::remove_dir_all(clone_dir.join(".git"))
        .map_err(|err| with_context(format!("removing .git from {}", clone_dir.display()), err))?;
    let hash = hash_dir(&clone_dir)?;

    let dest_dir = cache_bucket.join(&commit);
    match fs::symlink_metadata(&dest_dir) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            verify_hash(&dest_dir, &hash).map_err(|err| {
                invalid(format!(
                    "cached dependency {} differs from freshly resolved content: {err}",
                    dest_dir.display()
                ))
            })?;
        }
        Ok(_) => {
            return Err(invalid(format!(
                "dependency cache path {} is not a directory",
                dest_dir.display()
            )));
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::rename(&clone_dir, &dest_dir)
                .map_err(|err| with_context("moving dependency to cache", err))?;
        }
        Err(err) => {
            return Err(with_context(
                format!("inspecting dependency cache {}", dest_dir.display()),
                err,
            ));
        }
    }
    Ok((commit, hash))
}

pub fn hash_dir(dir: impl AsRef<Path>) -> Result<String, ManifestError> {
    let dir = dir.as_ref();
    let metadata = fs::symlink_metadata(dir)
        .map_err(|err| with_context(format!("hashing directory {}", dir.display()), err))?;
    if !metadata.file_type().is_dir() {
        return Err(invalid(format!(
            "hashing directory {}: root is not a directory",
            dir.display()
        )));
    }
    let mut entries = Vec::new();
    collect_hash_entries(dir, dir, &mut entries)
        .map_err(|err| with_context(format!("hashing directory {}", dir.display()), err))?;
    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(format!("file {} {}\n", entry.relative_path, entry.content.len()).as_bytes());
        hasher.update(&entry.content);
    }
    Ok(format!("sha256:{}", hex_lower(&hasher.finalize())))
}

pub fn verify_hash(dir: impl AsRef<Path>, expected: &str) -> Result<(), ManifestError> {
    if !valid_sha256_hash(expected) {
        return Err(invalid(format!(
            "invalid hash format for {}: expected sha256: followed by 64 lowercase hex characters, got {expected:?}",
            dir.as_ref().display()
        )));
    }
    let actual = hash_dir(dir.as_ref())?;
    if actual != expected {
        return Err(invalid(format!(
            "hash mismatch for {}: expected {expected}, got {actual}",
            dir.as_ref().display()
        )));
    }
    Ok(())
}

pub fn verify_cached_dependency(
    cache_dir: impl AsRef<Path>,
    git_url: &str,
    commit: &str,
    expected_hash: &str,
) -> Result<PathBuf, ManifestError> {
    validate_cache_key(commit, expected_hash)?;
    let cache_dir = cache_dir.as_ref();
    require_cache_directory(cache_dir, "dependency cache root")?;
    let cache_bucket = cache_bucket_path(cache_dir, git_url);
    require_cache_directory(&cache_bucket, "dependency cache bucket")?;
    let dependency_dir = cache_bucket.join(commit);
    require_cache_directory(&dependency_dir, "dependency cache entry")?;
    verify_hash(&dependency_dir, expected_hash)?;
    Ok(dependency_dir)
}

pub fn resolve_dependencies(
    root_dir: impl AsRef<Path>,
    manifest: &Manifest,
    cache_dir: impl AsRef<Path>,
) -> Result<Vec<ResolvedDep>, ManifestError> {
    let mut resolver = Resolver {
        cache_dir: cache_dir.as_ref().to_path_buf(),
        resolved: BTreeMap::new(),
        visiting: BTreeSet::new(),
        edges: BTreeMap::new(),
    };
    for (alias, dependency) in &manifest.dependencies {
        resolver.resolve(alias, dependency, root_dir.as_ref(), &[], None)?;
    }
    for (name, dependency) in &mut resolver.resolved {
        if dependency.dependency.is_local() {
            continue;
        }
        dependency.dependencies = resolver
            .edges
            .remove(name)
            .unwrap_or_default()
            .into_iter()
            .map(|(alias, dependency)| LockDependency::new(alias, &dependency))
            .collect();
    }
    Ok(resolver.resolved.into_values().collect())
}

pub fn to_lock_entries(resolved: &[ResolvedDep]) -> Vec<LockEntry> {
    resolved
        .iter()
        .filter(|dependency| !dependency.dependency.is_local())
        .map(|resolved| LockEntry {
            name: resolved.name.clone(),
            git: resolved.dependency.git.clone(),
            tag: resolved.dependency.tag.clone(),
            rev: resolved.dependency.rev.clone(),
            branch: resolved.dependency.branch.clone(),
            commit: resolved.commit.clone(),
            hash: resolved.hash.clone(),
            dependencies: resolved.dependencies.clone(),
        })
        .collect()
}

pub fn valid_alias(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first != '_' && !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn validate_manifest(manifest: Manifest) -> Result<Manifest, ManifestError> {
    if manifest.package.name.is_empty() {
        return Err(invalid("yar.toml: [package] name is required"));
    }
    if !valid_alias(&manifest.package.name) {
        return Err(invalid(format!(
            "yar.toml: invalid package name {:?}",
            manifest.package.name
        )));
    }
    for (alias, dependency) in &manifest.dependencies {
        if !valid_alias(alias) {
            return Err(invalid(format!(
                "yar.toml: invalid dependency alias {alias:?}"
            )));
        }
        validate_dependency(alias, dependency)?;
    }
    Ok(manifest)
}

pub(crate) fn validate_lock_entries(entries: &[LockEntry]) -> Result<(), ManifestError> {
    let mut names = BTreeSet::new();
    for (index, entry) in entries.iter().enumerate() {
        validate_lock_entry(index, entry)?;
        if !names.insert(entry.name.as_str()) {
            return Err(invalid(format!(
                "yar.lock: duplicate package name {:?}",
                entry.name
            )));
        }
    }
    Ok(())
}

fn validate_lock_entry(index: usize, entry: &LockEntry) -> Result<(), ManifestError> {
    let label = if entry.name.is_empty() {
        format!("package[{index}]")
    } else {
        format!("package {:?}", entry.name)
    };
    if !valid_alias(&entry.name) {
        return Err(invalid(format!(
            "yar.lock: {label} has invalid name {:?}",
            entry.name
        )));
    }
    validate_lock_source(&label, &entry.dependency())?;
    if !is_lower_hex(&entry.commit, 40) {
        return Err(invalid(format!(
            "yar.lock: {label} commit must be 40 lowercase hex characters"
        )));
    }
    if !valid_sha256_hash(&entry.hash) {
        return Err(invalid(format!(
            "yar.lock: {label} hash must be sha256: followed by 64 lowercase hex characters"
        )));
    }
    let mut dependencies = BTreeSet::new();
    for dependency in &entry.dependencies {
        if !valid_alias(&dependency.name) {
            return Err(invalid(format!(
                "yar.lock: {label} has invalid dependency name {:?}",
                dependency.name
            )));
        }
        if !dependencies.insert(dependency.name.as_str()) {
            return Err(invalid(format!(
                "yar.lock: {label} has duplicate dependency {:?}",
                dependency.name
            )));
        }
        validate_lock_source(
            &format!("{label} dependency {:?}", dependency.name),
            &dependency.dependency(),
        )?;
    }
    Ok(())
}

fn validate_lock_source(label: &str, dependency: &Dependency) -> Result<(), ManifestError> {
    if !valid_git_url(&dependency.git) {
        return Err(invalid(format!(
            "yar.lock: {label} has unsupported git URL scheme {:?}",
            dependency.git
        )));
    }
    let version_count = [
        !dependency.tag.is_empty(),
        !dependency.rev.is_empty(),
        !dependency.branch.is_empty(),
    ]
    .into_iter()
    .filter(|set| *set)
    .count();
    if version_count != 1 {
        return Err(invalid(format!(
            "yar.lock: {label} must specify exactly one of tag, rev, or branch"
        )));
    }
    Ok(())
}

fn valid_sha256_hash(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|digest| is_lower_hex(digest, 64))
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_cache_key(commit: &str, expected_hash: &str) -> Result<(), ManifestError> {
    if !is_lower_hex(commit, 40) {
        return Err(invalid(format!(
            "invalid dependency commit {commit:?}: expected 40 lowercase hex characters"
        )));
    }
    if !valid_sha256_hash(expected_hash) {
        return Err(invalid(format!(
            "invalid dependency hash {expected_hash:?}: expected sha256: followed by 64 lowercase hex characters"
        )));
    }
    Ok(())
}

fn prepare_cache_bucket(cache_dir: &Path, git_url: &str) -> Result<PathBuf, ManifestError> {
    prepare_cache_directory(cache_dir, "dependency cache root")?;
    let cache_bucket = cache_bucket_path(cache_dir, git_url);
    prepare_cache_directory(&cache_bucket, "dependency cache bucket")?;
    Ok(cache_bucket)
}

fn prepare_cache_directory(path: &Path, label: &str) -> Result<(), ManifestError> {
    match fs::symlink_metadata(path) {
        Ok(_) => require_cache_directory(path, label),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path)
                .map_err(|err| with_context(format!("creating {label} {}", path.display()), err))?;
            require_cache_directory(path, label)
        }
        Err(err) => Err(with_context(
            format!("inspecting {label} {}", path.display()),
            err,
        )),
    }
}

fn require_cache_directory(path: &Path, label: &str) -> Result<(), ManifestError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|err| with_context(format!("inspecting {label} {}", path.display()), err))?;
    if metadata.file_type().is_dir() {
        return Ok(());
    }
    Err(invalid(format!(
        "{label} {} is not a real directory",
        path.display()
    )))
}

fn validate_dependency(alias: &str, dependency: &Dependency) -> Result<(), ManifestError> {
    if dependency.is_local() {
        if !dependency.git.is_empty()
            || !dependency.tag.is_empty()
            || !dependency.rev.is_empty()
            || !dependency.branch.is_empty()
        {
            return Err(invalid(format!(
                "yar.toml: dependency {alias:?} has both path and git fields"
            )));
        }
        return Ok(());
    }
    if dependency.git.is_empty() {
        return Err(invalid(format!(
            "yar.toml: dependency {alias:?} must specify git or path"
        )));
    }
    if !valid_git_url(&dependency.git) {
        return Err(invalid(format!(
            "yar.toml: dependency {alias:?} has unsupported git URL scheme {:?}",
            dependency.git
        )));
    }
    let version_count = [
        !dependency.tag.is_empty(),
        !dependency.rev.is_empty(),
        !dependency.branch.is_empty(),
    ]
    .into_iter()
    .filter(|set| *set)
    .count();
    if version_count != 1 {
        return Err(invalid(format!(
            "yar.toml: dependency {alias:?} must specify exactly one of tag, rev, or branch"
        )));
    }
    Ok(())
}

pub fn valid_git_url(url: &str) -> bool {
    ["https://", "http://", "ssh://", "git://", "git@"]
        .iter()
        .any(|prefix| url.starts_with(prefix))
}

fn invalid(message: impl Into<String>) -> ManifestError {
    ManifestError::Invalid(message.into())
}

fn with_context(context: impl Into<String>, err: io::Error) -> ManifestError {
    invalid(format!("{}: {err}", context.into()))
}

fn git_clone(dependency: &Dependency, destination: &Path) -> Result<(), ManifestError> {
    if !valid_git_url(&dependency.git) {
        return Err(invalid(format!(
            "unsupported git URL scheme: {}",
            dependency.git
        )));
    }

    let mut args = Vec::new();
    args.push("clone".to_string());
    match () {
        _ if !dependency.tag.is_empty() => {
            args.push("--depth=1".to_string());
            args.push("--branch".to_string());
            args.push(dependency.tag.clone());
        }
        _ if !dependency.branch.is_empty() => {
            args.push("--depth=1".to_string());
            args.push("--branch".to_string());
            args.push(dependency.branch.clone());
        }
        _ => {}
    }
    args.push(dependency.git.clone());
    args.push(destination.display().to_string());

    run_git(&args, format!("git clone {}", dependency.git))?;

    if !dependency.rev.is_empty() {
        run_git(
            &[
                "-C".to_string(),
                destination.display().to_string(),
                "checkout".to_string(),
                dependency.rev.clone(),
            ],
            format!("git checkout {}", dependency.rev),
        )?;
    }
    Ok(())
}

fn git_rev_parse(dir: &Path) -> Result<String, ManifestError> {
    let output = Command::new("git")
        .args(["-C", &dir.display().to_string(), "rev-parse", "HEAD"])
        .output()?;
    if !output.status.success() {
        return Err(invalid(format!(
            "git rev-parse HEAD in {} failed: {}\n{}",
            dir.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git(args: &[String], context: String) -> Result<(), ManifestError> {
    let output = Command::new("git").args(args).output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(invalid(format!(
        "{context} failed: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    )))
}

struct HashEntry {
    relative_path: String,
    content: Vec<u8>,
}

fn collect_hash_entries(root: &Path, dir: &Path, entries: &mut Vec<HashEntry>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_hash_entries(root, &path, entries)?;
            continue;
        }
        if !file_type.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "dependency tree entry {} is not a regular file or directory",
                    path.display()
                ),
            ));
        }
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        entries.push(HashEntry {
            relative_path,
            content: fs::read(&path)?,
        });
    }
    Ok(())
}

struct Resolver {
    cache_dir: PathBuf,
    resolved: BTreeMap<String, ResolvedDep>,
    visiting: BTreeSet<String>,
    edges: BTreeMap<String, BTreeMap<String, Dependency>>,
}

impl Resolver {
    fn resolve(
        &mut self,
        alias: &str,
        dependency: &Dependency,
        parent_dir: &Path,
        chain: &[String],
        locked_parent: Option<&str>,
    ) -> Result<(), ManifestError> {
        if dependency.is_local() && !chain.is_empty() {
            return Err(invalid(format!(
                "dependency {alias:?} uses a path inside dependency {}: path dependencies are supported only in the root yar.toml",
                format_chain(chain)
            )));
        }
        if let Some(parent) = locked_parent {
            self.record_edge(parent, alias, dependency)?;
        }

        if self.visiting.contains(alias) {
            return Err(invalid(format!(
                "dependency cycle detected: {} -> {}",
                format_chain(chain),
                alias
            )));
        }

        if let Some(existing) = self.resolved.get(alias) {
            if existing.dependency != *dependency {
                return Err(invalid(format!(
                    "dependency conflict for {alias:?}: {} (from {}) vs {}",
                    format_dependency_source(&existing.dependency),
                    format_chain(chain),
                    format_dependency_source(dependency)
                )));
            }
            return Ok(());
        }

        self.visiting.insert(alias.to_string());
        let result = if dependency.is_local() {
            self.resolve_local(alias, dependency, parent_dir, chain)
        } else {
            match self.resolve_git(alias, dependency) {
                Ok(resolved) => {
                    let dir = resolved.dir.clone();
                    self.resolved.insert(alias.to_string(), resolved);
                    self.resolve_transitive(alias, &dir, chain, Some(alias))
                }
                Err(err) => Err(err),
            }
        };
        self.visiting.remove(alias);
        result
    }

    fn resolve_local(
        &mut self,
        alias: &str,
        dependency: &Dependency,
        parent_dir: &Path,
        chain: &[String],
    ) -> Result<(), ManifestError> {
        let mut dir = PathBuf::from(&dependency.path);
        if !dir.is_absolute() {
            dir = parent_dir.join(dir);
        }
        dir = clean_path(&dir);

        self.resolved.insert(
            alias.to_string(),
            ResolvedDep {
                name: alias.to_string(),
                dependency: dependency.clone(),
                commit: String::new(),
                hash: String::new(),
                dir: dir.clone(),
                dependencies: Vec::new(),
            },
        );
        self.resolve_transitive(alias, &dir, chain, None)
    }

    fn resolve_git(
        &self,
        alias: &str,
        dependency: &Dependency,
    ) -> Result<ResolvedDep, ManifestError> {
        let (commit, hash) = fetch_and_resolve_commit(&self.cache_dir, dependency)
            .map_err(|err| invalid(format!("resolving {alias}: {err}")))?;
        let dir = cache_path(&self.cache_dir, &dependency.git, &commit);
        Ok(ResolvedDep {
            name: alias.to_string(),
            dependency: dependency.clone(),
            commit,
            hash,
            dir,
            dependencies: Vec::new(),
        })
    }

    fn record_edge(
        &mut self,
        parent: &str,
        alias: &str,
        dependency: &Dependency,
    ) -> Result<(), ManifestError> {
        let edges = self.edges.entry(parent.to_string()).or_default();
        if let Some(existing) = edges.get(alias) {
            if existing != dependency {
                return Err(invalid(format!(
                    "dependency conflict for {alias:?}: {} vs {}",
                    format_dependency_source(existing),
                    format_dependency_source(dependency)
                )));
            }
            return Ok(());
        }
        edges.insert(alias.to_string(), dependency.clone());
        Ok(())
    }

    fn resolve_transitive(
        &mut self,
        alias: &str,
        dir: &Path,
        chain: &[String],
        locked_parent: Option<&str>,
    ) -> Result<(), ManifestError> {
        let manifest_path = dir.join(MANIFEST_FILE);
        let manifest = match read_manifest(&manifest_path) {
            Ok(manifest) => manifest,
            Err(ManifestError::Io(err)) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };

        let mut next_chain = chain.to_vec();
        next_chain.push(alias.to_string());
        for (child_alias, dependency) in &manifest.dependencies {
            self.resolve(child_alias, dependency, dir, &next_chain, locked_parent)?;
        }
        Ok(())
    }
}

pub(crate) fn clean_path(path: &Path) -> PathBuf {
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if matches!(
                    cleaned.components().next_back(),
                    Some(std::path::Component::Normal(_))
                ) {
                    cleaned.pop();
                } else if !cleaned.has_root() {
                    cleaned.push(component.as_os_str());
                }
            }
            _ => cleaned.push(component.as_os_str()),
        }
    }
    cleaned
}

fn format_chain(chain: &[String]) -> String {
    if chain.is_empty() {
        return "root".to_string();
    }
    let mut parts = Vec::with_capacity(chain.len() + 1);
    parts.push("root".to_string());
    parts.extend(chain.iter().cloned());
    parts.join(" -> ")
}

fn format_dependency_source(dependency: &Dependency) -> String {
    if dependency.is_local() {
        return format!("path {:?}", dependency.path);
    }
    if !dependency.tag.is_empty() {
        return format!("{} tag {:?}", dependency.git, dependency.tag);
    }
    if !dependency.rev.is_empty() {
        return format!("{} rev {:?}", dependency.git, dependency.rev);
    }
    format!("{} branch {:?}", dependency.git, dependency.branch)
}

fn push_toml_string(content: &mut String, key: &str, value: &str) {
    content.push_str(key);
    content.push_str(" = ");
    content.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => content.push_str("\\\\"),
            '"' => content.push_str("\\\""),
            '\n' => content.push_str("\\n"),
            '\r' => content.push_str("\\r"),
            '\t' => content.push_str("\\t"),
            _ => content.push(ch),
        }
    }
    content.push_str("\"\n");
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(parent: &Path, prefix: &str) -> Result<Self, ManifestError> {
        fs::create_dir_all(parent)?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = parent.join(format!("{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(hex_digit(byte >> 4));
        out.push(hex_digit(byte & 0x0f));
    }
    out
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        10..=15 => char::from(b'a' + value - 10),
        _ => unreachable!("nibble is always <= 15"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest() {
        let input = r#"
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
http = { git = "https://github.com/user/yar-http.git", tag = "v0.3.1" }
json = { git = "https://github.com/user/yar-json.git", rev = "a1b2c3d" }
logger = { git = "https://github.com/user/yar-logger.git", branch = "main" }
local_lib = { path = "../my-local-lib" }
"#;

        let manifest = parse_manifest(input).unwrap();

        assert_eq!(manifest.package.name, "myapp");
        assert_eq!(manifest.package.version, "0.1.0");
        assert_eq!(manifest.dependencies.len(), 4);
        assert_eq!(
            manifest.dependencies["http"].git,
            "https://github.com/user/yar-http.git"
        );
        assert_eq!(manifest.dependencies["http"].tag, "v0.3.1");
        assert!(!manifest.dependencies["http"].is_local());
        assert_eq!(manifest.dependencies["json"].rev, "a1b2c3d");
        assert_eq!(manifest.dependencies["logger"].branch, "main");
        assert!(manifest.dependencies["local_lib"].is_local());
        assert_eq!(manifest.dependencies["local_lib"].path, "../my-local-lib");
    }

    #[test]
    fn rejects_invalid_manifest_shapes() {
        for input in [
            r#"[package]
version = "0.1.0"
"#,
            r#"[package]
name = "myapp"

[dependencies]
"my-lib" = { git = "https://example.com/repo.git", tag = "v1.0.0" }
"#,
            r#"[package]
name = "myapp"

[dependencies]
lib = { git = "https://example.com/repo.git", tag = "v1.0.0", rev = "abc123" }
"#,
            r#"[package]
name = "myapp"

[dependencies]
lib = { path = "../lib", git = "https://example.com/repo.git" }
"#,
            r#"[package]
name = "myapp"

[dependencies]
lib = { git = "https://example.com/repo.git" }
"#,
        ] {
            assert!(parse_manifest(input).is_err(), "accepted {input}");
        }
    }

    #[test]
    fn parses_manifest_without_dependencies() {
        let manifest = parse_manifest(
            r#"[package]
name = "myapp"
"#,
        )
        .unwrap();

        assert_eq!(manifest.package.name, "myapp");
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn validates_aliases() {
        for (name, want) in [
            ("http", true),
            ("my_lib", true),
            ("Lib", true),
            ("lib2", true),
            ("_private", true),
            ("", false),
            ("2lib", false),
            ("my-lib", false),
            ("my.lib", false),
            ("my lib", false),
        ] {
            assert_eq!(valid_alias(name), want, "{name}");
        }
    }

    #[test]
    fn writes_manifest_round_trip() {
        let mut dependencies = BTreeMap::new();
        dependencies.insert(
            "http".to_string(),
            Dependency {
                git: "https://github.com/user/yar-http.git".to_string(),
                tag: "v0.3.1".to_string(),
                ..Dependency::default()
            },
        );
        let manifest = Manifest {
            package: PackageInfo {
                name: "myapp".to_string(),
                version: "0.1.0".to_string(),
            },
            dependencies,
        };

        let content = write_manifest(&manifest).unwrap();
        let round_trip = parse_manifest(&content).unwrap();

        assert_eq!(round_trip.package.name, "myapp");
        assert_eq!(round_trip.dependencies.len(), 1);
        assert_eq!(round_trip.dependencies["http"].tag, "v0.3.1");
    }

    #[test]
    fn parses_lock_file() {
        let entries = parse_lock_file(
            r#"
# Auto-generated by yar. Do not edit.
version = 1

[[package]]
name = "http"
git = "https://github.com/user/yar-http.git"
tag = "v0.3.1"
commit = "abc123def456789012345678901234567890abcd"
hash = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"

[[package]]
name = "json"
git = "https://github.com/user/yar-json.git"
rev = "def789"
commit = "def789abc123456012345678901234567890abcd"
hash = "sha256:1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
"#,
        )
        .unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "http");
        assert_eq!(entries[0].tag, "v0.3.1");
        assert_eq!(entries[1].rev, "def789");
    }

    #[test]
    fn writes_lock_file_sorted_with_header() {
        let entries = [
            LockEntry {
                name: "json".to_string(),
                git: "https://github.com/user/yar-json.git".to_string(),
                rev: "def789".to_string(),
                commit: "def789abc123456012345678901234567890abcd".to_string(),
                hash: format!("sha256:{}", "1".repeat(64)),
                ..LockEntry::default()
            },
            LockEntry {
                name: "http".to_string(),
                git: "https://github.com/user/yar-http.git".to_string(),
                tag: "v0.3.1".to_string(),
                commit: "abc123def456789012345678901234567890abcd".to_string(),
                hash: format!("sha256:{}", "2".repeat(64)),
                dependencies: vec![
                    LockDependency::new(
                        "zeta",
                        &Dependency {
                            git: "https://example.com/zeta.git".to_string(),
                            tag: "v1".to_string(),
                            ..Dependency::default()
                        },
                    ),
                    LockDependency::new(
                        "alpha",
                        &Dependency {
                            git: "https://example.com/alpha.git".to_string(),
                            rev: "abc123".to_string(),
                            ..Dependency::default()
                        },
                    ),
                ],
                ..LockEntry::default()
            },
        ];

        let content = write_lock_file(&entries);

        assert!(content.starts_with("# Auto-generated by yar. Do not edit.\n\n"));
        assert!(content.contains("\nversion = 1\n"));
        assert!(
            content.find("name = \"http\"").unwrap() < content.find("name = \"json\"").unwrap()
        );
        let round_trip = parse_lock_file(&content).unwrap();
        assert_eq!(round_trip[0].name, "http");
        assert_eq!(round_trip[1].name, "json");
        assert_eq!(
            round_trip[0]
                .dependencies
                .iter()
                .map(|dependency| dependency.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "zeta"]
        );
    }

    #[test]
    fn rejects_legacy_duplicate_and_ambiguous_lock_metadata() {
        let commit = "1".repeat(40);
        let hash = format!("sha256:{}", "a".repeat(64));
        let package = format!(
            r#"[[package]]
name = "http"
git = "https://example.com/http.git"
tag = "v1"
commit = "{commit}"
hash = "{hash}"
"#
        );
        let duplicate = format!("version = 1\n\n{package}\n{package}");
        let invalid_alias = format!(
            "version = 1\n\n{}",
            package.replacen("name = \"http\"", "name = \"bad-name\"", 1)
        );
        let invalid_url = format!(
            "version = 1\n\n{}",
            package.replacen(
                "git = \"https://example.com/http.git\"",
                "git = \"../http\"",
                1,
            )
        );
        let missing_ref = format!(
            "version = 1\n\n{}",
            package.replacen("tag = \"v1\"\n", "", 1)
        );
        let multiple_refs = format!(
            "version = 1\n\n{}",
            package.replacen("tag = \"v1\"", "tag = \"v1\"\nbranch = \"v1\"", 1)
        );
        let duplicate_edge = format!(
            r#"version = 1

{package}
[[package.dependencies]]
name = "child"
git = "https://example.com/child.git"
tag = "v1"

[[package.dependencies]]
name = "child"
git = "https://example.com/child.git"
tag = "v1"
"#
        );
        let unknown_field = format!("version = 1\n\n{package}dependecies = []\n");

        for input in [
            package,
            "version = 2\n\n".to_string(),
            duplicate,
            invalid_alias,
            invalid_url,
            missing_ref,
            multiple_refs,
            duplicate_edge,
            unknown_field,
        ] {
            assert!(parse_lock_file(&input).is_err(), "accepted {input}");
        }
    }

    #[test]
    fn rejects_lock_entries_with_missing_required_fields() {
        for input in [
            r#"version = 1

[[package]]
git = "https://example.com/repo.git"
commit = "abc123"
hash = "sha256:abc"
"#,
            r#"version = 1

[[package]]
name = "lib"
commit = "abc123"
hash = "sha256:abc"
"#,
            r#"version = 1

[[package]]
name = "lib"
git = "https://example.com/repo.git"
hash = "sha256:abc"
"#,
            r#"version = 1

[[package]]
name = "lib"
git = "https://example.com/repo.git"
commit = "1111111111111111111111111111111111111111"
"#,
        ] {
            assert!(parse_lock_file(input).is_err(), "accepted {input}");
        }
    }

    #[test]
    fn rejects_lock_values_that_cannot_be_cache_keys() {
        let valid_hash = format!("sha256:{}", "a".repeat(64));
        for input in [
            format!(
                r#"version = 1

[[package]]
name = "http"
git = "https://example.com/http.git"
tag = "v1"
commit = "abc123"
hash = "{valid_hash}"
"#,
            ),
            format!(
                r#"version = 1

[[package]]
name = "http"
git = "https://example.com/http.git"
tag = "v1"
commit = "{}"
hash = "{valid_hash}"
"#,
                "A".repeat(40),
            ),
            format!(
                r#"version = 1

[[package]]
name = "http"
git = "https://example.com/http.git"
tag = "v1"
commit = "{}"
hash = "sha256:abc123"
"#,
                "1".repeat(40),
            ),
        ] {
            assert!(parse_lock_file(&input).is_err(), "accepted {input}");
        }
    }

    #[test]
    fn cache_paths_are_deterministic() {
        let path = cache_path("/cache", "https://github.com/user/repo.git", "abc123");
        assert_eq!(path.file_name().unwrap(), "abc123");
        assert_eq!(
            path,
            cache_path("/cache", "https://github.com/user/repo.git", "abc123")
        );
        assert_ne!(
            path,
            cache_path("/cache", "https://github.com/other/repo.git", "abc123")
        );
    }

    #[test]
    fn hashes_directories_deterministically() {
        let dir = temp_dir("yar-hash-dir");
        fs::write(dir.join("a.txt"), "hello").unwrap();
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub").join("b.txt"), "world").unwrap();

        let hash = hash_dir(&dir).unwrap();
        let same_hash = hash_dir(&dir).unwrap();

        assert_eq!(
            hash,
            "sha256:e68a20c412df9857f6ce49e03cf0754d7080cd60545e03da83d418bae9f6ad1c"
        );
        assert_eq!(hash, same_hash);
        verify_hash(&dir, &hash).unwrap();
        assert!(verify_hash(&dir, "sha256:abc123").is_err());

        fs::write(dir.join("a.txt"), "changed").unwrap();
        assert_ne!(hash, hash_dir(&dir).unwrap());
        assert!(verify_hash(&dir, &hash).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_in_hashed_dependency_trees() {
        use std::os::unix::fs::symlink;

        let dir = temp_dir("yar-hash-symlink");
        fs::write(dir.join("target.yar"), "package target\n").unwrap();
        symlink("target.yar", dir.join("alias.yar")).unwrap();

        let err = hash_dir(&dir).unwrap_err().to_string();
        assert!(
            err.contains("not a regular file or directory"),
            "unexpected error: {err}"
        );

        let root_link = dir.with_extension("link");
        symlink(&dir, &root_link).unwrap();
        let err = hash_dir(&root_link).unwrap_err().to_string();
        assert!(
            err.contains("root is not a directory"),
            "unexpected error: {err}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_cache_roots_and_buckets() {
        use std::os::unix::fs::symlink;

        let git = "https://example.com/dependency.git";
        let commit = "1".repeat(40);
        let real_cache = temp_dir("yar-real-cache");
        let dependency_dir = cache_path(&real_cache, git, &commit);
        fs::create_dir_all(&dependency_dir).unwrap();
        fs::write(dependency_dir.join("dep.yar"), "package dep\n").unwrap();
        let hash = hash_dir(&dependency_dir).unwrap();

        let linked_cache = real_cache.with_extension("link");
        symlink(&real_cache, &linked_cache).unwrap();
        let err = verify_cached_dependency(&linked_cache, git, &commit, &hash)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("dependency cache root") && err.contains("not a real directory"),
            "unexpected error: {err}"
        );

        let cache = temp_dir("yar-cache-bucket-link");
        let real_bucket = temp_dir("yar-real-cache-bucket");
        let dependency_dir = real_bucket.join(&commit);
        fs::create_dir_all(&dependency_dir).unwrap();
        fs::write(dependency_dir.join("dep.yar"), "package dep\n").unwrap();
        let hash = hash_dir(&dependency_dir).unwrap();
        let linked_bucket = cache_path(&cache, git, &commit)
            .parent()
            .expect("cache entry has a bucket")
            .to_path_buf();
        symlink(&real_bucket, &linked_bucket).unwrap();

        let err = verify_cached_dependency(&cache, git, &commit, &hash)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("dependency cache bucket") && err.contains("not a real directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn resolver_distinguishes_ref_kinds_and_reports_active_cycles() {
        let tag = Dependency {
            git: "https://example.com/shared.git".to_string(),
            tag: "v1".to_string(),
            ..Dependency::default()
        };
        let branch = Dependency {
            git: tag.git.clone(),
            branch: "v1".to_string(),
            ..Dependency::default()
        };
        let mut resolver = Resolver {
            cache_dir: PathBuf::new(),
            resolved: BTreeMap::from([(
                "shared".to_string(),
                ResolvedDep {
                    name: "shared".to_string(),
                    dependency: tag,
                    commit: "1".repeat(40),
                    hash: format!("sha256:{}", "1".repeat(64)),
                    dir: PathBuf::from("shared"),
                    dependencies: Vec::new(),
                },
            )]),
            visiting: BTreeSet::new(),
            edges: BTreeMap::new(),
        };
        let error = resolver
            .resolve("shared", &branch, Path::new("."), &[], None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("tag \"v1\""), "{error}");
        assert!(error.contains("branch \"v1\""), "{error}");

        let error = resolver
            .resolve(
                "local",
                &Dependency {
                    path: "../local".to_string(),
                    ..Dependency::default()
                },
                Path::new("."),
                &["parent".to_string()],
                Some("parent"),
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("path inside dependency"), "{error}");

        let shared = resolver.resolved["shared"].dependency.clone();
        resolver.visiting.insert("shared".to_string());
        let error = resolver
            .resolve(
                "shared",
                &shared,
                Path::new("."),
                &["parent".to_string()],
                Some("parent"),
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("dependency cycle"), "{error}");
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
