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
use crate::private_inventory::batch::ExpectedInventoryEntry;
use crate::secure_fs_retain::RetainedBoundedFile;
use crate::secure_fs_retain::RetainedLeafPermissions;

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
        })?;
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
