use super::*;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ExpectedInventoryEntry {
    pub(crate) relative_path: String,
    pub(crate) kind: InventoryKind,
    pub(crate) sha256: Option<String>,
}

pub(crate) struct RetainedPendingReview {
    canonical_private_root: PathBuf,
    relative_path: String,
    retained: crate::secure_fs_retain::RetainedBoundedFile,
}

pub(crate) struct VerifiedPendingReviewInventory {
    inventory: VerifiedPrivateInventory,
    pending_reviews: [RetainedPendingReview; 3],
}

impl VerifiedPendingReviewInventory {
    pub(crate) fn inventory_root_sha256(&self) -> &str {
        self.inventory.inventory_root_sha256()
    }

    pub(crate) fn reviews(&self) -> &[RetainedPendingReview; 3] {
        &self.pending_reviews
    }

    pub(crate) fn verify_binding(
        &self,
        pair_id: &str,
        frozen_context_sha256: &str,
        private_root: &Path,
    ) -> Result<()> {
        self.inventory
            .verify_binding(pair_id, frozen_context_sha256, private_root)
    }

    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        for review in &self.pending_reviews {
            review.reverify_unchanged()?;
        }
        self.inventory.reverify_unchanged()?;
        for review in &self.pending_reviews {
            review.reverify_unchanged()?;
        }
        Ok(())
    }

    pub(crate) fn into_parts(self) -> (VerifiedPrivateInventory, [RetainedPendingReview; 3]) {
        (self.inventory, self.pending_reviews)
    }
}

impl RetainedPendingReview {
    pub(crate) fn retain(root: &Path, reviewer_id: &str) -> Result<Self> {
        const REVIEW_CAP: u64 = 64 * 1024;
        canonical_root(root)?;
        let mut characters = reviewer_id.chars();
        if !(3..=64).contains(&reviewer_id.len())
            || !characters
                .next()
                .is_some_and(|value| value.is_ascii_alphanumeric())
            || characters
                .any(|value| !value.is_ascii_alphanumeric() && !matches!(value, '.' | '_' | '-'))
        {
            bail!("score continuation reviewer ID is not one safe path component");
        }
        let relative_path = format!("reviews/{reviewer_id}.json");
        validate_private_relative_path(Path::new(&relative_path))?;
        let path = root.join(&relative_path);
        let retained = crate::secure_fs_retain::RetainedBoundedFile::retain(
            &path,
            REVIEW_CAP,
            crate::secure_fs_retain::RetainedLeafPermissions::AllowNonOwnerOnlyInsidePrivateDirectory,
        )?;
        Ok(Self {
            canonical_private_root: root.to_path_buf(),
            relative_path,
            retained,
        })
    }

    pub(crate) fn raw_bytes(&self) -> &[u8] {
        self.retained.raw_bytes()
    }

    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        self.retained.reverify_unchanged()
    }

    fn inventory_entry(&self) -> ExpectedInventoryEntry {
        ExpectedInventoryEntry {
            relative_path: self.relative_path.clone(),
            kind: InventoryKind::File,
            sha256: Some(digest(self.raw_bytes())),
        }
    }
}

pub(crate) fn verify_private_inventory_continuation(
    root: &Path,
    pending_reviews: [RetainedPendingReview; 3],
    receipt_sha256: &str,
    receipt_prefix_sha256: &str,
) -> Result<VerifiedPendingReviewInventory> {
    const RECEIPT: &str = "coordinator/blind-pack-receipt.json";
    canonical_root(root)?;
    if pending_reviews
        .iter()
        .any(|review| review.canonical_private_root != root)
    {
        bail!("pending review belongs to a different private root");
    }
    for review in &pending_reviews {
        review.reverify_unchanged()?;
    }
    let mut expected_reviews = pending_reviews
        .iter()
        .map(RetainedPendingReview::inventory_entry)
        .collect::<Vec<_>>();
    expected_reviews.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let allowed_unrecorded = validate_expected_tree(&expected_reviews)?;
    let (records, inventory_bytes, pair_marker) = verified_state(
        root,
        Some(&allowed_unrecorded),
        /* supplied_bytes */ None,
        /* trust_allowed_projection */ true,
    )?;
    let tail = records
        .last()
        .context("private inventory has no blind receipt tail")?;
    let tail_bytes = canonical(tail)?;
    let prefix_len = inventory_bytes
        .len()
        .checked_sub(tail_bytes.len() + 1)
        .context("private inventory blind receipt tail bounds")?;
    if tail.relative_path != RECEIPT
        || tail.kind != InventoryKind::File
        || tail.sha256.as_deref() != Some(receipt_sha256)
        || digest(&inventory_bytes[..prefix_len]) != receipt_prefix_sha256
    {
        bail!("blind receipt is not the current private inventory tail");
    }
    for review in &pending_reviews {
        review.reverify_unchanged()?;
    }
    let inventory = VerifiedPrivateInventory {
        canonical_private_root: root.to_path_buf(),
        inventory_root_sha256: digest(&inventory_bytes),
        inventory_bytes,
        pair_marker,
        allowed_unrecorded,
        trust_allowed_projection: true,
    };
    Ok(VerifiedPendingReviewInventory {
        inventory,
        pending_reviews,
    })
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
    let (records, _, _) = verified_state(
        root,
        Some(&expected_tree),
        Some(old_bytes.clone()),
        /* trust_allowed_projection */ false,
    )?;
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
    let tree = collect_tree(
        root, /* allowed_unrecorded */ None, /* trusted_recorded */ None,
    )?;
    let entry = tree
        .get(&relative)
        .context("inventory append target is absent")?;
    let allowed_new = BTreeMap::from([(relative.clone(), entry.clone())]);
    let inventory_path = resolve_private_relative(root, Path::new(INVENTORY))?;
    let (mut inventory_file, old_bytes) = open_inventory_append(&inventory_path)?;
    if expected_old_root.is_some_and(|expected| digest(&old_bytes) != expected) {
        bail!("private inventory cursor changed before append");
    }
    let (records, _, _) = verified_state(
        root,
        Some(&allowed_new),
        Some(old_bytes.clone()),
        /* trust_allowed_projection */ false,
    )?;
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
