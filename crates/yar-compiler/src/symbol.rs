use sha2::{Digest, Sha256};

use crate::ast::{Package, PackageId, SourceId};

// `$` cannot appear in Yar identifiers or import segments, so this namespace
// can be recognized without confusing it with source-written names.
const INTERNAL_PACKAGE_PREFIX: &str = "$yar$";
const SHA256_HEX_LEN: usize = 64;

pub(crate) fn canonical_decl_name(entry: &PackageId, package: &Package, name: &str) -> String {
    let prefix = match &package.id.source {
        SourceId::Entry if &package.id == entry => package.name.clone(),
        SourceId::Stdlib => package.id.subpath.replace('/', "."),
        _ => format!(
            "{INTERNAL_PACKAGE_PREFIX}{}.{}",
            package_id_digest(&package.id),
            package.name
        ),
    };
    format!("{prefix}.{name}")
}

pub(crate) fn sanitize_diagnostic_message(message: String) -> String {
    if !message.contains(INTERNAL_PACKAGE_PREFIX) {
        return message;
    }

    let mut output = String::with_capacity(message.len());
    let mut remaining = message.as_str();
    while let Some(offset) = remaining.find(INTERNAL_PACKAGE_PREFIX) {
        output.push_str(&remaining[..offset]);
        let candidate = &remaining[offset..];
        if let Some(rest) = strip_internal_package_prefix(candidate) {
            remaining = rest;
        } else {
            output.push_str(INTERNAL_PACKAGE_PREFIX);
            remaining = &candidate[INTERNAL_PACKAGE_PREFIX.len()..];
        }
    }
    output.push_str(remaining);
    output
}

fn strip_internal_package_prefix(name: &str) -> Option<&str> {
    let suffix = name.strip_prefix(INTERNAL_PACKAGE_PREFIX)?;
    let digest = suffix.get(..SHA256_HEX_LEN)?;
    if !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    suffix.get(SHA256_HEX_LEN..)?.strip_prefix('.')
}

fn package_id_digest(id: &PackageId) -> String {
    let mut hasher = Sha256::new();
    match &id.source {
        SourceId::Entry => hasher.update(b"entry"),
        SourceId::Path { manifest_path } => {
            hasher.update(b"path");
            hash_field(&mut hasher, manifest_path.as_bytes());
        }
        SourceId::Git { git, commit } => {
            hasher.update(b"git");
            hash_field(&mut hasher, git.as_bytes());
            hash_field(&mut hasher, commit.as_bytes());
        }
        SourceId::Stdlib => hasher.update(b"stdlib"),
    }
    hash_field(&mut hasher, id.subpath.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_only_well_formed_internal_package_prefixes() {
        let digest = "a".repeat(SHA256_HEX_LEN);
        let malformed = format!(
            "type {INTERNAL_PACKAGE_PREFIX}{}.users.User is invalid",
            "a".repeat(SHA256_HEX_LEN - 1)
        );
        assert_eq!(sanitize_diagnostic_message(malformed.clone()), malformed);
        assert_eq!(
            sanitize_diagnostic_message(format!(
                "type {INTERNAL_PACKAGE_PREFIX}{digest}.users.User is invalid"
            )),
            "type users.User is invalid"
        );
        assert_eq!(
            sanitize_diagnostic_message(format!(
                "{INTERNAL_PACKAGE_PREFIX}{digest}.users.User vs {INTERNAL_PACKAGE_PREFIX}{digest}.users.Role"
            )),
            "users.User vs users.Role"
        );
    }
}
