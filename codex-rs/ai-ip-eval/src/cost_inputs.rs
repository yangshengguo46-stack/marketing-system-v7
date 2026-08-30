use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::Digest;
use sha2::Sha256;

use crate::NativeHeldOutAttestation;
use crate::cost_contracts::BillingPolicyV1;
use crate::cost_contracts::FrozenCostContracts;
use crate::cost_contracts::FxPolicyV1;
use crate::cost_contracts::ProviderBudgetEvidenceV1;
use crate::cost_contracts::ProviderRateCardV1;
use crate::cost_contracts::SupplierStatementV1;
use crate::private_inventory::InventoryKind;
use crate::private_inventory::VerifiedPrivateInventory;
use crate::private_inventory::batch::ExpectedInventoryEntry;
use crate::secure_fs_retain::RetainedBoundedFile;
use crate::secure_fs_retain::RetainedLeafPermissions;
use crate::secure_fs_retain::RetainedPrivateRoot;

const INPUT_CAP_BYTES: u64 = 64 * 1024;

pub(crate) struct RetainedExactPrivateInput<T> {
    path: PathBuf,
    sha256: String,
    typed: T,
    retained: RetainedBoundedFile,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SupplierInventoryState {
    Pending,
    Covered,
}

pub(crate) struct RetainedPendingSupplierStatement {
    canonical_private_root: PathBuf,
    relative_path: String,
    retained: RetainedExactPrivateInput<SupplierStatementV1>,
    state: SupplierInventoryState,
}

impl RetainedPendingSupplierStatement {
    pub(crate) fn retain(
        private_root: &Path,
        condition: crate::EvaluationCondition,
        supplied: &Path,
    ) -> Result<Self> {
        let canonical_private_root = private_root
            .canonicalize()
            .context("canonicalize pending supplier private root")?;
        if !private_root.is_absolute() || canonical_private_root != private_root {
            bail!("pending supplier private root is not exact canonical bytes");
        }
        let relative_path = supplier_statement_relative_path(condition).to_string();
        let expected = private_root.join(&relative_path);
        if supplied != expected {
            bail!("pending supplier path is not the exact condition-bound supplier leaf");
        }
        let contracts = FrozenCostContracts::load()?;
        let retained = RetainedExactPrivateInput::retain(supplied, &expected, |bytes| {
            contracts.validate_supplier_statement(bytes)
        }).context("retain pending supplier statement")?;
        if retained.typed().condition != condition {
            bail!("pending supplier typed condition differs from its condition-bound supplier leaf");
        }
        retained.reverify_unchanged()?;
        Ok(Self {
            canonical_private_root,
            relative_path,
            retained,
            state: SupplierInventoryState::Pending,
        })
    }

    pub(crate) fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub(crate) fn raw_bytes(&self) -> &[u8] {
        self.retained.raw_bytes()
    }

    pub(crate) fn sha256(&self) -> &str {
        self.retained.sha256()
    }

    pub(crate) fn typed(&self) -> &SupplierStatementV1 {
        self.retained.typed()
    }

    pub(crate) fn inventory_entry(&self) -> ExpectedInventoryEntry {
        ExpectedInventoryEntry {
            relative_path: self.relative_path.clone(),
            kind: InventoryKind::File,
            sha256: Some(self.sha256().to_string()),
        }
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.state == SupplierInventoryState::Pending
    }

    pub(crate) fn into_covered(mut self) -> Self {
        self.state = SupplierInventoryState::Covered;
        self
    }

    pub(crate) fn private_root(&self) -> &Path {
        &self.canonical_private_root
    }

    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        self.retained.reverify_unchanged()
    }
}

pub(crate) fn supplier_statement_relative_path(
    condition: crate::EvaluationCondition,
) -> &'static str {
    match condition {
        crate::EvaluationCondition::Generic => "inputs/supplier-statements/generic.json",
        crate::EvaluationCondition::Candidate => "inputs/supplier-statements/candidate.json",
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum CostReceiptCheckpoint {
    AfterSupplierRetainBeforePairVerify,
    BeforeSupplierInventoryAppend,
    AfterSupplierInventoryAppend,
    BeforeReceiptCreate,
    AfterReceiptCreateBeforeInventoryAppend,
    BeforeReceiptInventoryAppend,
    AfterReceiptInventoryAppend,
}

pub(crate) struct PreparedCostReceiptContext {
    pub(crate) condition: crate::EvaluationCondition,
    pub(crate) old_inventory_root: String,
    pub(crate) supplier: Option<RetainedPendingSupplierStatement>,
    pub(crate) absence_parent: Option<RetainedPrivateRoot>,
}

pub(crate) fn publish_cost_receipt(
    authority: crate::cost_authority::VerifiedLiveCostAuthority,
    condition: crate::EvaluationCondition,
    supplier_path: Option<&Path>,
    clock: &dyn crate::cost_authority::CostClock,
) -> Result<()> {
    publish_cost_receipt_with_hook(
        authority, condition, supplier_path, clock, &mut |_| Ok(()),
    )
}

#[cfg(test)]
pub(crate) fn publish_cost_receipt_for_test(
    authority: crate::cost_authority::VerifiedLiveCostAuthority,
    condition: crate::EvaluationCondition,
    supplier_path: Option<&Path>,
    clock: &dyn crate::cost_authority::CostClock,
    hook: &mut dyn FnMut(CostReceiptCheckpoint) -> Result<()>,
) -> Result<()> {
    publish_cost_receipt_with_hook(authority, condition, supplier_path, clock, hook)
}

fn publish_cost_receipt_with_hook(
    mut authority: crate::cost_authority::VerifiedLiveCostAuthority,
    condition: crate::EvaluationCondition,
    supplier_path: Option<&Path>,
    clock: &dyn crate::cost_authority::CostClock,
    hook: &mut dyn FnMut(CostReceiptCheckpoint) -> Result<()>,
) -> Result<()> {
    let (_, _, root) = authority.transaction_binding();
    let root = PathBuf::from(root);
    crate::preflight_cost_receipt_absent(&root, condition)?;
    if authority.transaction.is_none() {
        let (inventory, transaction) =
            prepare_transaction_context(&root, condition, supplier_path, hook)?;
        authority = authority.rebuild(inventory)?;
        authority.transaction = Some(transaction);
    }
    let transaction = authority.transaction.take()
        .context("prepared cost receipt transaction context is absent")?;
    crate::verify_prepared_cost_receipt_arguments(
        &root, condition, supplier_path, &transaction,
    )?;
    publish_prepared_receipt(authority, transaction, clock, hook)
}

fn publish_prepared_receipt(
    mut authority: crate::cost_authority::VerifiedLiveCostAuthority,
    mut transaction: PreparedCostReceiptContext,
    clock: &dyn crate::cost_authority::CostClock,
    hook: &mut dyn FnMut(CostReceiptCheckpoint) -> Result<()>,
) -> Result<()> {
    let root = PathBuf::from(authority.transaction_binding().2);
    let supplier = transaction.supplier.as_ref()
        .map(|value| crate::cost_authority::retain_supplier_statement(value.raw_bytes()))
        .transpose()?;
    verify_sources(&authority, &transaction)?;
    let expected = crate::cost_authority::commit_cost_receipt(
        &authority, transaction.condition, supplier.as_ref(), clock,
    )?;
    let receipt_bytes = crate::canonical_cost_receipt_bytes(&expected)?;

    if transaction.supplier.as_ref().is_some_and(|value| value.is_pending()) {
        verify_sources(&authority, &transaction)?;
        hook(CostReceiptCheckpoint::BeforeSupplierInventoryAppend)?;
        verify_sources(&authority, &transaction)?;
        let statement = transaction.supplier.as_ref().context("pending supplier is absent")?;
        let new_root = crate::private_inventory::batch::append_private_inventory_batch_from_root(
            &root, &transaction.old_inventory_root, &[statement.inventory_entry()],
        )?;
        let fresh = fresh_inventory(&authority, &root, &new_root)?;
        hook(CostReceiptCheckpoint::AfterSupplierInventoryAppend)?;
        authority = authority.rebuild(fresh)?;
        transaction.old_inventory_root = new_root;
        transaction.supplier = transaction.supplier.take().map(|value| value.into_covered());
        verify_sources(&authority, &transaction)?;
        crate::cost_authority::revalidate_cost_receipt(&authority, supplier.as_ref(), &expected)?;
    }

    verify_sources(&authority, &transaction)?;
    hook(CostReceiptCheckpoint::BeforeReceiptCreate)?;
    verify_sources(&authority, &transaction)?;
    let relative = crate::cost_receipt_relative_path(transaction.condition);
    let receipt_path = root.join(relative);
    let created = crate::secure_fs::create_owner_only_file_new_retained(
        &receipt_path, &receipt_bytes,
    )?;
    let retained = RetainedBoundedFile::retain_created_with(
        &receipt_path,
        created,
        128 * 1024,
        RetainedLeafPermissions::RequireOwnerOnly,
        || verify_supplier_context(&root, &transaction),
    )?;
    if retained.raw_bytes() != receipt_bytes
        || crate::cost_contracts::validate_cost_receipt(retained.raw_bytes())? != expected
    {
        bail!("retained receipt differs from the prospective exact receipt");
    }
    hook(CostReceiptCheckpoint::AfterReceiptCreateBeforeInventoryAppend)?;
    retained.reverify_unchanged()?;
    verify_supplier_context(&root, &transaction)?;
    hook(CostReceiptCheckpoint::BeforeReceiptInventoryAppend)?;
    retained.reverify_unchanged()?;
    let receipt_entry = ExpectedInventoryEntry {
        relative_path: relative.to_string(),
        kind: InventoryKind::File,
        sha256: Some(format!("{:x}", Sha256::digest(retained.raw_bytes()))),
    };
    let final_root = crate::private_inventory::batch::append_private_inventory_batch_from_root(
        &root, &transaction.old_inventory_root, &[receipt_entry],
    )?;
    let fresh = fresh_inventory(&authority, &root, &final_root)?;
    hook(CostReceiptCheckpoint::AfterReceiptInventoryAppend)?;
    authority = authority.rebuild(fresh)?;
    retained.reverify_unchanged()?;
    verify_supplier_context(&root, &transaction)?;
    crate::cost_authority::revalidate_cost_receipt(&authority, supplier.as_ref(), &expected)
}

pub(crate) fn prepare_transaction_context(
    root: &Path,
    condition: crate::EvaluationCondition,
    supplied: Option<&Path>,
    hook: &mut dyn FnMut(CostReceiptCheckpoint) -> Result<()>,
) -> Result<(VerifiedPrivateInventory, PreparedCostReceiptContext)> {
    let (inventory, supplier, absence_parent) = if let Some(path) = supplied {
        let retained = RetainedPendingSupplierStatement::retain(root, condition, path)?;
        hook(CostReceiptCheckpoint::AfterSupplierRetainBeforePairVerify)?;
        retained.reverify_unchanged()?;
        match crate::private_inventory::verify_private_inventory_state(root) {
            Ok(inventory) => (inventory, Some(retained.into_covered()), None),
            Err(_) => {
                let inventory = crate::private_inventory::batch::verify_pending_supplier_inventory(
                    root, &retained.inventory_entry(),
                )?;
                (inventory, Some(retained), None)
            }
        }
    } else {
        let parent = RetainedPrivateRoot::retain(root)?;
        verify_supplier_absent(root, condition, &parent)?;
        let inventory = crate::private_inventory::verify_private_inventory_state(root)?;
        (inventory, None, Some(parent))
    };
    let old_inventory_root = inventory.inventory_root_sha256().to_string();
    Ok((inventory, PreparedCostReceiptContext {
        condition, old_inventory_root, supplier, absence_parent,
    }))
}

fn fresh_inventory(
    authority: &crate::cost_authority::VerifiedLiveCostAuthority,
    root: &Path,
    expected_root: &str,
) -> Result<VerifiedPrivateInventory> {
    let fresh = crate::private_inventory::verify_private_inventory_state(root)?;
    if fresh.inventory_root_sha256() != expected_root {
        bail!("fresh inventory root differs from expected-SHA append result");
    }
    let (pair_id, frozen_sha, _) = authority.transaction_binding();
    fresh.verify_binding(pair_id, frozen_sha, root)?;
    Ok(fresh)
}

fn verify_sources(
    authority: &crate::cost_authority::VerifiedLiveCostAuthority,
    transaction: &PreparedCostReceiptContext,
) -> Result<()> {
    authority.reverify()?;
    verify_supplier_context(Path::new(authority.transaction_binding().2), transaction)
}

fn verify_supplier_context(root: &Path, transaction: &PreparedCostReceiptContext) -> Result<()> {
    match (&transaction.supplier, &transaction.absence_parent) {
        (Some(supplier), None) => supplier.reverify_unchanged(),
        (None, Some(parent)) => verify_supplier_absent(root, transaction.condition, parent),
        _ => bail!("cost receipt supplier context is not exhaustive"),
    }
}

fn verify_supplier_absent(
    root: &Path,
    condition: crate::EvaluationCondition,
    parent: &RetainedPrivateRoot,
) -> Result<()> {
    parent.reverify_unchanged()?;
    match std::fs::symlink_metadata(root.join(supplier_statement_relative_path(condition))) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => parent.reverify_unchanged(),
        Ok(_) => bail!("omitted supplier statement exists at the selected fixed leaf"),
        Err(error) => Err(error).context("inspect omitted supplier fixed leaf"),
    }
}

impl<T> RetainedExactPrivateInput<T>
where
    T: DeserializeOwned + Serialize,
{
    pub(crate) fn retain(
        supplied: &Path,
        expected: &Path,
        validator: impl FnOnce(&[u8]) -> Result<T>,
    ) -> Result<Self> {
        if supplied != expected {
            bail!("private input path differs from its fixed retention-managed slot");
        }
        let retained = RetainedBoundedFile::retain(
            supplied,
            INPUT_CAP_BYTES,
            RetainedLeafPermissions::RequireOwnerOnly,
        )?;
        let typed = validator(retained.raw_bytes())?;
        if serde_json::to_value(&typed)? != crate::jcs::parse_json(retained.raw_bytes())? {
            bail!("retained private input failed typed deep equality");
        }
        let sha256 = format!("{:x}", Sha256::digest(retained.raw_bytes()));
        retained.reverify_unchanged()?;
        Ok(Self {
            path: supplied.to_path_buf(),
            sha256,
            typed,
            retained,
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn sha256(&self) -> &str {
        &self.sha256
    }

    pub(crate) fn raw_bytes(&self) -> &[u8] {
        self.retained.raw_bytes()
    }

    pub(crate) fn typed(&self) -> &T {
        &self.typed
    }

    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        self.retained.reverify_unchanged()
    }
}

pub(crate) struct RetainedPrivateCostInputs {
    pub(crate) attestation: RetainedExactPrivateInput<NativeHeldOutAttestation>,
    pub(crate) budget: RetainedExactPrivateInput<ProviderBudgetEvidenceV1>,
    pub(crate) rate_card: RetainedExactPrivateInput<ProviderRateCardV1>,
    pub(crate) billing_policy: RetainedExactPrivateInput<BillingPolicyV1>,
    pub(crate) fx_policy: RetainedExactPrivateInput<FxPolicyV1>,
}

impl RetainedPrivateCostInputs {
    pub(crate) fn retain(
        private_root: &Path,
        attestation: &Path,
        budget: &Path,
        rate_card: &Path,
        billing_policy: &Path,
        fx_policy: &Path,
    ) -> Result<Self> {
        let inputs = private_root.join("inputs");
        let contracts = FrozenCostContracts::load()?;
        let frozen = crate::FrozenContracts::load()?;
        let retained = Self {
            attestation: RetainedExactPrivateInput::retain(
                attestation,
                &inputs.join("held-out-attestation.json"),
                |bytes| {
                    frozen
                        .validate_native_attestation(bytes)
                        .map_err(anyhow::Error::from)
                },
            )?,
            budget: RetainedExactPrivateInput::retain(
                budget,
                &inputs.join("provider-budget-evidence.json"),
                |bytes| contracts.validate_provider_budget_evidence(bytes),
            )?,
            rate_card: RetainedExactPrivateInput::retain(
                rate_card,
                &inputs.join("rate-card.json"),
                |bytes| contracts.validate_rate_card(bytes),
            )?,
            billing_policy: RetainedExactPrivateInput::retain(
                billing_policy,
                &inputs.join("billing-policy.json"),
                |bytes| contracts.validate_billing_policy(bytes),
            )?,
            fx_policy: RetainedExactPrivateInput::retain(
                fx_policy,
                &inputs.join("fx-policy.json"),
                |bytes| contracts.validate_fx_policy(bytes),
            )?,
        };
        contracts
            .validate_inputs(
                retained.rate_card.raw_bytes(),
                retained.billing_policy.raw_bytes(),
                retained.fx_policy.raw_bytes(),
                retained.budget.raw_bytes(),
            )
            .context("validate retained cost input set")?;
        retained.reverify_unchanged()?;
        Ok(retained)
    }

    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        self.attestation.reverify_unchanged()?;
        self.budget.reverify_unchanged()?;
        self.rate_card.reverify_unchanged()?;
        self.billing_policy.reverify_unchanged()?;
        self.fx_policy.reverify_unchanged()
    }
}
