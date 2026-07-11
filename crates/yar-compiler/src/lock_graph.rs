use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
};

use crate::manifest::{
    Dependency, LockDependency, LockEntry, MANIFEST_FILE, Manifest, ManifestError, clean_path,
    read_manifest, validate_lock_entries,
};

pub fn requires_lock(root_dir: &Path, manifest: &Manifest) -> Result<bool, ManifestError> {
    Ok(!effective_root_dependencies(root_dir, manifest)?
        .git
        .is_empty())
}

pub fn reconcile_lock_entries(
    root_dir: &Path,
    manifest: &Manifest,
    entries: &[LockEntry],
) -> Result<(), ManifestError> {
    let roots = effective_root_dependencies(root_dir, manifest)?;
    validate_local_aliases(&roots, entries)?;
    let reachable = reachable_entries(&roots.git, entries)?;
    if reachable.len() == entries.len() {
        return Ok(());
    }

    let reachable_names = reachable
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<BTreeSet<_>>();
    let stale = entries
        .iter()
        .filter(|entry| !reachable_names.contains(entry.name.as_str()))
        .map(|entry| format!("{:?}", entry.name))
        .collect::<Vec<_>>();
    Err(invalid(format!(
        "yar.lock: unreachable package entries: {}",
        stale.join(", ")
    )))
}

pub fn verify_locked_dependency_manifest(
    dependency_dir: &Path,
    expected: &[LockDependency],
) -> Result<(), ManifestError> {
    let manifest_path = dependency_dir.join(MANIFEST_FILE);
    let manifest = match read_manifest(&manifest_path) {
        Ok(manifest) => manifest,
        Err(ManifestError::Io(err)) if err.kind() == io::ErrorKind::NotFound => Manifest::default(),
        Err(err) => return Err(err),
    };

    let mut actual = Vec::with_capacity(manifest.dependencies.len());
    for (alias, dependency) in &manifest.dependencies {
        if dependency.is_local() {
            return Err(invalid(format!(
                "locked dependency manifest {} declares path dependency {alias:?}; path dependencies are supported only in the root yar.toml",
                manifest_path.display()
            )));
        }
        actual.push(LockDependency::new(alias, dependency));
    }
    actual.sort_by(|left, right| left.name.cmp(&right.name));

    let mut expected = expected.to_vec();
    expected.sort_by(|left, right| left.name.cmp(&right.name));
    if actual == expected {
        return Ok(());
    }
    Err(invalid(format!(
        "locked dependency manifest {} does not match its dependency edges in yar.lock",
        manifest_path.display()
    )))
}

pub fn merge_lock_entries_for_update(
    root_dir: &Path,
    manifest: &Manifest,
    selected_alias: &str,
    existing: &[LockEntry],
    replacement: &[LockEntry],
) -> Result<Vec<LockEntry>, ManifestError> {
    let Some(selected) = manifest.dependencies.get(selected_alias) else {
        return Err(invalid(format!(
            "dependency {selected_alias:?} is not declared in yar.toml"
        )));
    };
    if selected.is_local() {
        return Err(invalid(format!(
            "dependency {selected_alias:?} is a path dependency and has no independent locked revision; run 'yar lock'"
        )));
    }

    validate_lock_entries(existing)?;
    validate_lock_entries(replacement)?;

    let roots = effective_root_dependencies(root_dir, manifest)?;
    let mut unselected_roots = roots.git.clone();
    unselected_roots.remove(selected_alias);
    let preserved = reachable_entries(&unselected_roots, existing)?;

    let mut selected_manifest = Manifest {
        package: manifest.package.clone(),
        dependencies: BTreeMap::new(),
    };
    selected_manifest
        .dependencies
        .insert(selected_alias.to_string(), selected.clone());
    reconcile_lock_entries(root_dir, &selected_manifest, replacement)?;

    let mut candidate = preserved
        .into_iter()
        .map(|entry| (entry.name.clone(), entry))
        .collect::<BTreeMap<_, _>>();
    for entry in replacement {
        if let Some(existing) = candidate.get(&entry.name)
            && existing.dependency() != entry.dependency()
        {
            return Err(invalid(format!(
                "dependency conflict for {:?}: selected update requires {}, but an unselected root requires {}",
                entry.name,
                format_dependency_source(&entry.dependency()),
                format_dependency_source(&existing.dependency())
            )));
        }
        candidate.insert(entry.name.clone(), entry.clone());
    }

    let candidate = candidate.into_values().collect::<Vec<_>>();
    let reachable = reachable_entries(&roots.git, &candidate)?;
    reconcile_lock_entries(root_dir, manifest, &reachable)?;
    Ok(reachable)
}

pub fn validate_lock_entries_for_update(
    root_dir: &Path,
    manifest: &Manifest,
    selected_alias: &str,
    existing: &[LockEntry],
) -> Result<(), ManifestError> {
    let Some(selected) = manifest.dependencies.get(selected_alias) else {
        return Err(invalid(format!(
            "dependency {selected_alias:?} is not declared in yar.toml"
        )));
    };
    if selected.is_local() {
        return Err(invalid(format!(
            "dependency {selected_alias:?} is a path dependency and has no independent locked revision; run 'yar lock'"
        )));
    }

    validate_lock_entries(existing)?;
    let roots = effective_root_dependencies(root_dir, manifest)?;
    let mut unselected_roots = roots.git.clone();
    unselected_roots.remove(selected_alias);
    let preserved = reachable_entries(&unselected_roots, existing)?;
    validate_local_aliases(&roots, &preserved)?;
    if let Some(entry) = preserved.iter().find(|entry| entry.name == selected_alias)
        && entry.dependency() != *selected
    {
        return Err(invalid(format!(
            "dependency conflict for {selected_alias:?}: the selected root requires {}, but an unselected root requires {}",
            format_dependency_source(selected),
            format_dependency_source(&entry.dependency())
        )));
    }
    Ok(())
}

#[derive(Default)]
struct EffectiveRoots {
    git: BTreeMap<String, Dependency>,
    local: BTreeMap<String, PathBuf>,
}

fn effective_root_dependencies(
    root_dir: &Path,
    manifest: &Manifest,
) -> Result<EffectiveRoots, ManifestError> {
    let mut roots = EffectiveRoots::default();
    let mut visited = BTreeSet::new();
    collect_live_dependencies(root_dir, manifest, true, &mut roots, &mut visited)?;
    Ok(roots)
}

fn collect_live_dependencies(
    parent_dir: &Path,
    manifest: &Manifest,
    allow_path_dependencies: bool,
    roots: &mut EffectiveRoots,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<(), ManifestError> {
    for (alias, dependency) in &manifest.dependencies {
        if !dependency.is_local() {
            if let Some(local) = roots.local.get(alias) {
                return Err(invalid(format!(
                    "dependency conflict for {alias:?}: path {} vs {}",
                    local.display(),
                    format_dependency_source(dependency)
                )));
            }
            if let Some(existing) = roots.git.get(alias) {
                if existing != dependency {
                    return Err(invalid(format!(
                        "dependency conflict for {alias:?}: {} vs {}",
                        format_dependency_source(existing),
                        format_dependency_source(dependency)
                    )));
                }
                continue;
            }
            roots.git.insert(alias.clone(), dependency.clone());
            continue;
        }

        if !allow_path_dependencies {
            return Err(invalid(format!(
                "dependency {alias:?} uses a path inside live dependency {}: path dependencies are supported only in the root yar.toml",
                parent_dir.display()
            )));
        }
        let dir = dependency_dir(parent_dir, dependency);
        if let Some(existing) = roots.git.get(alias) {
            return Err(invalid(format!(
                "dependency conflict for {alias:?}: {} vs path {}",
                format_dependency_source(existing),
                dir.display()
            )));
        }
        if let Some(existing) = roots.local.get(alias) {
            if existing != &dir {
                return Err(invalid(format!(
                    "dependency conflict for {alias:?}: path {} vs path {}",
                    existing.display(),
                    dir.display()
                )));
            }
        } else {
            roots.local.insert(alias.clone(), dir.clone());
        }

        if !visited.insert(dir.clone()) {
            continue;
        }
        let manifest_path = dir.join(MANIFEST_FILE);
        match read_manifest(&manifest_path) {
            Ok(manifest) => collect_live_dependencies(&dir, &manifest, false, roots, visited)?,
            Err(ManifestError::Io(err)) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

fn validate_local_aliases(
    roots: &EffectiveRoots,
    entries: &[LockEntry],
) -> Result<(), ManifestError> {
    if let Some(entry) = entries
        .iter()
        .find(|entry| roots.local.contains_key(&entry.name))
    {
        return Err(invalid(format!(
            "yar.lock: package {:?} conflicts with a live path dependency of the same name",
            entry.name
        )));
    }
    Ok(())
}

fn reachable_entries(
    roots: &BTreeMap<String, Dependency>,
    entries: &[LockEntry],
) -> Result<Vec<LockEntry>, ManifestError> {
    validate_lock_entries(entries)?;
    let entries = entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut visited = BTreeSet::new();
    let mut active = BTreeSet::new();
    let mut stack = Vec::new();
    for (alias, dependency) in roots {
        visit(
            alias,
            dependency,
            &entries,
            &mut visited,
            &mut active,
            &mut stack,
        )?;
    }
    Ok(visited
        .into_iter()
        .filter_map(|name| entries.get(name.as_str()).copied().cloned())
        .collect())
}

fn visit(
    alias: &str,
    expected: &Dependency,
    entries: &BTreeMap<&str, &LockEntry>,
    visited: &mut BTreeSet<String>,
    active: &mut BTreeSet<String>,
    stack: &mut Vec<String>,
) -> Result<(), ManifestError> {
    let Some(entry) = entries.get(alias).copied() else {
        return Err(invalid(format!(
            "yar.lock: missing package {alias:?} required by the dependency graph"
        )));
    };
    if entry.dependency() != *expected {
        return Err(invalid(format!(
            "yar.lock: package {alias:?} has {}, but the dependency graph requires {}",
            format_dependency_source(&entry.dependency()),
            format_dependency_source(expected)
        )));
    }
    if active.contains(alias) {
        let start = stack.iter().position(|name| name == alias).unwrap_or(0);
        let mut cycle = stack[start..].to_vec();
        cycle.push(alias.to_string());
        return Err(invalid(format!(
            "yar.lock: dependency cycle: {}",
            cycle.join(" -> ")
        )));
    }
    if !visited.insert(alias.to_string()) {
        return Ok(());
    }

    active.insert(alias.to_string());
    stack.push(alias.to_string());
    for dependency in &entry.dependencies {
        visit(
            &dependency.name,
            &dependency.dependency(),
            entries,
            visited,
            active,
            stack,
        )?;
    }
    stack.pop();
    active.remove(alias);
    Ok(())
}

fn dependency_dir(parent_dir: &Path, dependency: &Dependency) -> PathBuf {
    let path = PathBuf::from(&dependency.path);
    if path.is_absolute() {
        clean_path(&path)
    } else {
        clean_path(&parent_dir.join(path))
    }
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

fn invalid(message: impl Into<String>) -> ManifestError {
    ManifestError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::manifest::{PackageInfo, write_manifest};

    use super::*;

    #[test]
    fn reconciles_a_valid_transitive_diamond_without_dependency_caches() {
        let root = temp_dir("yar-lock-graph-diamond");
        let manifest = manifest([("left", git("left", "v1")), ("right", git("right", "v1"))]);
        let entries = vec![
            entry("left", "v1", '1', &[("shared", "v1")]),
            entry("right", "v1", '2', &[("shared", "v1")]),
            entry("shared", "v1", '3', &[]),
        ];

        reconcile_lock_entries(&root, &manifest, &entries).unwrap();
    }

    #[test]
    fn rejects_missing_mismatched_cyclic_and_unreachable_nodes() {
        let root = temp_dir("yar-lock-graph-invalid");
        let root_manifest = manifest([("root", git("root", "v1"))]);

        let error = reconcile_lock_entries(&root, &root_manifest, &[])
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing package \"root\""), "{error}");

        let error = reconcile_lock_entries(&root, &root_manifest, &[entry("root", "v2", '1', &[])])
            .unwrap_err()
            .to_string();
        assert!(error.contains("dependency graph requires"), "{error}");

        let branch_manifest = manifest([(
            "root",
            Dependency {
                git: "https://example.com/root.git".to_string(),
                branch: "v1".to_string(),
                ..Dependency::default()
            },
        )]);
        let error =
            reconcile_lock_entries(&root, &branch_manifest, &[entry("root", "v1", '1', &[])])
                .unwrap_err()
                .to_string();
        assert!(error.contains("branch \"v1\""), "{error}");
        assert!(error.contains("tag \"v1\""), "{error}");

        let error = reconcile_lock_entries(
            &root,
            &root_manifest,
            &[entry("root", "v1", '1', &[("missing", "v1")])],
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("missing package \"missing\""), "{error}");

        let error = reconcile_lock_entries(
            &root,
            &root_manifest,
            &[
                entry("root", "v1", '1', &[("child", "v1")]),
                entry("child", "v1", '2', &[("root", "v1")]),
            ],
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("dependency cycle"), "{error}");

        let error = reconcile_lock_entries(
            &root,
            &root_manifest,
            &[
                entry("root", "v1", '1', &[]),
                entry("injected", "v1", '2', &[]),
            ],
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("unreachable package entries"), "{error}");
        assert!(error.contains("injected"), "{error}");
    }

    #[test]
    fn live_local_manifests_contribute_git_roots() {
        let root = temp_dir("yar-lock-graph-local-root");
        let local = root.join("local");
        fs::create_dir_all(&local).unwrap();
        fs::write(
            local.join(MANIFEST_FILE),
            write_manifest(&manifest([("child", git("child", "v1"))])).unwrap(),
        )
        .unwrap();
        let root_manifest = manifest([(
            "local",
            Dependency {
                path: "local".to_string(),
                ..Dependency::default()
            },
        )]);

        assert!(requires_lock(&root, &root_manifest).unwrap());
        reconcile_lock_entries(&root, &root_manifest, &[entry("child", "v1", '1', &[])]).unwrap();
    }

    #[test]
    fn selected_dependency_manifest_must_match_recorded_edges() {
        let dir = temp_dir("yar-lock-graph-selected-manifest");
        fs::write(
            dir.join(MANIFEST_FILE),
            write_manifest(&manifest([("child", git("child", "v1"))])).unwrap(),
        )
        .unwrap();

        verify_locked_dependency_manifest(&dir, &[edge("child", "v1")]).unwrap();
        let error = verify_locked_dependency_manifest(&dir, &[edge("child", "v2")])
            .unwrap_err()
            .to_string();
        assert!(error.contains("does not match"), "{error}");

        fs::write(
            dir.join(MANIFEST_FILE),
            write_manifest(&manifest([(
                "child",
                Dependency {
                    path: "../child".to_string(),
                    ..Dependency::default()
                },
            )]))
            .unwrap(),
        )
        .unwrap();
        let error = verify_locked_dependency_manifest(&dir, &[])
            .unwrap_err()
            .to_string();
        assert!(error.contains("declares path dependency"), "{error}");
    }

    #[test]
    fn selected_update_prunes_orphans_and_preserves_shared_nodes() {
        let root = temp_dir("yar-lock-graph-update");
        let manifest = manifest([
            ("parent", git("parent", "v1")),
            ("sibling", git("sibling", "v1")),
        ]);
        let existing = vec![
            entry("parent", "v1", '1', &[("old", "v1"), ("shared", "v1")]),
            entry("sibling", "v1", '2', &[("shared", "v1")]),
            entry("old", "v1", '3', &[]),
            entry("shared", "v1", '4', &[]),
        ];
        let replacement = vec![
            entry("parent", "v1", '5', &[("new", "v1"), ("shared", "v1")]),
            entry("new", "v1", '6', &[]),
            entry("shared", "v1", '7', &[]),
        ];

        let merged =
            merge_lock_entries_for_update(&root, &manifest, "parent", &existing, &replacement)
                .unwrap();
        let names = merged
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["new", "parent", "shared", "sibling"]);
        assert_eq!(find(&merged, "sibling").commit, "2".repeat(40));
        assert_eq!(find(&merged, "shared").commit, "7".repeat(40));
    }

    #[test]
    fn selected_update_rejects_stale_unselected_roots_and_shared_conflicts() {
        let root = temp_dir("yar-lock-graph-update-conflict");
        let stale_manifest = manifest([
            ("parent", git("parent", "v1")),
            ("sibling", git("sibling", "v2")),
        ]);
        let existing = vec![
            entry("parent", "v0", '1', &[]),
            entry("sibling", "v1", '2', &[]),
        ];
        let replacement = vec![entry("parent", "v1", '3', &[])];
        let error = merge_lock_entries_for_update(
            &root,
            &stale_manifest,
            "parent",
            &existing,
            &replacement,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("dependency graph requires"), "{error}");

        let manifest = manifest([
            ("parent", git("parent", "v1")),
            ("sibling", git("sibling", "v1")),
        ]);
        let existing = vec![
            entry("parent", "v0", '1', &[]),
            entry("sibling", "v1", '2', &[("shared", "v1")]),
            entry("shared", "v1", '3', &[]),
        ];
        let replacement = vec![
            entry("parent", "v1", '4', &[("shared", "v2")]),
            entry("shared", "v2", '5', &[]),
        ];
        let error =
            merge_lock_entries_for_update(&root, &manifest, "parent", &existing, &replacement)
                .unwrap_err()
                .to_string();
        assert!(
            error.contains("dependency conflict for \"shared\""),
            "{error}"
        );
    }

    #[test]
    fn selected_update_can_replace_a_stale_git_alias_with_a_root_path_dependency() {
        let root = temp_dir("yar-lock-graph-update-local-replacement");
        let manifest = manifest([
            ("parent", git("parent", "v1")),
            (
                "old",
                Dependency {
                    path: "local-old".to_string(),
                    ..Dependency::default()
                },
            ),
        ]);
        let existing = vec![
            entry("parent", "v1", '1', &[("old", "v1")]),
            entry("old", "v1", '2', &[]),
        ];
        let replacement = vec![entry("parent", "v1", '3', &[])];

        validate_lock_entries_for_update(&root, &manifest, "parent", &existing).unwrap();
        let merged =
            merge_lock_entries_for_update(&root, &manifest, "parent", &existing, &replacement)
                .unwrap();

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "parent");
        assert_eq!(merged[0].commit, "3".repeat(40));
    }

    fn manifest<const N: usize>(dependencies: [(&str, Dependency); N]) -> Manifest {
        Manifest {
            package: PackageInfo {
                name: "app".to_string(),
                version: String::new(),
            },
            dependencies: dependencies
                .into_iter()
                .map(|(name, dependency)| (name.to_string(), dependency))
                .collect(),
        }
    }

    fn git(name: &str, tag: &str) -> Dependency {
        Dependency {
            git: format!("https://example.com/{name}.git"),
            tag: tag.to_string(),
            ..Dependency::default()
        }
    }

    fn edge(name: &str, tag: &str) -> LockDependency {
        LockDependency::new(name, &git(name, tag))
    }

    fn entry(
        name: &str,
        tag: &str,
        commit_digit: char,
        dependencies: &[(&str, &str)],
    ) -> LockEntry {
        let dependency = git(name, tag);
        LockEntry {
            name: name.to_string(),
            git: dependency.git,
            tag: dependency.tag,
            commit: commit_digit.to_string().repeat(40),
            hash: format!("sha256:{}", commit_digit.to_string().repeat(64)),
            dependencies: dependencies
                .iter()
                .map(|(name, tag)| edge(name, tag))
                .collect(),
            ..LockEntry::default()
        }
    }

    fn find<'a>(entries: &'a [LockEntry], name: &str) -> &'a LockEntry {
        entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap_or_else(|| panic!("missing {name:?}: {entries:?}"))
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
