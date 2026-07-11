use std::{
    env,
    error::Error,
    ffi::{OsStr, OsString},
    fmt, fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{SystemTime, UNIX_EPOCH},
};

mod metadata_transaction;
mod project;

use metadata_transaction::{FileChange, MetadataChanges};
use project::ProjectPaths;

use yar_compiler::{
    compile::{CompileOptions, Unit, compile_path, compile_test_path},
    diag,
    lock_graph::{
        merge_lock_entries_for_update, reconcile_lock_entries, requires_lock,
        validate_lock_entries_for_update, verify_locked_dependency_manifest,
    },
    manifest::{
        Dependency, MANIFEST_FILE, Manifest, ManifestError, PackageInfo, STDLIB_IMPORT_ROOT,
        cache_dir_from_env, fetch_locked_dependency, is_cached, read_lock_file, read_manifest,
        resolve_dependencies, to_lock_entries, valid_alias, valid_dependency_alias,
        write_lock_file, write_manifest,
    },
};

const RUNTIME_ARCHIVE_ENV: &str = "YAR_RUNTIME_ARCHIVE";
const ROOT_USAGE: &str = "usage: yar [--manifest-path <path/to/yar.toml>] <command> [arguments]";

fn main() -> ExitCode {
    match run(env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(message)) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
        Err(CliError::Diagnostics { path, diagnostics }) => {
            eprintln!("{}", diag::format(&path, &diagnostics));
            ExitCode::FAILURE
        }
        Err(CliError::ProcessExit(code)) => ExitCode::from(code.clamp(1, 255) as u8),
        Err(CliError::Other(err)) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<OsString>) -> Result<(), CliError> {
    let Invocation {
        command,
        command_args,
        manifest_path,
    } = parse_invocation(args)?;
    let current_dir = env::current_dir()?;

    match command.as_str() {
        "check" => run_check(&command_args, &current_dir, manifest_path.as_deref()),
        "emit-ir" => run_emit_ir(&command_args, &current_dir, manifest_path.as_deref()),
        "build" => run_build(&command_args, &current_dir, manifest_path.as_deref()),
        "run" => run_run(&command_args, &current_dir, manifest_path.as_deref()),
        "test" => run_test(&command_args, &current_dir, manifest_path.as_deref()),
        "init" => run_init(&command_args, &current_dir, manifest_path.as_deref()),
        "add" => run_add(&command_args, &current_dir, manifest_path.as_deref()),
        "remove" => run_remove(&command_args, &current_dir, manifest_path.as_deref()),
        "fetch" => run_fetch(&command_args, &current_dir, manifest_path.as_deref()),
        "lock" => run_lock(&command_args, &current_dir, manifest_path.as_deref()),
        "update" => run_update(&command_args, &current_dir, manifest_path.as_deref()),
        _ => Err(CliError::usage(format!("unknown command {command:?}"))),
    }
}

struct Invocation {
    command: String,
    command_args: Vec<OsString>,
    manifest_path: Option<PathBuf>,
}

fn parse_invocation(mut args: Vec<OsString>) -> Result<Invocation, CliError> {
    let mut manifest_path = None;
    while args.first().is_some_and(|arg| arg == "--manifest-path") {
        if manifest_path.is_some() {
            return Err(CliError::usage(
                "--manifest-path may be specified only once",
            ));
        }
        if args.len() < 2 {
            return Err(CliError::usage(ROOT_USAGE));
        }
        let path = PathBuf::from(&args[1]);
        if path.file_name() != Some(OsStr::new(MANIFEST_FILE)) {
            return Err(CliError::usage("--manifest-path must name yar.toml"));
        }
        manifest_path = Some(path);
        args.drain(..2);
    }

    let Some(command) = args.first().and_then(|arg| arg.to_str()) else {
        return Err(CliError::usage(ROOT_USAGE));
    };
    Ok(Invocation {
        command: command.to_owned(),
        command_args: args[1..].to_vec(),
        manifest_path,
    })
}

fn entry_project(
    current_dir: &Path,
    entry: &Path,
    manifest_path: Option<&Path>,
) -> Result<ProjectPaths, CliError> {
    let project = ProjectPaths::for_entry(current_dir, entry, manifest_path)?;
    if manifest_path.is_some() {
        project.require_manifest()?;
    }
    Ok(project)
}

fn metadata_project(
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<ProjectPaths, CliError> {
    let project = ProjectPaths::for_metadata(current_dir, manifest_path)?;
    project.require_manifest()?;
    Ok(project)
}

fn run_check(
    args: &[OsString],
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<(), CliError> {
    let path = parse_single_path(args, "usage: yar check <file|dir>")?;
    let project = entry_project(current_dir, &path, manifest_path)?;
    let options = CompileOptions {
        project_root: Some(project.root().to_path_buf()),
        ..CompileOptions::default()
    };
    let (unit, diagnostics) = compile_path(&path, &options)?;
    if !diagnostics.is_empty() {
        return Err(CliError::Diagnostics {
            path: path_string(&path),
            diagnostics,
        });
    }
    if unit.is_none() {
        return Err(CliError::other("compilation stopped without diagnostics"));
    }
    Ok(())
}

fn run_emit_ir(
    args: &[OsString],
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<(), CliError> {
    let path = parse_single_path(args, "usage: yar emit-ir <file|dir>")?;
    let project = entry_project(current_dir, &path, manifest_path)?;
    let target = Target::resolve()?;
    let options = CompileOptions {
        target_triple: target.triple.clone(),
        project_root: Some(project.root().to_path_buf()),
    };
    let (unit, diagnostics) = compile_path(&path, &options)?;
    if !diagnostics.is_empty() {
        return Err(CliError::Diagnostics {
            path: path_string(&path),
            diagnostics,
        });
    }
    let Some(unit) = unit else {
        return Err(CliError::other("compilation stopped without diagnostics"));
    };
    print!("{}", unit.ir);
    Ok(())
}

fn run_build(
    args: &[OsString],
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<(), CliError> {
    let target = Target::resolve()?;
    let BuildArgs { path, output } = parse_build_args(args, &target)?;
    let project = entry_project(current_dir, &path, manifest_path)?;
    build_path(&path, project.root(), &target, &output)
}

fn run_run(
    args: &[OsString],
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<(), CliError> {
    let path = parse_single_path(args, "usage: yar run <file|dir>")?;
    let project = entry_project(current_dir, &path, manifest_path)?;
    let target = Target::resolve()?;
    if target.is_cross() {
        return Err(CliError::other(format!(
            "cannot execute cross-compiled binary (target {}/{})",
            target.os, target.arch
        )));
    }

    let tmp_dir = TempDir::new("yar-rust-run")?;
    let output = tmp_dir
        .path()
        .join(format!("program{}", target.exe_suffix()));
    build_path(&path, project.root(), &target, &output)?;
    invoke_program(&output)
}

fn run_test(
    args: &[OsString],
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<(), CliError> {
    let path = parse_single_path(args, "usage: yar test <file|dir>")?;
    let project = entry_project(current_dir, &path, manifest_path)?;
    let target = Target::resolve()?;
    if target.is_cross() {
        return Err(CliError::other(format!(
            "cannot execute cross-compiled test binary (target {}/{})",
            target.os, target.arch
        )));
    }

    let options = CompileOptions {
        target_triple: target.triple.clone(),
        project_root: Some(project.root().to_path_buf()),
    };
    let (unit, diagnostics) = compile_test_path(&path, &options)?;
    if !diagnostics.is_empty() {
        return Err(CliError::Diagnostics {
            path: path_string(&path),
            diagnostics,
        });
    }
    let Some(unit) = unit else {
        return Err(CliError::other("compilation stopped without diagnostics"));
    };

    let tmp_dir = TempDir::new("yar-rust-test")?;
    let output = tmp_dir.path().join(format!("test{}", target.exe_suffix()));
    build_unit(unit, &target, &output)?;
    invoke_program(&output)
}

fn run_init(
    args: &[OsString],
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<(), CliError> {
    if !args.is_empty() {
        return Err(CliError::usage("usage: yar init"));
    }
    let project = ProjectPaths::for_init(current_dir, manifest_path)?;
    let path = project.manifest();
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            return Err(CliError::other(format!(
                "{} already exists",
                path.display()
            )));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let manifest = Manifest {
        package: PackageInfo {
            name: default_package_name(project.root()),
            version: String::new(),
        },
        dependencies: Default::default(),
    };
    write_manifest_file(&path, &manifest)?;
    println!("created yar.toml");
    Ok(())
}

fn run_add(
    args: &[OsString],
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<(), CliError> {
    if args.len() < 2 {
        return Err(CliError::usage(
            "usage: yar add <alias> <git-url> [--tag=v1.0.0|--rev=abc123|--branch=main]\n       yar add <alias> --path=<dir>",
        ));
    }

    let alias = args[0].to_string_lossy();
    if alias == STDLIB_IMPORT_ROOT {
        return Err(CliError::other(format!(
            "dependency alias {alias:?} is reserved for the standard library"
        )));
    }
    if !valid_dependency_alias(&alias) {
        return Err(CliError::other(format!("invalid alias {alias:?}")));
    }

    let dependency = parse_dependency_args(&args[1..])?;

    let project = ProjectPaths::for_metadata(current_dir, manifest_path)?;
    let mut manifest = read_or_create_manifest(&project)?;
    manifest.dependencies.insert(alias.to_string(), dependency);
    let manifest_content = write_manifest(&manifest)?;
    let lock_content = resolve_lock_content(project.root(), &manifest)?;
    publish_metadata(
        project.root(),
        FileChange::Write(manifest_content.as_bytes()),
        lock_content.as_deref(),
    )?;
    println!("added dependency {alias:?}");
    if lock_content.is_some() {
        println!("yar.lock updated");
    }
    Ok(())
}

fn run_remove(
    args: &[OsString],
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<(), CliError> {
    if args.len() != 1 {
        return Err(CliError::usage("usage: yar remove <alias>"));
    }

    let alias = args[0].to_string_lossy();
    let project = metadata_project(current_dir, manifest_path)?;
    let mut manifest = read_project_manifest(&project)?;
    if manifest.dependencies.remove(alias.as_ref()).is_none() {
        return Err(CliError::other(format!("dependency {alias:?} not found")));
    }
    let manifest_content = write_manifest(&manifest)?;
    let lock_content = resolve_lock_content(project.root(), &manifest)?;
    publish_metadata(
        project.root(),
        FileChange::Write(manifest_content.as_bytes()),
        lock_content.as_deref(),
    )?;
    println!("removed dependency {alias:?}");
    if lock_content.is_some() {
        println!("yar.lock updated");
    }
    Ok(())
}

fn run_fetch(
    args: &[OsString],
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<(), CliError> {
    if !args.is_empty() {
        return Err(CliError::usage("usage: yar fetch"));
    }
    let project = metadata_project(current_dir, manifest_path)?;
    let manifest = read_project_manifest(&project)?;
    let needs_lock = requires_lock(project.root(), &manifest)?;
    let entries = match read_lock_file(project.lock()) {
        Ok(entries) => entries,
        Err(ManifestError::Io(err)) if err.kind() == io::ErrorKind::NotFound && !needs_lock => {
            Vec::new()
        }
        Err(err) => {
            return Err(CliError::other(format!(
                "yar.lock is invalid: {err}; run 'yar lock'"
            )));
        }
    };
    reconcile_lock_entries(project.root(), &manifest, &entries).map_err(|err| {
        CliError::other(format!(
            "yar.toml and yar.lock do not describe one valid dependency graph (run 'yar lock'): {err}"
        ))
    })?;
    if entries.is_empty() {
        println!("all dependencies fetched");
        return Ok(());
    }
    let cache_dir = cache_dir_from_env()?;
    for entry in entries {
        if !is_cached(&cache_dir, &entry.git, &entry.commit) {
            println!("fetching {}...", entry.name);
        }
        fetch_locked_dependency(
            &cache_dir,
            &entry.git,
            &entry.commit,
            &entry.hash,
            |dependency_dir| {
                verify_locked_dependency_manifest(dependency_dir, &entry.dependencies).map_err(
                    |err| {
                        ManifestError::Invalid(format!(
                            "dependency manifest does not match yar.lock: {err}; run 'yar lock'"
                        ))
                    },
                )
            },
        )
        .map_err(|err| CliError::other(format!("fetching {}: {err}", entry.name)))?;
    }
    println!("all dependencies fetched");
    Ok(())
}

fn run_lock(
    args: &[OsString],
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<(), CliError> {
    if !args.is_empty() {
        return Err(CliError::usage("usage: yar lock"));
    }
    let project = metadata_project(current_dir, manifest_path)?;
    update_lock_file(&project)
}

fn run_update(
    args: &[OsString],
    current_dir: &Path,
    manifest_path: Option<&Path>,
) -> Result<(), CliError> {
    if args.len() > 1 {
        return Err(CliError::usage("usage: yar update [alias]"));
    }
    let project = metadata_project(current_dir, manifest_path)?;
    match args.first() {
        Some(alias) => update_lock_file_for_alias(&project, &alias.to_string_lossy()),
        None => update_lock_file(&project),
    }
}

fn build_path(
    path: &Path,
    project_root: &Path,
    target: &Target,
    output: &Path,
) -> Result<(), CliError> {
    let options = CompileOptions {
        target_triple: target.triple.clone(),
        project_root: Some(project_root.to_path_buf()),
    };
    let (unit, diagnostics) = compile_path(path, &options)?;
    if !diagnostics.is_empty() {
        return Err(CliError::Diagnostics {
            path: path_string(path),
            diagnostics,
        });
    }
    let Some(unit) = unit else {
        return Err(CliError::other("compilation stopped without diagnostics"));
    };

    build_unit(unit, target, output)
}

fn build_unit(unit: Unit, target: &Target, output: &Path) -> Result<(), CliError> {
    let archive = runtime_archive(target)?;

    let tmp_dir = TempDir::new("yar-rust-build")?;
    let ir_path = tmp_dir.path().join("main.ll");
    fs::write(&ir_path, unit.ir)?;
    invoke_clang(target, &ir_path, &archive, output)?;
    Ok(())
}

fn parse_single_path(args: &[OsString], usage: &'static str) -> Result<PathBuf, CliError> {
    if args.len() != 1 {
        return Err(CliError::usage(usage));
    }
    let path = PathBuf::from(&args[0]);
    if path.as_os_str().is_empty() || args[0].to_string_lossy().starts_with('-') {
        return Err(CliError::usage(usage));
    }
    Ok(path)
}

struct BuildArgs {
    path: PathBuf,
    output: PathBuf,
}

fn parse_build_args(args: &[OsString], target: &Target) -> Result<BuildArgs, CliError> {
    let mut path = None;
    let mut output = PathBuf::from(default_output_name(target));
    let mut idx = 0;
    while idx < args.len() {
        if args[idx] == "-o" {
            idx += 1;
            if idx >= args.len() {
                return Err(CliError::usage("usage: yar build <file|dir> [-o output]"));
            }
            output = PathBuf::from(&args[idx]);
        } else if args[idx].to_string_lossy().starts_with('-') {
            return Err(CliError::usage(format!(
                "unknown build flag {:?}",
                args[idx].to_string_lossy()
            )));
        } else if path.replace(PathBuf::from(&args[idx])).is_some() {
            return Err(CliError::usage("usage: yar build <file|dir> [-o output]"));
        }
        idx += 1;
    }

    let Some(path) = path else {
        return Err(CliError::usage("usage: yar build <file|dir> [-o output]"));
    };
    Ok(BuildArgs { path, output })
}

fn parse_dependency_args(args: &[OsString]) -> Result<Dependency, CliError> {
    let mut dependency = Dependency::default();
    for arg in args {
        let arg = arg.to_string_lossy();
        if let Some(value) = arg.strip_prefix("--tag=") {
            dependency.tag = value.to_string();
        } else if let Some(value) = arg.strip_prefix("--rev=") {
            dependency.rev = value.to_string();
        } else if let Some(value) = arg.strip_prefix("--branch=") {
            dependency.branch = value.to_string();
        } else if let Some(value) = arg.strip_prefix("--path=") {
            dependency.path = value.to_string();
        } else if dependency.git.is_empty() {
            dependency.git = arg.to_string();
        } else {
            return Err(CliError::usage(format!("unexpected argument {arg:?}")));
        }
    }

    if dependency.is_local() {
        if !dependency.git.is_empty()
            || !dependency.tag.is_empty()
            || !dependency.rev.is_empty()
            || !dependency.branch.is_empty()
        {
            return Err(CliError::usage(
                "--path cannot be combined with git options",
            ));
        }
        return Ok(dependency);
    }
    if dependency.git.is_empty() {
        return Err(CliError::usage("git URL required"));
    }
    if dependency.tag.is_empty() && dependency.rev.is_empty() && dependency.branch.is_empty() {
        return Err(CliError::usage("one of --tag, --rev, or --branch required"));
    }
    Ok(dependency)
}

fn read_or_create_manifest(project: &ProjectPaths) -> Result<Manifest, CliError> {
    let path = project.manifest();
    match read_manifest(&path) {
        Ok(manifest) => Ok(manifest),
        Err(ManifestError::Io(err)) if err.kind() == io::ErrorKind::NotFound => Ok(Manifest {
            package: PackageInfo {
                name: default_package_name(project.root()),
                version: String::new(),
            },
            dependencies: Default::default(),
        }),
        Err(error) => Err(manifest_read_error(&path, error)),
    }
}

fn read_project_manifest(project: &ProjectPaths) -> Result<Manifest, CliError> {
    let path = project.manifest();
    read_manifest(&path).map_err(|error| manifest_read_error(&path, error))
}

fn manifest_read_error(path: &Path, error: ManifestError) -> CliError {
    CliError::other(format!("reading manifest {}: {error}", path.display()))
}

fn default_package_name(root: &Path) -> String {
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| valid_alias(name))
        .unwrap_or_else(|| "myproject".to_string());
    if name.is_empty() {
        "myproject".to_string()
    } else {
        name
    }
}

fn write_manifest_file(path: &Path, manifest: &Manifest) -> Result<(), CliError> {
    let content = write_manifest(manifest)?;
    fs::write(path, content)?;
    Ok(())
}

fn update_lock_file(project: &ProjectPaths) -> Result<(), CliError> {
    let manifest = read_project_manifest(project)?;
    let lock_content = resolve_lock_content(project.root(), &manifest)?;
    publish_metadata(
        project.root(),
        FileChange::Preserve,
        lock_content.as_deref(),
    )?;
    if lock_content.is_some() {
        println!("yar.lock updated");
    }
    Ok(())
}

fn resolve_lock_content(root_dir: &Path, manifest: &Manifest) -> Result<Option<String>, CliError> {
    if !requires_lock(root_dir, manifest)? {
        return Ok(None);
    }
    let cache_dir = cache_dir_from_env()?;
    let resolved = resolve_dependencies(root_dir, manifest, &cache_dir)?;
    let entries = to_lock_entries(&resolved);
    reconcile_lock_entries(root_dir, manifest, &entries)?;
    if entries.is_empty() {
        return Ok(None);
    }
    Ok(Some(write_lock_file(&entries)))
}

fn update_lock_file_for_alias(project: &ProjectPaths, alias: &str) -> Result<(), CliError> {
    let manifest = read_project_manifest(project)?;
    let Some(dependency) = manifest.dependencies.get(alias) else {
        return Err(CliError::other(format!("dependency {alias:?} not found")));
    };
    if dependency.is_local() {
        return Err(CliError::other(format!(
            "dependency {alias:?} is a path dependency and has no independent locked revision; run 'yar lock'"
        )));
    }

    let existing_entries = match read_lock_file(project.lock()) {
        Ok(entries) => entries,
        Err(ManifestError::Io(err)) if err.kind() == io::ErrorKind::NotFound => {
            return update_lock_file(project);
        }
        Err(err) => {
            return Err(CliError::other(format!(
                "cannot selectively update an invalid or legacy yar.lock: {err}; run 'yar lock'"
            )));
        }
    };
    validate_lock_entries_for_update(project.root(), &manifest, alias, &existing_entries).map_err(
        |err| {
            CliError::other(format!(
                "cannot selectively update {alias:?}: {err}; run 'yar lock' to regenerate the full graph"
            ))
        },
    )?;

    let mut selected_manifest = Manifest {
        package: manifest.package.clone(),
        dependencies: Default::default(),
    };
    selected_manifest
        .dependencies
        .insert(alias.to_string(), dependency.clone());

    let cache_dir = cache_dir_from_env()?;
    let resolved = resolve_dependencies(project.root(), &selected_manifest, &cache_dir)?;
    let updated_entries = to_lock_entries(&resolved);
    let merged_entries = merge_lock_entries_for_update(
        project.root(),
        &manifest,
        alias,
        &existing_entries,
        &updated_entries,
    )
    .map_err(|err| {
        CliError::other(format!(
            "cannot selectively update {alias:?}: {err}; run 'yar lock' to regenerate the full graph"
        ))
    })?;

    let lock_content = if merged_entries.is_empty() {
        None
    } else {
        Some(write_lock_file(&merged_entries))
    };
    publish_metadata(
        project.root(),
        FileChange::Preserve,
        lock_content.as_deref(),
    )?;
    println!("yar.lock updated");
    Ok(())
}

fn publish_metadata(
    root_dir: &Path,
    manifest: FileChange<'_>,
    lock_content: Option<&str>,
) -> Result<(), CliError> {
    let lock = match lock_content {
        Some(content) => FileChange::Write(content.as_bytes()),
        None => FileChange::Delete,
    };
    metadata_transaction::publish(root_dir, MetadataChanges { manifest, lock })
        .map_err(|err| CliError::other(format!("publishing dependency metadata: {err}")))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Target {
    os: String,
    arch: String,
    triple: Option<String>,
}

impl Target {
    fn resolve() -> Result<Self, CliError> {
        let yar_os = env::var("YAR_OS").unwrap_or_default();
        let yar_arch = env::var("YAR_ARCH").unwrap_or_default();
        if yar_os.is_empty() && yar_arch.is_empty() {
            return Ok(host_target());
        }
        if yar_os.is_empty() || yar_arch.is_empty() {
            return Err(CliError::other("set both YAR_OS and YAR_ARCH, or neither"));
        }
        let Some(triple) = target_triple(&yar_os, &yar_arch) else {
            return Err(CliError::other(format!(
                "unsupported target {yar_os}/{yar_arch}; supported targets: {}",
                supported_targets()
            )));
        };
        Ok(Self {
            os: yar_os,
            arch: yar_arch,
            triple: Some(triple.to_string()),
        })
    }

    fn is_cross(&self) -> bool {
        let host = host_target();
        self.os != host.os || self.arch != host.arch
    }

    fn exe_suffix(&self) -> &'static str {
        if self.os == "windows" { ".exe" } else { "" }
    }
}

fn host_target() -> Target {
    let os = match env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
    .to_string();
    let arch = match env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
    .to_string();
    let triple = target_triple(&os, &arch).map(str::to_string);
    Target { os, arch, triple }
}

fn target_triple(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("darwin", "amd64") => Some("x86_64-apple-darwin"),
        ("darwin", "arm64") => Some("aarch64-apple-darwin"),
        ("linux", "amd64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "arm64") => Some("aarch64-unknown-linux-gnu"),
        ("windows", "amd64") => Some("x86_64-pc-windows-gnu"),
        _ => None,
    }
}

fn supported_targets() -> &'static str {
    "darwin/amd64, darwin/arm64, linux/amd64, linux/arm64, windows/amd64"
}

fn default_output_name(target: &Target) -> &'static str {
    if target.os == "windows" {
        "a.exe"
    } else {
        "a.out"
    }
}

fn runtime_archive(target: &Target) -> Result<PathBuf, CliError> {
    if let Some(path) = runtime_archive_from_env() {
        return require_runtime_archive(path);
    }
    if target.is_cross() {
        return Err(CliError::other(format!(
            "cross-compiling with the Rust CLI requires {RUNTIME_ARCHIVE_ENV} to point at a runtime archive for target {}/{}",
            target.os, target.arch
        )));
    }
    if let Some(path) = runtime_archive_next_to_exe()? {
        return Ok(path);
    }

    let root = repo_root()?;
    build_rust_runtime(&root)?;
    require_runtime_archive(
        root.join("target")
            .join("release")
            .join(rust_runtime_archive_name()),
    )
}

fn runtime_archive_from_env() -> Option<PathBuf> {
    let path = env::var_os(RUNTIME_ARCHIVE_ENV)?;
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn runtime_archive_next_to_exe() -> Result<Option<PathBuf>, CliError> {
    let exe = env::current_exe()?;
    let Some(dir) = exe.parent() else {
        return Ok(None);
    };
    for name in rust_runtime_archive_names() {
        let archive = dir.join(name);
        if archive.is_file() {
            return Ok(Some(archive));
        }
    }
    Ok(None)
}

fn require_runtime_archive(path: PathBuf) -> Result<PathBuf, CliError> {
    if path.is_file() {
        return Ok(path);
    }
    Err(CliError::other(format!(
        "rust runtime archive not found at {}",
        path.display()
    )))
}

fn build_rust_runtime(root: &Path) -> Result<(), CliError> {
    let output = Command::new("cargo")
        .args(["build", "-p", "yar-runtime", "--release"])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(CliError::other(format!(
            "cargo build -p yar-runtime --release failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

fn invoke_clang(
    target: &Target,
    ir_path: &Path,
    runtime_archive: &Path,
    output_path: &Path,
) -> Result<(), CliError> {
    let cc = env::var("CC").unwrap_or_else(|_| "clang".to_string());
    let mut args = vec![OsString::from("-Wno-override-module")];
    if let Some(triple) = &target.triple {
        args.push(OsString::from(format!("--target={triple}")));
    }
    if target.os != "windows" {
        args.push(OsString::from("-pthread"));
    }
    args.push(ir_path.as_os_str().to_owned());
    args.push(runtime_archive.as_os_str().to_owned());
    args.push(OsString::from("-o"));
    args.push(output_path.as_os_str().to_owned());
    if target.os == "windows" {
        args.push(OsString::from("-lws2_32"));
    }

    let output = Command::new(&cc).args(&args).output()?;
    if !output.status.success() {
        return Err(CliError::other(format!(
            "{cc} failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

fn invoke_program(path: &Path) -> Result<(), CliError> {
    let status = Command::new(path).status()?;
    if !status.success() {
        return Err(CliError::ProcessExit(status.code().unwrap_or(1)));
    }
    Ok(())
}

fn repo_root() -> Result<PathBuf, CliError> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("Cargo.toml").exists() && dir.join("crates/yar-runtime").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(CliError::other("cannot locate repository root"));
        }
    }
}

fn rust_runtime_archive_name() -> &'static str {
    rust_runtime_archive_names()[0]
}

fn rust_runtime_archive_names() -> &'static [&'static str] {
    if env::consts::OS == "windows" {
        &["yar_runtime.lib", "libyar_runtime.a"]
    } else {
        &["libyar_runtime.a"]
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[derive(Debug)]
enum CliError {
    Usage(String),
    Diagnostics {
        path: String,
        diagnostics: Vec<diag::Diagnostic>,
    },
    ProcessExit(i32),
    Other(Box<dyn Error>),
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        CliError::Usage(message.into())
    }

    fn other(message: impl Into<String>) -> Self {
        CliError::Other(message.into().into())
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Usage(message) => f.write_str(message),
            CliError::Diagnostics { .. } => f.write_str("diagnostics available"),
            CliError::ProcessExit(code) => write!(f, "program exited with status {code}"),
            CliError::Other(err) => write!(f, "{err}"),
        }
    }
}

impl Error for CliError {}

impl From<yar_compiler::compile::CompileError> for CliError {
    fn from(value: yar_compiler::compile::CompileError) -> Self {
        CliError::Other(Box::new(value))
    }
}

impl From<ManifestError> for CliError {
    fn from(value: ManifestError) -> Self {
        CliError::Other(Box::new(value))
    }
}

impl From<io::Error> for CliError {
    fn from(value: io::Error) -> Self {
        CliError::Other(Box::new(value))
    }
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path)?;
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
