use std::{
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

pub const MANIFEST_FILE: &str = "yar-runtime.toml";
pub const FORMAT_VERSION: u32 = 1;
pub const RUNTIME_ABI: u32 = 2;
pub const COMPILER_COMPATIBILITY: u32 = 1;

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

#[derive(Debug)]
pub struct RuntimeBundle {
    archive: PathBuf,
    system_libraries: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleManifest {
    format: u32,
    target: String,
    runtime_abi: u32,
    compiler_compatibility: u32,
    archive: String,
    link: LinkManifest,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkManifest {
    system_libraries: Vec<String>,
}

impl RuntimeBundle {
    pub fn load(directory: &Path, expected_target: &str) -> Result<Self, String> {
        Self::load_with_archive(directory, expected_target, None)
    }

    pub fn load_with_archive(
        directory: &Path,
        expected_target: &str,
        archive_override: Option<PathBuf>,
    ) -> Result<Self, String> {
        let manifest_path = directory.join(MANIFEST_FILE);
        let metadata = fs::symlink_metadata(&manifest_path).map_err(|error| {
            format!(
                "reading runtime bundle manifest {}: {error}",
                manifest_path.display()
            )
        })?;
        if !metadata.file_type().is_file() {
            return Err(format!(
                "runtime bundle manifest is not a regular file: {}",
                manifest_path.display()
            ));
        }
        if metadata.len() > MAX_MANIFEST_BYTES {
            return Err(format!(
                "runtime bundle manifest {} exceeds {MAX_MANIFEST_BYTES} bytes",
                manifest_path.display()
            ));
        }
        let mut source = String::new();
        File::open(&manifest_path)
            .and_then(|file| {
                file.take(MAX_MANIFEST_BYTES + 1)
                    .read_to_string(&mut source)
            })
            .map_err(|error| {
                format!(
                    "reading runtime bundle manifest {}: {error}",
                    manifest_path.display()
                )
            })?;
        if source.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(format!(
                "runtime bundle manifest {} exceeds {MAX_MANIFEST_BYTES} bytes",
                manifest_path.display()
            ));
        }
        let manifest: BundleManifest = toml::from_str(&source).map_err(|error| {
            format!(
                "parsing runtime bundle manifest {}: {error}",
                manifest_path.display()
            )
        })?;
        validate_manifest(&manifest, expected_target)?;

        let archive = match archive_override {
            Some(archive) => {
                if archive.file_name() != Some(std::ffi::OsStr::new(&manifest.archive)) {
                    return Err(format!(
                        "runtime bundle archive override {} does not match declared archive {:?}",
                        archive.display(),
                        manifest.archive
                    ));
                }
                archive
            }
            None => directory.join(&manifest.archive),
        };
        let archive_metadata = fs::symlink_metadata(&archive).map_err(|error| {
            format!(
                "reading runtime bundle archive {}: {error}",
                archive.display()
            )
        })?;
        if !archive_metadata.file_type().is_file() {
            return Err(format!(
                "runtime bundle archive is not a regular file: {}",
                archive.display()
            ));
        }

        Ok(Self {
            archive,
            system_libraries: manifest.link.system_libraries,
        })
    }

    pub fn archive(&self) -> &Path {
        &self.archive
    }

    pub fn append_library_args(&self, args: &mut Vec<std::ffi::OsString>) {
        args.extend(
            self.system_libraries
                .iter()
                .map(|library| format!("-l{library}").into()),
        );
    }
}

fn validate_manifest(manifest: &BundleManifest, expected_target: &str) -> Result<(), String> {
    if manifest.format != FORMAT_VERSION {
        return Err(format!(
            "unsupported runtime bundle format {}; expected {FORMAT_VERSION}",
            manifest.format
        ));
    }
    if manifest.target != expected_target {
        return Err(format!(
            "runtime bundle targets {}; selected target is {expected_target}",
            manifest.target
        ));
    }
    if manifest.runtime_abi != RUNTIME_ABI {
        return Err(format!(
            "runtime bundle ABI {} is incompatible with compiler ABI {RUNTIME_ABI}",
            manifest.runtime_abi
        ));
    }
    if manifest.compiler_compatibility != COMPILER_COMPATIBILITY {
        return Err(format!(
            "runtime bundle compiler compatibility {} does not match compiler compatibility {COMPILER_COMPATIBILITY}",
            manifest.compiler_compatibility
        ));
    }
    validate_archive_name(&manifest.archive)?;

    for library in &manifest.link.system_libraries {
        if !valid_library_name(library) {
            return Err(format!(
                "invalid runtime bundle system library name {library:?}"
            ));
        }
    }
    Ok(())
}

fn validate_archive_name(archive: &str) -> Result<(), String> {
    let mut components = Path::new(archive).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(format!(
            "runtime bundle archive must be one relative filename, got {archive:?}"
        ));
    }
    Ok(())
}

fn valid_library_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+' | b'-' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn loads_a_valid_bundle_and_translates_typed_link_metadata() {
        let dir = bundle_dir("valid");
        write_bundle(
            &dir,
            r#"format = 1
target = "x86_64-unknown-linux-gnu"
runtime_abi = 2
compiler_compatibility = 1
archive = "libyar_runtime.a"

[link]
system_libraries = ["m", "m", "dl"]
"#,
        );

        let bundle = RuntimeBundle::load(&dir, "x86_64-unknown-linux-gnu").unwrap();
        let mut args = Vec::new();
        args.push(bundle.archive().as_os_str().to_owned());
        bundle.append_library_args(&mut args);
        assert_eq!(
            args,
            [
                dir.join("libyar_runtime.a").into_os_string(),
                "-lm".into(),
                "-lm".into(),
                "-ldl".into(),
            ]
        );
    }

    #[test]
    fn rejects_mismatched_contracts_and_unsafe_link_inputs() {
        for (name, manifest, message) in [
            (
                "format",
                manifest_with("format = 2"),
                "unsupported runtime bundle format",
            ),
            (
                "target",
                manifest_with("target = \"aarch64-unknown-linux-gnu\""),
                "selected target",
            ),
            (
                "abi",
                manifest_with("runtime_abi = 3"),
                "incompatible with compiler ABI",
            ),
            (
                "compatibility",
                manifest_with("compiler_compatibility = 2"),
                "does not match compiler compatibility",
            ),
            (
                "archive",
                manifest_with("archive = \"../runtime.a\""),
                "one relative filename",
            ),
            (
                "library",
                manifest_with("system_libraries = [\"-o\", \"owned\"]"),
                "invalid runtime bundle system library",
            ),
        ] {
            let dir = bundle_dir(name);
            write_bundle(&dir, &manifest);
            let error = RuntimeBundle::load(&dir, "x86_64-unknown-linux-gnu").unwrap_err();
            assert!(error.contains(message), "error: {error}");
        }

        let dir = bundle_dir("unknown-field");
        let manifest = format!("{}\nunknown = true\n", manifest_with("format = 1"));
        write_bundle(&dir, &manifest);
        let error = RuntimeBundle::load(&dir, "x86_64-unknown-linux-gnu").unwrap_err();
        assert!(error.contains("unknown field"), "error: {error}");
    }

    #[test]
    fn checked_in_target_manifests_match_the_compiler_contract() {
        for (target, libraries) in [
            ("x86_64-apple-darwin", &["System", "c", "m"][..]),
            ("aarch64-apple-darwin", &["System", "c", "m"][..]),
            (
                "x86_64-unknown-linux-gnu",
                &["gcc_s", "util", "rt", "pthread", "m", "dl", "c"][..],
            ),
            (
                "aarch64-unknown-linux-gnu",
                &["gcc_s", "util", "rt", "pthread", "m", "dl", "c"][..],
            ),
            (
                "x86_64-pc-windows-gnu",
                &["kernel32", "ntdll", "userenv", "ws2_32", "dbghelp"][..],
            ),
        ] {
            let archive = bundle_dir(target).join("libyar_runtime.a");
            fs::write(&archive, b"archive").unwrap();
            let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join("runtime-bundles")
                .join(target);
            let bundle =
                RuntimeBundle::load_with_archive(&manifest_dir, target, Some(archive)).unwrap();
            let mut args = Vec::new();
            bundle.append_library_args(&mut args);
            assert_eq!(
                args,
                libraries
                    .iter()
                    .map(|library| std::ffi::OsString::from(format!("-l{library}")))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlinked_archive() {
        use std::os::unix::fs::symlink;

        let dir = bundle_dir("symlink");
        fs::write(dir.join("outside.a"), b"archive").unwrap();
        write_bundle(&dir, &manifest_with("format = 1"));
        fs::remove_file(dir.join("libyar_runtime.a")).unwrap();
        symlink(dir.join("outside.a"), dir.join("libyar_runtime.a")).unwrap();

        let error = RuntimeBundle::load(&dir, "x86_64-unknown-linux-gnu").unwrap_err();
        assert!(error.contains("not a regular file"), "error: {error}");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlinked_manifest() {
        use std::os::unix::fs::symlink;

        let dir = bundle_dir("manifest-symlink");
        let manifest = manifest_with("format = 1");
        fs::write(dir.join("outside.toml"), manifest).unwrap();
        fs::write(dir.join("libyar_runtime.a"), b"archive").unwrap();
        symlink(dir.join("outside.toml"), dir.join(MANIFEST_FILE)).unwrap();

        let error = RuntimeBundle::load(&dir, "x86_64-unknown-linux-gnu").unwrap_err();
        assert!(error.contains("not a regular file"), "error: {error}");
    }

    #[test]
    fn rejects_an_oversized_manifest() {
        let dir = bundle_dir("oversized");
        fs::write(
            dir.join(MANIFEST_FILE),
            vec![b' '; usize::try_from(MAX_MANIFEST_BYTES + 1).unwrap()],
        )
        .unwrap();

        let error = RuntimeBundle::load(&dir, "x86_64-unknown-linux-gnu").unwrap_err();
        assert!(error.contains("exceeds"), "error: {error}");
    }

    fn manifest_with(replacement: &str) -> String {
        let base = r#"format = 1
target = "x86_64-unknown-linux-gnu"
runtime_abi = 2
compiler_compatibility = 1
archive = "libyar_runtime.a"

[link]
system_libraries = []
"#;
        let key = replacement.split_once(" = ").unwrap().0;
        base.lines()
            .map(|line| {
                if line.starts_with(&format!("{key} =")) {
                    replacement
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn bundle_dir(name: &str) -> PathBuf {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "yar-runtime-bundle-{name}-{}-{nanos}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_bundle(dir: &Path, manifest: &str) {
        fs::write(dir.join(MANIFEST_FILE), manifest).unwrap();
        fs::write(dir.join("libyar_runtime.a"), b"archive").unwrap();
    }
}
