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
use crate::secure_fs_retain::RetainedBoundedFile;
use crate::secure_fs_retain::RetainedLeafPermissions;

const INPUT_CAP_BYTES: u64 = 64 * 1024;

pub(crate) struct RetainedExactPrivateInput<T> {
    path: PathBuf,
    sha256: String,
    typed: T,
    retained: RetainedBoundedFile,
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
