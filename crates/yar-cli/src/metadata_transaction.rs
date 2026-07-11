use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use yar_compiler::manifest::{LOCK_FILE, MANIFEST_FILE};

const ACTIVE_JOURNAL: &str = ".yar-metadata-transaction";
const COMPLETION_MARKER: &str = ".yar-metadata-transaction.complete";
const JOURNAL_VERSION_FILE: &str = "version";
const JOURNAL_VERSION: &[u8] = b"1\n";
const COMPLETION_VERSION: &[u8] = b"1\n";
const JOURNAL_FILES: [&str; 10] = [
    JOURNAL_VERSION_FILE,
    "yar.toml.before",
    "yar.toml.before-absent",
    "yar.toml.after",
    "yar.toml.restore",
    "yar.lock.before",
    "yar.lock.before-absent",
    "yar.lock.after",
    "yar.lock.restore",
    "completion",
];

#[derive(Clone, Copy)]
pub(crate) enum FileChange<'a> {
    Preserve,
    Write(&'a [u8]),
    Delete,
}

#[derive(Clone, Copy)]
pub(crate) struct MetadataChanges<'a> {
    pub(crate) manifest: FileChange<'a>,
    pub(crate) lock: FileChange<'a>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MetadataTarget {
    Manifest,
    Lock,
}

impl MetadataTarget {
    fn file_name(self) -> &'static str {
        match self {
            Self::Manifest => MANIFEST_FILE,
            Self::Lock => LOCK_FILE,
        }
    }

    fn backup_name(self) -> &'static str {
        match self {
            Self::Manifest => "yar.toml.before",
            Self::Lock => "yar.lock.before",
        }
    }

    fn absent_name(self) -> &'static str {
        match self {
            Self::Manifest => "yar.toml.before-absent",
            Self::Lock => "yar.lock.before-absent",
        }
    }

    fn staged_name(self) -> &'static str {
        match self {
            Self::Manifest => "yar.toml.after",
            Self::Lock => "yar.lock.after",
        }
    }

    fn restore_name(self) -> &'static str {
        match self {
            Self::Manifest => "yar.toml.restore",
            Self::Lock => "yar.lock.restore",
        }
    }
}

#[derive(Clone, Copy)]
enum PreparedChange {
    Preserve,
    Write,
    Delete,
}

struct PreparedTransaction {
    root: PathBuf,
    journal: PathBuf,
    manifest: PreparedChange,
    lock: PreparedChange,
}

enum ApplyError {
    BeforeCommit(io::Error),
    AfterCommit(io::Error),
}

pub(crate) fn publish(root: &Path, changes: MetadataChanges<'_>) -> io::Result<()> {
    publish_with_hooks(root, changes, |_| Ok(()), || Ok(()))
}

pub(crate) fn recover(root: &Path) -> io::Result<()> {
    if completion_marker_exists(root)? {
        return finish_completed_transaction(root);
    }

    let journal = root.join(ACTIVE_JOURNAL);
    if !journal_directory_exists(&journal)? {
        return Ok(());
    }
    validate_journal(&journal)?;

    restore_target(root, &journal, MetadataTarget::Manifest)?;
    sync_directory(root)?;
    restore_target(root, &journal, MetadataTarget::Lock)?;
    sync_directory(root)?;

    mark_transaction_complete(root, &journal)?;
    finish_completed_transaction(root)
}

#[cfg(test)]
fn publish_with_hook<F>(
    root: &Path,
    changes: MetadataChanges<'_>,
    before_publish: F,
) -> io::Result<()>
where
    F: FnMut(MetadataTarget) -> io::Result<()>,
{
    publish_with_hooks(root, changes, before_publish, || Ok(()))
}

fn publish_with_hooks<F, C>(
    root: &Path,
    changes: MetadataChanges<'_>,
    mut before_publish: F,
    mut before_cleanup: C,
) -> io::Result<()>
where
    F: FnMut(MetadataTarget) -> io::Result<()>,
    C: FnMut() -> io::Result<()>,
{
    recover(root)?;
    let transaction = match PreparedTransaction::prepare(root, changes) {
        Ok(transaction) => transaction,
        Err(operation_error) => return rollback_before_commit(root, operation_error),
    };
    match transaction.apply_with_hooks(&mut before_publish, &mut before_cleanup) {
        Ok(()) => Ok(()),
        Err(ApplyError::BeforeCommit(operation_error)) => {
            rollback_before_commit(root, operation_error)
        }
        Err(ApplyError::AfterCommit(operation_error)) => match recover(root) {
            Ok(()) => Ok(()),
            Err(cleanup_error) => Err(io::Error::other(format!(
                "dependency metadata was committed, but cleanup is pending: {operation_error}; {cleanup_error}"
            ))),
        },
    }
}

fn rollback_before_commit(root: &Path, operation_error: io::Error) -> io::Result<()> {
    match recover(root) {
        Ok(()) => Err(operation_error),
        Err(recovery_error) => Err(io::Error::other(format!(
            "publishing dependency metadata failed: {operation_error}; recovery also failed: {recovery_error}"
        ))),
    }
}

impl PreparedTransaction {
    fn prepare(root: &Path, changes: MetadataChanges<'_>) -> io::Result<Self> {
        let journal = root.join(ACTIVE_JOURNAL);
        if path_exists(&journal)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "dependency metadata journal {} already exists",
                    journal.display()
                ),
            ));
        }

        let temp = root.join(format!(
            "{ACTIVE_JOURNAL}.preparing-{}-{}",
            std::process::id(),
            unique_timestamp()
        ));
        create_private_directory(&temp)?;

        let prepare_result = (|| {
            write_synced_file(&temp.join(JOURNAL_VERSION_FILE), JOURNAL_VERSION)?;
            let manifest_permissions = capture_target(root, &temp, MetadataTarget::Manifest)?;
            let lock_permissions = capture_target(root, &temp, MetadataTarget::Lock)?;
            stage_change(
                &temp,
                MetadataTarget::Manifest,
                changes.manifest,
                manifest_permissions.as_ref(),
            )?;
            stage_change(
                &temp,
                MetadataTarget::Lock,
                changes.lock,
                lock_permissions.as_ref(),
            )?;
            sync_directory(&temp)?;
            fs::rename(&temp, &journal)?;
            sync_directory(root)
        })();
        if let Err(error) = prepare_result {
            let _ = fs::remove_dir_all(&temp);
            return Err(error);
        }

        Ok(Self {
            root: root.to_path_buf(),
            journal,
            manifest: prepared_change(changes.manifest),
            lock: prepared_change(changes.lock),
        })
    }

    fn apply_with_hooks<F, C>(
        self,
        before_publish: &mut F,
        before_cleanup: &mut C,
    ) -> Result<(), ApplyError>
    where
        F: FnMut(MetadataTarget) -> io::Result<()>,
        C: FnMut() -> io::Result<()>,
    {
        before_publish(MetadataTarget::Manifest).map_err(ApplyError::BeforeCommit)?;
        self.publish_target(MetadataTarget::Manifest, self.manifest)
            .map_err(ApplyError::BeforeCommit)?;
        sync_directory(&self.root).map_err(ApplyError::BeforeCommit)?;

        before_publish(MetadataTarget::Lock).map_err(ApplyError::BeforeCommit)?;
        self.publish_target(MetadataTarget::Lock, self.lock)
            .map_err(ApplyError::BeforeCommit)?;
        sync_directory(&self.root).map_err(ApplyError::BeforeCommit)?;

        if let Err(error) = mark_transaction_complete(&self.root, &self.journal) {
            return match completion_marker_exists(&self.root) {
                Ok(true) => Err(ApplyError::AfterCommit(error)),
                Ok(false) => Err(ApplyError::BeforeCommit(error)),
                Err(marker_error) => Err(ApplyError::BeforeCommit(io::Error::other(format!(
                    "marking transaction complete failed: {error}; checking commit state also failed: {marker_error}"
                )))),
            };
        }
        before_cleanup().map_err(ApplyError::AfterCommit)?;
        finish_completed_transaction(&self.root).map_err(ApplyError::AfterCommit)
    }

    fn publish_target(&self, target: MetadataTarget, change: PreparedChange) -> io::Result<()> {
        let destination = self.root.join(target.file_name());
        match change {
            PreparedChange::Preserve => Ok(()),
            PreparedChange::Write => {
                replace_with_staged(&self.journal.join(target.staged_name()), &destination)
            }
            PreparedChange::Delete => remove_file_if_present(&destination),
        }
    }
}

fn prepared_change(change: FileChange<'_>) -> PreparedChange {
    match change {
        FileChange::Preserve => PreparedChange::Preserve,
        FileChange::Write(_) => PreparedChange::Write,
        FileChange::Delete => PreparedChange::Delete,
    }
}

fn capture_target(
    root: &Path,
    journal: &Path,
    target: MetadataTarget,
) -> io::Result<Option<fs::Permissions>> {
    let path = root.join(target.file_name());
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            let content = fs::read(&path)?;
            let permissions = metadata.permissions();
            write_synced_file_with_permissions(
                &journal.join(target.backup_name()),
                &content,
                Some(&permissions),
            )?;
            Ok(Some(permissions))
        }
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "dependency metadata path {} is not a regular file",
                path.display()
            ),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            write_synced_file(&journal.join(target.absent_name()), b"")?;
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

fn stage_change(
    journal: &Path,
    target: MetadataTarget,
    change: FileChange<'_>,
    permissions: Option<&fs::Permissions>,
) -> io::Result<()> {
    match change {
        FileChange::Write(content) => write_synced_file_with_permissions(
            &journal.join(target.staged_name()),
            content,
            permissions,
        ),
        FileChange::Preserve | FileChange::Delete => Ok(()),
    }
}

fn restore_target(root: &Path, journal: &Path, target: MetadataTarget) -> io::Result<()> {
    let backup = journal.join(target.backup_name());
    let absent = journal.join(target.absent_name());
    let has_backup = regular_file_exists(&backup)?;
    let was_absent = regular_file_exists(&absent)?;
    match (has_backup, was_absent) {
        (true, false) => restore_from_backup(
            journal,
            &backup,
            &root.join(target.file_name()),
            target.restore_name(),
        ),
        (false, true) => remove_file_if_present(&root.join(target.file_name())),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "dependency metadata journal {} has an invalid saved state for {}",
                journal.display(),
                target.file_name()
            ),
        )),
    }
}

fn validate_journal(journal: &Path) -> io::Result<()> {
    if !regular_file_exists(&journal.join(JOURNAL_VERSION_FILE))? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "dependency metadata journal {} has no version",
                journal.display()
            ),
        ));
    }
    let version = fs::read(journal.join(JOURNAL_VERSION_FILE))?;
    if version != JOURNAL_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported dependency metadata journal at {}",
                journal.display()
            ),
        ));
    }
    for target in [MetadataTarget::Manifest, MetadataTarget::Lock] {
        let has_backup = regular_file_exists(&journal.join(target.backup_name()))?;
        let was_absent = regular_file_exists(&journal.join(target.absent_name()))?;
        if has_backup == was_absent {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "dependency metadata journal {} has an invalid saved state for {}",
                    journal.display(),
                    target.file_name()
                ),
            ));
        }
    }
    Ok(())
}

fn mark_transaction_complete(root: &Path, journal: &Path) -> io::Result<()> {
    let staged = journal.join("completion");
    if regular_file_exists(&staged)? {
        if fs::read(&staged)? != COMPLETION_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid transaction completion state {}", staged.display()),
            ));
        }
    } else {
        write_synced_file(&staged, COMPLETION_VERSION)?;
    }
    fs::rename(staged, root.join(COMPLETION_MARKER))?;
    sync_directory(root)
}

fn completion_marker_exists(root: &Path) -> io::Result<bool> {
    let marker = root.join(COMPLETION_MARKER);
    if !regular_file_exists(&marker)? {
        return Ok(false);
    }
    if fs::read(&marker)? != COMPLETION_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unsupported dependency metadata completion marker {}",
                marker.display()
            ),
        ));
    }
    Ok(true)
}

fn finish_completed_transaction(root: &Path) -> io::Result<()> {
    let journal = root.join(ACTIVE_JOURNAL);
    if journal_directory_exists(&journal)? {
        remove_known_journal_files(&journal)?;
        sync_directory(root)?;
    }
    fs::remove_file(root.join(COMPLETION_MARKER))?;
    sync_directory(root)
}

fn remove_known_journal_files(journal: &Path) -> io::Result<()> {
    for entry in fs::read_dir(journal)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "dependency metadata journal {} contains an unknown entry",
                    journal.display()
                ),
            ));
        };
        if !JOURNAL_FILES.contains(&name) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "dependency metadata journal {} contains unknown entry {name:?}",
                    journal.display()
                ),
            ));
        }
    }
    for name in JOURNAL_FILES {
        remove_file_if_present(&journal.join(name))?;
    }
    fs::remove_dir(journal)
}

fn journal_directory_exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "dependency metadata journal {} is not a directory",
                path.display()
            ),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn regular_file_exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("transaction state {} is not a regular file", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn path_exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn remove_file_if_present(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
            fs::remove_file(path)
        }
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("dependency metadata path {} is not a file", path.display()),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn replace_with_staged(staged: &Path, destination: &Path) -> io::Result<()> {
    match fs::rename(staged, destination) {
        Ok(()) => Ok(()),
        Err(first_error) => match fs::symlink_metadata(destination) {
            Ok(metadata) if metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
                fs::remove_file(destination)?;
                fs::rename(staged, destination)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Err(first_error),
            Ok(_) | Err(_) => Err(first_error),
        },
    }
}

fn restore_from_backup(
    journal: &Path,
    backup: &Path,
    destination: &Path,
    restore_name: &str,
) -> io::Result<()> {
    let staged = journal.join(restore_name);
    remove_file_if_present(&staged)?;
    fs::copy(backup, &staged)?;
    let permissions = fs::metadata(backup)?.permissions();
    fs::set_permissions(&staged, permissions)?;
    File::open(&staged)?.sync_all()?;
    let result = replace_with_staged(&staged, destination);
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

fn write_synced_file(path: &Path, content: &[u8]) -> io::Result<()> {
    write_synced_file_with_permissions(path, content, None)
}

fn write_synced_file_with_permissions(
    path: &Path,
    content: &[u8],
    permissions: Option<&fs::Permissions>,
) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let result = file.write_all(content).and_then(|()| {
        if let Some(permissions) = permissions {
            file.set_permissions(permissions.clone())?;
        }
        file.sync_all()
    });
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

#[cfg(unix)]
fn create_private_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path)
}

#[cfg(not(unix))]
fn create_private_directory(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn unique_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_publication_failure_restores_existing_and_absent_pairs() {
        for old_pair in [
            Some((b"old manifest".as_slice(), b"old lock".as_slice())),
            None,
        ] {
            let dir = TestDir::new("yar-metadata-second-publish");
            if let Some((manifest, lock)) = old_pair {
                fs::write(dir.path().join(MANIFEST_FILE), manifest).unwrap();
                fs::write(dir.path().join(LOCK_FILE), lock).unwrap();
                set_private_mode(&dir.path().join(MANIFEST_FILE));
                set_private_mode(&dir.path().join(LOCK_FILE));
            }

            let mut failed = false;
            let error = publish_with_hook(
                dir.path(),
                MetadataChanges {
                    manifest: FileChange::Write(b"new manifest"),
                    lock: FileChange::Write(b"new lock"),
                },
                |target| {
                    if target == MetadataTarget::Lock && !failed {
                        assert_eq!(
                            fs::read(dir.path().join(MANIFEST_FILE)).unwrap(),
                            b"new manifest"
                        );
                        if let Some((_, lock)) = old_pair {
                            assert_eq!(fs::read(dir.path().join(LOCK_FILE)).unwrap(), lock);
                        } else {
                            assert!(!dir.path().join(LOCK_FILE).exists());
                        }
                        failed = true;
                        return Err(io::Error::other("injected lock publication failure"));
                    }
                    Ok(())
                },
            )
            .unwrap_err();

            assert!(
                error
                    .to_string()
                    .contains("injected lock publication failure")
            );
            match old_pair {
                Some((manifest, lock)) => {
                    assert_eq!(fs::read(dir.path().join(MANIFEST_FILE)).unwrap(), manifest);
                    assert_eq!(fs::read(dir.path().join(LOCK_FILE)).unwrap(), lock);
                    assert_private_mode(&dir.path().join(MANIFEST_FILE));
                    assert_private_mode(&dir.path().join(LOCK_FILE));
                }
                None => {
                    assert!(!dir.path().join(MANIFEST_FILE).exists());
                    assert!(!dir.path().join(LOCK_FILE).exists());
                }
            }
            assert_no_journal(dir.path());
        }
    }

    #[test]
    fn failed_lock_delete_restores_both_files() {
        let dir = TestDir::new("yar-metadata-delete-failure");
        fs::write(dir.path().join(MANIFEST_FILE), b"old manifest").unwrap();
        fs::write(dir.path().join(LOCK_FILE), b"old lock").unwrap();

        let error = publish_with_hook(
            dir.path(),
            MetadataChanges {
                manifest: FileChange::Write(b"new manifest"),
                lock: FileChange::Delete,
            },
            |target| {
                if target == MetadataTarget::Lock {
                    assert_eq!(
                        fs::read(dir.path().join(MANIFEST_FILE)).unwrap(),
                        b"new manifest"
                    );
                    assert_eq!(fs::read(dir.path().join(LOCK_FILE)).unwrap(), b"old lock");
                    Err(io::Error::other("injected lock deletion failure"))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("injected lock deletion failure"));
        assert_eq!(
            fs::read(dir.path().join(MANIFEST_FILE)).unwrap(),
            b"old manifest"
        );
        assert_eq!(fs::read(dir.path().join(LOCK_FILE)).unwrap(), b"old lock");
        assert_no_journal(dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn successful_publication_preserves_existing_metadata_permissions() {
        let dir = TestDir::new("yar-metadata-permissions");
        let manifest = dir.path().join(MANIFEST_FILE);
        let lock = dir.path().join(LOCK_FILE);
        fs::write(&manifest, b"old manifest").unwrap();
        fs::write(&lock, b"old lock").unwrap();
        set_private_mode(&manifest);
        set_private_mode(&lock);

        publish(
            dir.path(),
            MetadataChanges {
                manifest: FileChange::Write(b"new manifest"),
                lock: FileChange::Write(b"new lock"),
            },
        )
        .unwrap();

        assert_eq!(fs::read(&manifest).unwrap(), b"new manifest");
        assert_eq!(fs::read(&lock).unwrap(), b"new lock");
        assert_private_mode(&manifest);
        assert_private_mode(&lock);
        assert_no_journal(dir.path());
    }

    #[test]
    fn post_commit_cleanup_error_is_recovered_as_success() {
        let dir = TestDir::new("yar-metadata-post-commit-cleanup");
        fs::write(dir.path().join(MANIFEST_FILE), b"old manifest").unwrap();
        fs::write(dir.path().join(LOCK_FILE), b"old lock").unwrap();
        let mut failed = false;

        publish_with_hooks(
            dir.path(),
            MetadataChanges {
                manifest: FileChange::Write(b"new manifest"),
                lock: FileChange::Write(b"new lock"),
            },
            |_| Ok(()),
            || {
                if !failed {
                    failed = true;
                    return Err(io::Error::other("injected cleanup failure"));
                }
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(
            fs::read(dir.path().join(MANIFEST_FILE)).unwrap(),
            b"new manifest"
        );
        assert_eq!(fs::read(dir.path().join(LOCK_FILE)).unwrap(), b"new lock");
        assert_no_journal(dir.path());
    }

    #[test]
    fn prepared_journal_recovery_rolls_back_partial_publication_idempotently() {
        let dir = TestDir::new("yar-metadata-prepared-recovery");
        fs::write(dir.path().join(MANIFEST_FILE), b"old manifest").unwrap();
        fs::write(dir.path().join(LOCK_FILE), b"old lock").unwrap();
        let transaction = PreparedTransaction::prepare(
            dir.path(),
            MetadataChanges {
                manifest: FileChange::Write(b"new manifest"),
                lock: FileChange::Write(b"new lock"),
            },
        )
        .unwrap();
        transaction
            .publish_target(MetadataTarget::Manifest, transaction.manifest)
            .unwrap();
        fs::write(
            transaction
                .journal
                .join(MetadataTarget::Manifest.restore_name()),
            b"interrupted restore scratch",
        )
        .unwrap();

        recover(dir.path()).unwrap();
        recover(dir.path()).unwrap();

        assert_eq!(
            fs::read(dir.path().join(MANIFEST_FILE)).unwrap(),
            b"old manifest"
        );
        assert_eq!(fs::read(dir.path().join(LOCK_FILE)).unwrap(), b"old lock");
        assert_no_journal(dir.path());
    }

    #[test]
    fn prepared_delete_recovery_restores_old_lock() {
        let dir = TestDir::new("yar-metadata-prepared-delete");
        fs::write(dir.path().join(MANIFEST_FILE), b"old manifest").unwrap();
        fs::write(dir.path().join(LOCK_FILE), b"old lock").unwrap();
        let transaction = PreparedTransaction::prepare(
            dir.path(),
            MetadataChanges {
                manifest: FileChange::Write(b"new manifest"),
                lock: FileChange::Delete,
            },
        )
        .unwrap();
        transaction
            .publish_target(MetadataTarget::Manifest, transaction.manifest)
            .unwrap();
        transaction
            .publish_target(MetadataTarget::Lock, transaction.lock)
            .unwrap();

        recover(dir.path()).unwrap();

        assert_eq!(
            fs::read(dir.path().join(MANIFEST_FILE)).unwrap(),
            b"old manifest"
        );
        assert_eq!(fs::read(dir.path().join(LOCK_FILE)).unwrap(), b"old lock");
        assert_no_journal(dir.path());
    }

    #[test]
    fn committed_cleanup_recovery_keeps_the_new_pair() {
        let dir = TestDir::new("yar-metadata-committed-cleanup");
        fs::write(dir.path().join(MANIFEST_FILE), b"old manifest").unwrap();
        fs::write(dir.path().join(LOCK_FILE), b"old lock").unwrap();
        let transaction = PreparedTransaction::prepare(
            dir.path(),
            MetadataChanges {
                manifest: FileChange::Write(b"new manifest"),
                lock: FileChange::Write(b"new lock"),
            },
        )
        .unwrap();
        transaction
            .publish_target(MetadataTarget::Manifest, transaction.manifest)
            .unwrap();
        transaction
            .publish_target(MetadataTarget::Lock, transaction.lock)
            .unwrap();
        mark_transaction_complete(dir.path(), &transaction.journal).unwrap();
        fs::remove_file(transaction.journal.join(JOURNAL_VERSION_FILE)).unwrap();
        fs::remove_file(
            transaction
                .journal
                .join(MetadataTarget::Manifest.backup_name()),
        )
        .unwrap();

        recover(dir.path()).unwrap();

        assert_eq!(
            fs::read(dir.path().join(MANIFEST_FILE)).unwrap(),
            b"new manifest"
        );
        assert_eq!(fs::read(dir.path().join(LOCK_FILE)).unwrap(), b"new lock");
        assert_no_journal(dir.path());
    }

    #[test]
    fn marker_only_recovery_keeps_committed_write_and_deletion() {
        let dir = TestDir::new("yar-metadata-marker-only-cleanup");
        fs::write(dir.path().join(MANIFEST_FILE), b"old manifest").unwrap();
        fs::write(dir.path().join(LOCK_FILE), b"old lock").unwrap();
        let transaction = PreparedTransaction::prepare(
            dir.path(),
            MetadataChanges {
                manifest: FileChange::Write(b"new manifest"),
                lock: FileChange::Delete,
            },
        )
        .unwrap();
        transaction
            .publish_target(MetadataTarget::Manifest, transaction.manifest)
            .unwrap();
        transaction
            .publish_target(MetadataTarget::Lock, transaction.lock)
            .unwrap();
        mark_transaction_complete(dir.path(), &transaction.journal).unwrap();
        fs::remove_dir_all(&transaction.journal).unwrap();

        recover(dir.path()).unwrap();
        recover(dir.path()).unwrap();

        assert_eq!(
            fs::read(dir.path().join(MANIFEST_FILE)).unwrap(),
            b"new manifest"
        );
        assert!(!dir.path().join(LOCK_FILE).exists());
        assert_no_journal(dir.path());
    }

    #[test]
    fn committed_cleanup_rejects_unknown_journal_entries_without_deleting_them() {
        let dir = TestDir::new("yar-metadata-unknown-committed-entry");
        fs::write(dir.path().join(MANIFEST_FILE), b"committed manifest").unwrap();
        fs::write(dir.path().join(LOCK_FILE), b"committed lock").unwrap();
        let journal = dir.path().join(ACTIVE_JOURNAL);
        fs::create_dir(&journal).unwrap();
        let sentinel = journal.join("unknown-sentinel");
        fs::write(&sentinel, b"do not delete").unwrap();
        fs::write(dir.path().join(COMPLETION_MARKER), COMPLETION_VERSION).unwrap();

        let error = recover(dir.path()).unwrap_err();

        assert!(error.to_string().contains("contains unknown entry"));
        assert_eq!(fs::read(&sentinel).unwrap(), b"do not delete");
        assert!(dir.path().join(COMPLETION_MARKER).exists());
        assert_eq!(
            fs::read(dir.path().join(MANIFEST_FILE)).unwrap(),
            b"committed manifest"
        );
        assert_eq!(
            fs::read(dir.path().join(LOCK_FILE)).unwrap(),
            b"committed lock"
        );
    }

    #[test]
    fn recovery_ignores_unrecognized_staging_and_rejects_malformed_prepared_state() {
        let dir = TestDir::new("yar-metadata-invalid-recovery");
        fs::write(dir.path().join(MANIFEST_FILE), b"old manifest").unwrap();
        fs::write(dir.path().join(LOCK_FILE), b"old lock").unwrap();
        let preparing = dir
            .path()
            .join(format!("{ACTIVE_JOURNAL}.preparing-unrecognized"));
        fs::create_dir(&preparing).unwrap();
        fs::write(preparing.join("incomplete"), b"state").unwrap();

        recover(dir.path()).unwrap();
        assert!(preparing.exists(), "unrecognized staging state was deleted");
        assert_eq!(
            fs::read(dir.path().join(MANIFEST_FILE)).unwrap(),
            b"old manifest"
        );
        assert_eq!(fs::read(dir.path().join(LOCK_FILE)).unwrap(), b"old lock");

        let prepared = dir.path().join(ACTIVE_JOURNAL);
        fs::create_dir(&prepared).unwrap();
        fs::write(prepared.join(JOURNAL_VERSION_FILE), JOURNAL_VERSION).unwrap();
        let error = recover(dir.path()).unwrap_err();

        assert!(error.to_string().contains("invalid saved state"));
        assert!(prepared.exists(), "malformed journal was deleted");
        assert_eq!(
            fs::read(dir.path().join(MANIFEST_FILE)).unwrap(),
            b"old manifest"
        );
        assert_eq!(fs::read(dir.path().join(LOCK_FILE)).unwrap(), b"old lock");
    }

    fn assert_no_journal(root: &Path) {
        assert!(!root.join(ACTIVE_JOURNAL).exists());
        assert!(!root.join(COMPLETION_MARKER).exists());
        for entry in fs::read_dir(root).unwrap() {
            let name = entry.unwrap().file_name().to_string_lossy().into_owned();
            assert!(
                !name.starts_with(&format!("{ACTIVE_JOURNAL}.preparing-"))
                    && !name.contains(".restore-"),
                "transaction artifact remained: {name}"
            );
        }
    }

    #[cfg(unix)]
    fn set_private_mode(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[cfg(not(unix))]
    fn set_private_mode(_path: &Path) {}

    #[cfg(unix)]
    fn assert_private_mode(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(not(unix))]
    fn assert_private_mode(_path: &Path) {}

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "{prefix}-{}-{}",
                std::process::id(),
                unique_timestamp()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
