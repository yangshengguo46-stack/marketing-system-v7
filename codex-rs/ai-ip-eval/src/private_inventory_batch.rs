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
    let expected_tree = validate_expected_tree(expected_new)?;
    let inventory_path = resolve_private_relative(root, Path::new(INVENTORY))?;
    let (mut inventory_file, old_bytes) = open_inventory_append(&inventory_path)?;
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
