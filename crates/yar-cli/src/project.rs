use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

use yar_compiler::manifest::{LOCK_FILE, MANIFEST_FILE};

use crate::metadata_transaction;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectPaths {
    root: PathBuf,
}

impl ProjectPaths {
    pub(crate) fn for_entry(
        cwd: &Path,
        entry: &Path,
        explicit_manifest: Option<&Path>,
    ) -> io::Result<Self> {
        let entry_dir = entry_directory(cwd, entry)?;
        let project = match explicit_manifest {
            Some(path) => {
                let project = Self::from_explicit_manifest(cwd, path)?;
                if !entry_dir.starts_with(&project.root) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "entry {} is outside the selected project root {}",
                            entry_dir.display(),
                            project.root.display()
                        ),
                    ));
                }
                project
            }
            None => Self {
                root: discover_project_root(&entry_dir)?.unwrap_or(entry_dir),
            },
        };
        project.recover()?;
        project.validate_manifest_if_present()?;
        Ok(project)
    }

    pub(crate) fn for_metadata(cwd: &Path, explicit_manifest: Option<&Path>) -> io::Result<Self> {
        let project = match explicit_manifest {
            Some(path) => Self::from_explicit_manifest(cwd, path)?,
            None => Self {
                root: discover_project_root(cwd)?.unwrap_or_else(|| cwd.to_path_buf()),
            },
        };
        project.recover()?;
        project.validate_manifest_if_present()?;
        Ok(project)
    }

    pub(crate) fn for_init(cwd: &Path, explicit_manifest: Option<&Path>) -> io::Result<Self> {
        let project = match explicit_manifest {
            Some(path) => Self::from_explicit_manifest(cwd, path)?,
            None => Self {
                root: cwd.to_path_buf(),
            },
        };
        project.recover()?;
        Ok(project)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn manifest(&self) -> PathBuf {
        self.root.join(MANIFEST_FILE)
    }

    pub(crate) fn lock(&self) -> PathBuf {
        self.root.join(LOCK_FILE)
    }

    pub(crate) fn require_manifest(&self) -> io::Result<()> {
        if self.validate_manifest_if_present()? {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "selected manifest {} does not exist",
                self.manifest().display()
            ),
        ))
    }

    fn validate_manifest_if_present(&self) -> io::Result<bool> {
        match fs::symlink_metadata(self.manifest()) {
            Ok(metadata) if metadata.file_type().is_file() => Ok(true),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "selected manifest {} is not a regular file",
                    self.manifest().display()
                ),
            )),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn from_explicit_manifest(cwd: &Path, path: &Path) -> io::Result<Self> {
        if path.file_name() != Some(OsStr::new(MANIFEST_FILE)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "selected manifest path must name yar.toml",
            ));
        }
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        let parent = absolute
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(cwd);
        let root = fs::canonicalize(parent).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "cannot resolve selected manifest directory {}: {error}",
                    parent.display()
                ),
            )
        })?;
        Ok(Self { root })
    }

    fn recover(&self) -> io::Result<()> {
        metadata_transaction::recover(&self.root).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "recovering dependency metadata in {}: {error}",
                    self.root.display()
                ),
            )
        })
    }
}

fn discover_project_root(start: &Path) -> io::Result<Option<PathBuf>> {
    'discovery: loop {
        for ancestor in start.ancestors() {
            let has_manifest = manifest_candidate_exists(ancestor)?;
            let has_recovery = metadata_transaction::has_recovery_state(ancestor)?;
            if !has_manifest && !has_recovery {
                continue;
            }
            if has_recovery {
                metadata_transaction::recover(ancestor).map_err(|error| {
                    io::Error::new(
                        error.kind(),
                        format!(
                            "recovering dependency metadata in {}: {error}",
                            ancestor.display()
                        ),
                    )
                })?;
                continue 'discovery;
            }
            return Ok(Some(ancestor.to_path_buf()));
        }
        return Ok(None);
    }
}

fn manifest_candidate_exists(root: &Path) -> io::Result<bool> {
    let path = root.join(MANIFEST_FILE);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "manifest candidate {} is not a regular file",
                path.display()
            ),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn entry_directory(cwd: &Path, entry: &Path) -> io::Result<PathBuf> {
    let path = if entry.is_absolute() {
        entry.to_path_buf()
    } else {
        cwd.join(entry)
    };
    let metadata = fs::metadata(&path)?;
    let directory = if metadata.is_dir() {
        path
    } else {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(cwd)
            .to_path_buf()
    };
    fs::canonicalize(directory)
}
