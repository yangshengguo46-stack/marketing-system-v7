use crate::jcs::canonicalize_value;
use crate::jcs::parse_json;
use crate::secure_fs::*;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
const MARKER: &str = "coordinator/pair-marker.json";
const INVENTORY: &str = "coordinator/private-inventory.jsonl";
#[path = "private_inventory_batch.rs"]
pub(crate) mod batch;
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct PairMarker {
    pub(crate) schema_version: u32,
    pub(crate) pair_id: String,
    pub(crate) frozen_run_context_sha256: String,
    pub(crate) private_root: String,
    pub(crate) inventory_relative_path: String,
    pub(crate) created_at: String,
}
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct VerifiedPrivateInventory {
    canonical_private_root: PathBuf,
    inventory_root_sha256: String,
    pair_marker: PairMarker,
}
impl VerifiedPrivateInventory {
    pub(crate) fn inventory_root_sha256(&self) -> &str {
        &self.inventory_root_sha256
    }
    pub(crate) fn pair_marker(&self) -> &PairMarker {
        &self.pair_marker
    }
    pub(crate) fn verify_binding(
        &self,
        pair_id: &str,
        frozen_context_sha256: &str,
        private_root: &Path,
    ) -> Result<()> {
        if self.canonical_private_root != private_root
            || self.pair_marker.pair_id != pair_id
            || self.pair_marker.frozen_run_context_sha256 != frozen_context_sha256
            || self.pair_marker.private_root
                != private_root.to_str().context("private root UTF-8")?
        {
            bail!("private inventory marker differs from the verified pair binding");
        }
        Ok(())
    }
    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        if verify_private_inventory_state(&self.canonical_private_root)? != *self {
            bail!("private inventory changed after initial verification");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum InventoryKind {
    File,
    Directory,
}
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct InventoryRecord {
    pub(crate) schema_version: u32,
    pub(crate) sequence: u64,
    pub(crate) relative_path: String,
    pub(crate) kind: InventoryKind,
    pub(crate) sha256: Option<String>,
    pub(crate) previous_record_sha256: Option<String>,
}
type TreeEntry = (InventoryKind, Option<String>);
pub(crate) fn bootstrap_private_inventory(
    root: &Path,
    pair_id: &str,
    frozen_sha256: &str,
    created_at: &str,
) -> Result<String> {
    let root_string = canonical_root(root)?;
    if !lower_hex(pair_id) || !lower_hex(frozen_sha256) {
        bail!("pair marker commitments must be lowercase SHA-256");
    }
    chrono::DateTime::parse_from_rfc3339(created_at).context("parse pair marker createdAt")?;
    collect_tree(root).context("preflight private tree before coordinator creation")?;
    let coordinator = resolve_private_relative(root, Path::new("coordinator"))?;
    if coordinator.try_exists()? {
        fsync_directory(&coordinator)?;
    } else {
        create_owner_only_dir_new(&coordinator)?;
    }
    let marker_path = resolve_private_relative(root, Path::new(MARKER))?;
    let inventory_path = resolve_private_relative(root, Path::new(INVENTORY))?;
    for path in [&marker_path, &inventory_path] {
        match fs::symlink_metadata(path) {
            Ok(_) => bail!("private inventory destination already exists"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect private inventory destination"),
        }
    }
    let marker = PairMarker {
        schema_version: 1,
        pair_id: pair_id.to_string(),
        frozen_run_context_sha256: frozen_sha256.to_string(),
        private_root: root_string.to_string(),
        inventory_relative_path: INVENTORY.to_string(),
        created_at: created_at.to_string(),
    };
    let marker_bytes = canonical(&marker)?;
    let mut tree = collect_tree(root)?;
    let before_marker = tree.clone();
    tree.insert(
        MARKER.to_string(),
        (InventoryKind::File, Some(digest(&marker_bytes))),
    );
    let records = tree
        .into_iter()
        .enumerate()
        .map(|(index, (relative_path, entry))| {
            Ok(InventoryRecord {
                schema_version: 1,
                sequence: u64::try_from(index + 1)?,
                relative_path,
                kind: entry.0,
                sha256: entry.1,
                previous_record_sha256: None,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let inventory_bytes = encode_chain(records)?;
    parse_chain(&inventory_bytes)?;
    write_owner_only_new(&inventory_path, &inventory_bytes)?;
    if collect_tree(root)? != before_marker {
        bail!("private tree changed before inventory commit marker");
    }
    write_owner_only_new(&marker_path, &marker_bytes)?;
    fsync_directory(&coordinator)?;
    fsync_directory(root)?;
    verify_private_inventory(root)
}
pub(crate) fn verify_private_inventory(root: &Path) -> Result<String> {
    Ok(verify_private_inventory_state(root)?.inventory_root_sha256)
}
pub(crate) fn verify_private_inventory_state(root: &Path) -> Result<VerifiedPrivateInventory> {
    let (_, bytes, pair_marker) = verified_state(root, None, None)?;
    Ok(VerifiedPrivateInventory {
        canonical_private_root: root.to_path_buf(),
        inventory_root_sha256: digest(&bytes),
        pair_marker,
    })
}
pub(crate) fn append_private_inventory(root: &Path, relative: &Path) -> Result<String> {
    let relative = normalized(relative)?;
    if relative == INVENTORY {
        bail!("inventory path is reserved");
    }
    let tree = collect_tree(root)?;
    let entry = tree
        .get(&relative)
        .context("inventory append target is absent")?;
    let allowed_new = BTreeMap::from([(relative.clone(), entry.clone())]);
    let inventory_path = resolve_private_relative(root, Path::new(INVENTORY))?;
    let (mut inventory_file, old_bytes) = open_inventory_append(&inventory_path)?;
    let (records, _, _) = verified_state(root, Some(&allowed_new), Some(old_bytes.clone()))?;
    let previous = records.last().map(canonical).transpose()?;
    let record = InventoryRecord {
        schema_version: 1,
        sequence: u64::try_from(records.len() + 1)?,
        relative_path: relative,
        kind: entry.0,
        sha256: entry.1.clone(),
        previous_record_sha256: previous.as_deref().map(digest),
    };
    let mut line = canonical(&record)?;
    line.push(b'\n');
    append_verified(&mut inventory_file, &line, &old_bytes)?;
    fsync_directory(&resolve_private_relative(root, Path::new("coordinator"))?)?;
    drop(inventory_file);
    let mut complete = old_bytes;
    complete.extend(line);
    let expected = digest(&complete);
    let actual = verify_private_inventory(root)?;
    if actual != expected {
        bail!("inventory root changed after append");
    }
    Ok(actual)
}
fn verified_state(
    root: &Path,
    allowed_new: Option<&BTreeMap<String, TreeEntry>>,
    supplied_bytes: Option<Vec<u8>>,
) -> Result<(Vec<InventoryRecord>, Vec<u8>, PairMarker)> {
    let root_string = canonical_root(root)?;
    let inventory_path = resolve_private_relative(root, Path::new(INVENTORY))?;
    let bytes = match supplied_bytes {
        Some(bytes) => bytes,
        None => {
            let bytes = read_single_link_regular(&inventory_path)?;
            #[cfg(windows)]
            if crate::runner::read_private_existing_no_follow(&inventory_path)? != bytes {
                bail!("inventory retained read disagrees with secure read");
            }
            bytes
        }
    };
    let records = parse_chain(&bytes)?;
    let marker_bytes =
        read_single_link_regular(&resolve_private_relative(root, Path::new(MARKER))?)?;
    let marker: PairMarker = serde_json::from_value(parse_json(&marker_bytes)?)?;
    if marker.schema_version != 1
        || canonical(&marker)? != marker_bytes
        || !lower_hex(&marker.pair_id)
        || !lower_hex(&marker.frozen_run_context_sha256)
        || chrono::DateTime::parse_from_rfc3339(&marker.created_at).is_err()
        || marker.private_root != root_string
        || marker.inventory_relative_path != INVENTORY
    {
        bail!("pair marker is not bound to this private inventory");
    }
    let mut tree = collect_tree(root)?;
    let expected = records
        .iter()
        .map(|record| {
            (
                record.relative_path.clone(),
                (record.kind, record.sha256.clone()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if expected.len() != records.len()
        || records
            .iter()
            .any(|record| record.relative_path == INVENTORY)
    {
        bail!("inventory contains a duplicate or reserved path");
    }
    if let Some(allowed_new) = allowed_new {
        for (path, entry) in allowed_new {
            if expected.contains_key(path) || tree.remove(path).as_ref() != Some(entry) {
                bail!("inventory append target is absent, recorded, or mismatched");
            }
        }
    }
    if tree != expected {
        bail!("private tree differs from its inventory records");
    }
    Ok((records, bytes, marker))
}
fn collect_tree(root: &Path) -> Result<BTreeMap<String, TreeEntry>> {
    fn walk(root: &Path, relative: &Path, output: &mut BTreeMap<String, TreeEntry>) -> Result<()> {
        let directory = root.join(relative);
        crate::runner::validate_private_existing_directory_no_follow(&directory)?;
        let mut children = fs::read_dir(&directory)?.collect::<std::io::Result<Vec<_>>>()?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let child_relative = relative.join(child.file_name());
            let normalized = normalized(&child_relative)?;
            if normalized == INVENTORY {
                continue;
            }
            let path = root.join(&child_relative);
            let metadata = fs::symlink_metadata(&path)?;
            let entry = if metadata.is_dir() {
                #[cfg(windows)]
                crate::runner::validate_private_existing_directory_no_follow(&path)?;
                (InventoryKind::Directory, None)
            } else if metadata.is_file() {
                let bytes = crate::runner::read_private_existing_no_follow(&path)?;
                (InventoryKind::File, Some(digest(&bytes)))
            } else {
                bail!("private tree contains a link or special entry");
            };
            output.insert(normalized, entry);
            if metadata.is_dir() {
                walk(root, &child_relative, output)?;
            }
        }
        Ok(())
    }
    let mut output = BTreeMap::new();
    walk(root, Path::new(""), &mut output)?;
    Ok(output)
}
fn encode_chain(mut records: Vec<InventoryRecord>) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut previous = None;
    for record in &mut records {
        record.previous_record_sha256 = previous.as_deref().map(digest);
        let line = canonical(record)?;
        previous = Some(line.clone());
        bytes.extend(line);
        bytes.push(b'\n');
    }
    Ok(bytes)
}
fn parse_chain(bytes: &[u8]) -> Result<Vec<InventoryRecord>> {
    if bytes.is_empty() || !bytes.ends_with(b"\n") {
        bail!("inventory must contain canonical records with one LF each");
    }
    let mut records = Vec::new();
    let mut previous: Option<&[u8]> = None;
    for (index, with_lf) in bytes.split_inclusive(|byte| *byte == b'\n').enumerate() {
        let line = &with_lf[..with_lf.len() - 1];
        let record: InventoryRecord = serde_json::from_value(parse_json(line)?)?;
        if canonical(&record)? != line
            || record.schema_version != 1
            || record.sequence != u64::try_from(index + 1)?
            || record.previous_record_sha256 != previous.map(digest)
            || (record.kind == InventoryKind::File)
                != record.sha256.as_deref().is_some_and(lower_hex)
            || (record.kind == InventoryKind::Directory && record.sha256.is_some())
            || normalized(Path::new(&record.relative_path))? != record.relative_path
        {
            bail!("inventory record or hash chain is invalid");
        }
        records.push(record);
        previous = Some(line);
    }
    Ok(records)
}
#[cfg(unix)]
fn open_inventory_append(path: &Path) -> Result<(fs::File, Vec<u8>)> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::FileExt;
    use std::os::unix::fs::MetadataExt;
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC;
    let fd = unsafe { libc::open(c"/".as_ptr(), flags) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error()).context("open inventory root");
    }
    let mut file = unsafe { fs::File::from_raw_fd(fd) };
    let parts = path.components().collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        match part {
            std::path::Component::RootDir => {}
            std::path::Component::Normal(name) => {
                let name = CString::new(name.as_bytes())?;
                let final_part = index + 1 == parts.len();
                let flags = if final_part {
                    libc::O_RDWR | libc::O_APPEND
                } else {
                    libc::O_RDONLY | libc::O_DIRECTORY
                };
                let flags = flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK;
                let fd = unsafe { libc::openat(file.as_raw_fd(), name.as_ptr(), flags) };
                if fd < 0 {
                    return Err(std::io::Error::last_os_error())
                        .context("open anchored inventory component");
                }
                file = unsafe { fs::File::from_raw_fd(fd) };
            }
            _ => bail!("inventory path is not a normalized absolute path"),
        }
    }
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.mode() & 0o077 != 0 {
        bail!("inventory is not an owner-only single-link file");
    }
    let mut bytes = vec![0; usize::try_from(metadata.len())?];
    file.read_exact_at(&mut bytes, 0)?;
    Ok((file, bytes))
}
#[cfg(windows)]
mod windows_append {
    use super::*;
    use crate::runner::private_existing_windows as existing;
    use windows_sys::Win32::Storage::FileSystem::*;
    pub(super) fn open_inventory_append(path: &Path) -> Result<(fs::File, Vec<u8>)> {
        let bytes = read_single_link_regular(path)?;
        if crate::runner::read_private_existing_no_follow(path)? != bytes {
            bail!("inventory retained read disagrees with secure read");
        }
        let parent_path = path.parent().context("inventory path has no parent")?;
        fsync_directory(parent_path)?;
        let (_parent, before) =
            existing::open(parent_path, true, FILE_READ_ATTRIBUTES | READ_CONTROL)?;
        let (file, _) = existing::open(
            path,
            false,
            FILE_GENERIC_READ | FILE_APPEND_DATA | READ_CONTROL,
        )?;
        let mut reader = file.try_clone()?;
        let mut retained = Vec::new();
        reader.read_to_end(&mut retained)?;
        let (_parent_after, after) =
            existing::open(parent_path, true, FILE_READ_ATTRIBUTES | READ_CONTROL)?;
        if existing::identity(&before) != existing::identity(&after) {
            bail!("inventory parent identity changed during append open");
        }
        if retained != bytes {
            bail!("inventory changed during retained append open");
        }
        Ok((file, bytes))
    }
}
#[cfg(windows)]
use windows_append::open_inventory_append;
fn append_verified(file: &mut fs::File, line: &[u8], old_bytes: &[u8]) -> Result<()> {
    let old_len = old_bytes.len();
    file.seek(SeekFrom::Start(0))?;
    let mut retained_bytes = Vec::new();
    file.read_to_end(&mut retained_bytes)?;
    if retained_bytes != old_bytes {
        bail!("inventory retained handle changed before append");
    }
    file.seek(SeekFrom::End(0))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let before = file.metadata()?;
        if before.nlink() != 1 || before.len() != u64::try_from(old_len)? {
            bail!("inventory identity changed before append");
        }
        file.write_all(line)?;
        file.sync_all()?;
        let after = file.metadata()?;
        if before.dev() != after.dev()
            || before.ino() != after.ino()
            || after.nlink() != 1
            || after.mode() & 0o077 != 0
            || after.len() != u64::try_from(old_len + line.len())?
        {
            bail!("inventory identity changed during append");
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use crate::runner::private_existing_windows as existing;
        let before = existing::info(file, false)?;
        if file.metadata()?.len() != u64::try_from(old_len)? {
            bail!("inventory changed before append");
        }
        file.write_all(line)?;
        file.sync_all()?;
        let after = existing::info(file, false)?;
        if existing::identity(&before) != existing::identity(&after)
            || file.metadata()?.len() != u64::try_from(old_len + line.len())?
        {
            bail!("inventory changed during append");
        }
        Ok(())
    }
}
fn canonical(value: &impl Serialize) -> Result<Vec<u8>> {
    Ok(canonicalize_value(&serde_json::to_value(value)?)?)
}
fn canonical_root(root: &Path) -> Result<&str> {
    if !root.is_absolute() || root.canonicalize()?.as_os_str() != root.as_os_str() {
        bail!("private root must be canonical absolute path bytes");
    }
    fsync_directory(root)?;
    root.to_str().context("private root is not UTF-8")
}
fn normalized(path: &Path) -> Result<String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        bail!("inventory path is not normalized and relative");
    }
    Ok(path
        .to_str()
        .context("inventory path is not UTF-8")?
        .replace(std::path::MAIN_SEPARATOR, "/"))
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn lower_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}
