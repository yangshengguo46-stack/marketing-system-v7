use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use sha2::Digest;
use std::fs;
use std::path::Path;

pub(crate) fn seal_postprocess_archive(
    private_root: &Path,
    coordinator_relative: &Path,
    capture: crate::proof_archive::ArmPostprocessCapture,
) -> Result<String> {
    crate::evidence::parse_notification_archive(&capture.notifications)?;
    let payloads = vec![
        capture.notifications,
        serde_json::to_vec(&capture.start)?,
        serde_json::to_vec(&capture.config)?,
        serde_json::to_vec(&capture.pre_catalog)?,
        serde_json::to_vec(&capture.post_catalog)?,
        serde_json::to_vec(&capture.quiet_tree)?,
        serde_json::to_vec(&capture.broker_snapshot)?,
    ];
    let mut entries = Vec::with_capacity(crate::proof_archive::SIDECARS.len());
    let mut destinations = Vec::with_capacity(crate::proof_archive::SIDECARS.len() + 1);
    for (((kind, suffix), bytes), ordinal) in crate::proof_archive::SIDECARS
        .into_iter()
        .zip(&payloads)
        .zip(std::iter::repeat(capture.run_ordinal))
    {
        let relative = coordinator_relative.join(format!("run-{ordinal}-{suffix}"));
        destinations.push(crate::secure_fs::resolve_private_relative(
            private_root,
            &relative,
        )?);
        entries.push(crate::PostprocessSidecarEntry {
            kind,
            relative_path: crate::proof_archive::relative_to_utf8(&relative)?,
            sha256: sha256(bytes),
        });
    }
    let index_relative = coordinator_relative.join(format!(
        "run-{}-postprocess-index.json",
        capture.run_ordinal
    ));
    destinations.push(crate::secure_fs::resolve_private_relative(
        private_root,
        &index_relative,
    )?);
    for path in &destinations {
        if fs::symlink_metadata(path).is_ok() {
            bail!("postprocess archive destination already exists");
        }
    }
    for (path, bytes) in destinations.iter().zip(&payloads) {
        crate::secure_fs::write_owner_only_new(path, bytes)?;
    }
    let index = crate::ArmPostprocessIndex {
        schema_version: 1,
        pair_id: capture.pair_id,
        run_ordinal: capture.run_ordinal,
        condition: capture.condition,
        evidence_source: capture.evidence_source,
        sidecars: entries,
    };
    let index_bytes = crate::jcs::canonicalize_value(&serde_json::to_value(&index)?)?;
    crate::secure_fs::write_owner_only_new(
        destinations.last().context("missing index destination")?,
        &index_bytes,
    )?;
    Ok(sha256(&index_bytes))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}
