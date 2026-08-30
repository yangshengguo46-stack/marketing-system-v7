use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct CostBindingV1 {
    pub(crate) execution_manifest_sha256: String,
    pub(crate) cost_receipt_sha256: String,
    pub(crate) broker_receipt_sha256: String,
    pub(crate) condition: crate::EvaluationCondition,
    pub(crate) bound_at: String,
}

pub(crate) fn commit_cost_binding(
    authority: &crate::cost_authority::VerifiedLiveCostAuthority,
    condition: crate::EvaluationCondition,
    clock: &dyn crate::cost_authority::CostClock,
) -> Result<CostBindingV1> {
    prepare_binding(authority, condition, clock).map(|prepared| prepared.binding)
}

pub(crate) fn publish_cost_binding(
    authority: crate::cost_authority::VerifiedLiveCostAuthority,
    condition: crate::EvaluationCondition,
    clock: &dyn crate::cost_authority::CostClock,
) -> Result<()> {
    const BINDING_CAP: u64 = 16 * 1024;

    let root = Path::new(authority.transaction_binding().2).to_path_buf();
    let output = root.join(binding_relative_path(condition));
    match std::fs::symlink_metadata(&output) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Ok(_) => bail!("cost binding destination already exists"),
        Err(error) => return Err(error).context("inspect fixed cost binding destination"),
    }
    let prepared = prepare_binding(&authority, condition, clock)?;
    prepared.reverify(&authority)?;
    let manifests_before = manifest_snapshots(&root)?;
    prepared.reverify(&authority)?;
    let old_root = crate::private_inventory::verify_private_inventory_state(&root)?
        .inventory_root_sha256()
        .to_string();
    let created = crate::secure_fs::create_owner_only_file_new_retained(&output, &prepared.bytes)?;
    let retained = crate::secure_fs_retain::RetainedBoundedFile::retain_created_with(
        &output,
        created,
        BINDING_CAP,
        crate::secure_fs_retain::RetainedLeafPermissions::RequireOwnerOnly,
        || prepared.reverify(&authority),
    )?;
    if retained.raw_bytes() != prepared.bytes
        || serde_json::from_slice::<CostBindingV1>(retained.raw_bytes())? != prepared.binding
    {
        bail!("retained cost binding differs from prospective exact binding");
    }
    crate::secure_fs::fsync_directory(output.parent().context("cost binding parent is absent")?)?;
    retained.reverify_unchanged()?;
    prepared.reverify(&authority)?;
    let new_root = crate::private_inventory::batch::append_private_inventory_batch_from_root(
        &root,
        &old_root,
        &[crate::private_inventory::batch::ExpectedInventoryEntry {
            relative_path: binding_relative_path(condition).to_string(),
            kind: crate::private_inventory::InventoryKind::File,
            sha256: Some(sha256(retained.raw_bytes())),
        }],
    )?;
    let fresh = crate::private_inventory::verify_private_inventory_state(&root)?;
    if fresh.inventory_root_sha256() != new_root {
        bail!("fresh inventory root differs from cost binding append result");
    }
    let rebuilt = authority.rebuild(fresh)?;
    retained.reverify_unchanged()?;
    prepared.reverify(&rebuilt)?;
    if manifest_snapshots(&root)? != manifests_before {
        bail!("run manifest bytes changed during cost binding transaction");
    }
    Ok(())
}

struct PreparedBinding {
    binding: CostBindingV1,
    bytes: Vec<u8>,
    receipt: crate::secure_fs_retain::RetainedBoundedFile,
    expected_receipt: crate::cost_contracts::CostReceiptV1,
    supplier: Option<crate::cost_authority::RetainedSupplierStatement>,
}

impl PreparedBinding {
    fn reverify(&self, authority: &crate::cost_authority::VerifiedLiveCostAuthority) -> Result<()> {
        self.receipt.reverify_unchanged()?;
        authority.reverify()?;
        crate::cost_authority::revalidate_cost_receipt(
            authority,
            self.supplier.as_ref(),
            &self.expected_receipt,
        )
    }
}

fn prepare_binding(
    authority: &crate::cost_authority::VerifiedLiveCostAuthority,
    condition: crate::EvaluationCondition,
    clock: &dyn crate::cost_authority::CostClock,
) -> Result<PreparedBinding> {
    const RECEIPT_CAP: u64 = 128 * 1024;
    const BINDING_CAP: u64 = 16 * 1024;

    let root = Path::new(authority.transaction_binding().2);
    let receipt = crate::secure_fs_retain::RetainedBoundedFile::retain(
        &root.join(receipt_relative_path(condition)),
        RECEIPT_CAP,
        crate::secure_fs_retain::RetainedLeafPermissions::RequireOwnerOnly,
    )?;
    let expected_receipt = crate::cost_contracts::validate_cost_receipt(receipt.raw_bytes())?;
    if crate::jcs::canonicalize_value(&crate::jcs::parse_json(receipt.raw_bytes())?)?
        != receipt.raw_bytes()
    {
        bail!("retained cost receipt is not exact JCS");
    }
    if expected_receipt.condition != condition {
        bail!("retained cost receipt condition differs from binding condition");
    }
    let manifest_index = usize::from(
        expected_receipt
            .run_ordinal
            .checked_sub(1)
            .context("cost receipt has invalid run ordinal")?,
    );
    let manifests = manifest_snapshots(root)?;
    let selected_manifest = manifests
        .get(manifest_index)
        .context("cost receipt run ordinal is outside the sealed pair")?;
    if sha256(selected_manifest) != expected_receipt.execution_manifest_sha256 {
        bail!("sealed execution manifest SHA-256 differs from retained cost receipt commitment");
    }
    let supplier = retain_selected_supplier(root, condition, &expected_receipt)?;
    authority.reverify()?;
    crate::cost_authority::revalidate_cost_receipt(authority, supplier.as_ref(), &expected_receipt)?;
    let prospective = crate::cost_authority::commit_cost_receipt(
        authority,
        condition,
        supplier.as_ref(),
        clock,
    )?;
    if parse_millis(&prospective.calculated_at)? < parse_millis(&expected_receipt.calculated_at)? {
        bail!("cost binding time is before retained receipt calculation");
    }
    let binding = CostBindingV1 {
        execution_manifest_sha256: expected_receipt.execution_manifest_sha256.clone(),
        cost_receipt_sha256: sha256(receipt.raw_bytes()),
        broker_receipt_sha256: expected_receipt.broker_receipt_sha256.clone(),
        condition,
        bound_at: prospective.calculated_at,
    };
    let bytes = crate::jcs::canonicalize_value(&serde_json::to_value(&binding)?)?;
    if bytes.len() > BINDING_CAP as usize {
        bail!("canonical cost binding exceeds private binding cap");
    }
    if serde_json::from_slice::<CostBindingV1>(&bytes)? != binding {
        bail!("canonical cost binding changed its typed value");
    }
    Ok(PreparedBinding { binding, bytes, receipt, expected_receipt, supplier })
}

fn retain_selected_supplier(
    root: &Path,
    condition: crate::EvaluationCondition,
    receipt: &crate::cost_contracts::CostReceiptV1,
) -> Result<Option<crate::cost_authority::RetainedSupplierStatement>> {
    let Some(expected_sha256) = receipt.supplier_statement_sha256.as_deref() else {
        return Ok(None);
    };
    let retained = crate::secure_fs_retain::RetainedBoundedFile::retain(
        &root.join(crate::cost_inputs::supplier_statement_relative_path(condition)),
        64 * 1024,
        crate::secure_fs_retain::RetainedLeafPermissions::RequireOwnerOnly,
    )?;
    if sha256(retained.raw_bytes()) != expected_sha256 {
        bail!("retained supplier SHA-256 differs from cost receipt commitment");
    }
    let selected = crate::cost_authority::retain_supplier_statement(retained.raw_bytes())?;
    retained.reverify_unchanged()?;
    Ok(Some(selected))
}

fn receipt_relative_path(condition: crate::EvaluationCondition) -> &'static str {
    match condition {
        crate::EvaluationCondition::Generic => "coordinator/cost/generic-receipt.json",
        crate::EvaluationCondition::Candidate => "coordinator/cost/candidate-receipt.json",
    }
}

fn binding_relative_path(condition: crate::EvaluationCondition) -> &'static str {
    match condition {
        crate::EvaluationCondition::Generic => "coordinator/cost/generic-binding.json",
        crate::EvaluationCondition::Candidate => "coordinator/cost/candidate-binding.json",
    }
}

fn manifest_snapshots(root: &Path) -> Result<[Vec<u8>; 2]> {
    Ok([
        crate::secure_fs::read_single_link_regular_bounded(
            &root.join("coordinator/run-1-manifest.json"),
            128 * 1024,
        )
        .context("read fixed sealed manifest run 1")?,
        crate::secure_fs::read_single_link_regular_bounded(
            &root.join("coordinator/run-2-manifest.json"),
            128 * 1024,
        )
        .context("read fixed sealed manifest run 2")?,
    ])
}

fn parse_millis(value: &str) -> Result<chrono::DateTime<chrono::Utc>> {
    let parsed = chrono::DateTime::parse_from_rfc3339(value)?;
    if parsed.to_rfc3339_opts(chrono::SecondsFormat::Millis, true) != value {
        bail!("cost binding timestamp is not exact UTC milliseconds");
    }
    Ok(parsed.with_timezone(&chrono::Utc))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
