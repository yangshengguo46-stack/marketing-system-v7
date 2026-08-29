use super::*;

pub(super) type TreeEntry = (InventoryKind, Option<String>);

pub(super) fn collect_tree(
    root: &Path,
    allowed_unrecorded: Option<&BTreeMap<String, TreeEntry>>,
    trusted_recorded: Option<&BTreeMap<String, TreeEntry>>,
) -> Result<BTreeMap<String, TreeEntry>> {
    fn walk(
        root: &Path,
        relative: &Path,
        allowed_unrecorded: Option<&BTreeMap<String, TreeEntry>>,
        trusted_recorded: Option<&BTreeMap<String, TreeEntry>>,
        output: &mut BTreeMap<String, TreeEntry>,
    ) -> Result<()> {
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
            let allowed_entry = trusted_recorded
                .is_some()
                .then(|| allowed_unrecorded.and_then(|set| set.get(&normalized)))
                .flatten();
            let recorded_entry = trusted_recorded.and_then(|set| set.get(&normalized));
            if trusted_recorded.is_some() && allowed_entry.is_none() && recorded_entry.is_none() {
                bail!("trusted inventory continuation contains an unexpected path");
            }
            let path = root.join(&child_relative);
            let metadata = fs::symlink_metadata(&path)?;
            let entry = if metadata.is_dir() {
                if allowed_entry.is_some()
                    || recorded_entry.is_some_and(|entry| entry.0 != InventoryKind::Directory)
                {
                    bail!("trusted inventory continuation entry kind changed");
                }
                #[cfg(windows)]
                crate::runner::validate_private_existing_directory_no_follow(&path)?;
                (InventoryKind::Directory, None)
            } else if metadata.is_file() {
                if let Some(expected) = allowed_entry {
                    if expected.0 != InventoryKind::File {
                        bail!("allowed private tree entry is not a regular file");
                    }
                    expected.clone()
                } else {
                    if recorded_entry.is_some_and(|entry| entry.0 != InventoryKind::File) {
                        bail!("trusted inventory continuation entry kind changed");
                    }
                    let bytes = crate::runner::read_private_existing_no_follow(&path)?;
                    (InventoryKind::File, Some(digest(&bytes)))
                }
            } else {
                bail!("private tree contains a link or special entry");
            };
            output.insert(normalized, entry);
            if metadata.is_dir() {
                walk(
                    root,
                    &child_relative,
                    allowed_unrecorded,
                    trusted_recorded,
                    output,
                )?;
            }
        }
        Ok(())
    }
    let mut output = BTreeMap::new();
    walk(
        root,
        Path::new(""),
        allowed_unrecorded,
        trusted_recorded,
        &mut output,
    )?;
    Ok(output)
}
