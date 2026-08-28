use super::*;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ExpectedInventoryEntry {
    pub(crate) relative_path: String,
    pub(crate) kind: InventoryKind,
    pub(crate) sha256: Option<String>,
}

pub(crate) fn append_private_inventory_batch(
    root: &Path,
    expected_new: &[ExpectedInventoryEntry],
) -> Result<String> {
    append_private_inventory_batch_inner(root, None, expected_new)
}

pub(crate) fn append_private_inventory_batch_from_root(
    root: &Path,
    expected_old_root: &str,
    expected_new: &[ExpectedInventoryEntry],
) -> Result<String> {
    append_private_inventory_batch_inner(root, Some(expected_old_root), expected_new)
}

fn append_private_inventory_batch_inner(
    root: &Path,
    expected_old_root: Option<&str>,
    expected_new: &[ExpectedInventoryEntry],
) -> Result<String> {
    let expected_tree = validate_expected_tree(expected_new)?;
    let inventory_path = resolve_private_relative(root, Path::new(INVENTORY))?;
    let (mut inventory_file, old_bytes) = open_inventory_append(&inventory_path)?;
    if expected_old_root.is_some_and(|expected| digest(&old_bytes) != expected) {
        bail!("private inventory cursor changed before batch append");
    }
    let (records, _, _) = verified_state(root, Some(&expected_tree), Some(old_bytes.clone()))?;
    let mut previous = records.last().map(canonical).transpose()?;
    let base_sequence = u64::try_from(records.len())?;
    let mut suffix = Vec::new();
    for (offset, entry) in expected_new.iter().enumerate() {
        let increment = u64::try_from(offset)?
            .checked_add(1)
            .context("inventory batch sequence increment overflow")?;
        let record = InventoryRecord {
            schema_version: 1,
            sequence: base_sequence
                .checked_add(increment)
                .context("inventory batch sequence overflow")?,
            relative_path: entry.relative_path.clone(),
            kind: entry.kind,
            sha256: entry.sha256.clone(),
            previous_record_sha256: previous.as_deref().map(digest),
        };
        let line = canonical(&record)?;
        suffix.extend(&line);
        suffix.push(b'\n');
        previous = Some(line);
    }
    append_verified(&mut inventory_file, &suffix, &old_bytes)?;
    fsync_directory(&resolve_private_relative(root, Path::new("coordinator"))?)?;
    drop(inventory_file);
    let mut complete = old_bytes;
    complete.extend(suffix);
    let expected_root = digest(&complete);
    let actual_root = verify_private_inventory(root)?;
    if actual_root != expected_root {
        bail!("inventory root changed after batch append");
    }
    Ok(actual_root)
}

pub(super) fn append_private_inventory_unchecked(root: &Path, relative: &Path) -> Result<String> {
    append_private_inventory_inner(root, None, relative)
}

pub(crate) fn append_private_inventory_from_root(
    root: &Path,
    expected_old_root: &str,
    relative: &Path,
) -> Result<String> {
    append_private_inventory_inner(root, Some(expected_old_root), relative)
}

fn append_private_inventory_inner(
    root: &Path,
    expected_old_root: Option<&str>,
    relative: &Path,
) -> Result<String> {
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
    if expected_old_root.is_some_and(|expected| digest(&old_bytes) != expected) {
        bail!("private inventory cursor changed before append");
    }
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

fn validate_expected_tree(
    expected_new: &[ExpectedInventoryEntry],
) -> Result<BTreeMap<String, TreeEntry>> {
    if expected_new.is_empty() {
        bail!("inventory batch must not be empty");
    }
    let mut tree = BTreeMap::new();
    let mut previous: Option<&str> = None;
    for entry in expected_new {
        let path = normalized(Path::new(&entry.relative_path))?;
        if path != entry.relative_path
            || entry.relative_path == INVENTORY
            || previous.is_some_and(|previous| previous >= entry.relative_path.as_str())
            || (entry.kind == InventoryKind::File) != entry.sha256.as_deref().is_some_and(lower_hex)
            || (entry.kind == InventoryKind::Directory && entry.sha256.is_some())
        {
            bail!("inventory batch entries are not exact sorted tree records");
        }
        tree.insert(
            entry.relative_path.clone(),
            (entry.kind, entry.sha256.clone()),
        );
        previous = Some(&entry.relative_path);
    }
    Ok(tree)
}
