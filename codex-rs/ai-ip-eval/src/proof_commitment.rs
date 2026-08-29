use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use anyhow::bail;
use hmac::Hmac;
use hmac::Mac;
use rand::TryRngCore;
use sha2::Digest;
use sha2::Sha256;

const DOMAIN: &[u8] = b"AI-IP-PROOF-V1\0";
const SHA256_HEX_LENGTH: usize = 64;

pub(crate) struct ProofCommitmentKey([u8; 32]);

impl ProofCommitmentKey {
    pub(crate) fn generate() -> anyhow::Result<Self> {
        let mut bytes = [0_u8; 32];
        rand::rngs::OsRng
            .try_fill_bytes(&mut bytes)
            .context("generate proof commitment key with OS CSPRNG")?;
        Ok(Self(bytes))
    }

    pub(crate) fn publish_owner_only_new(&self, path: &Path) -> anyhow::Result<()> {
        crate::secure_fs::write_owner_only_new(path, &self.0)
    }

    pub(crate) fn read_exact_owner_only(path: &Path) -> anyhow::Result<Self> {
        let bytes = crate::secure_fs::read_single_link_regular_bounded(path, 32)?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("proof commitment key must be exactly 32 bytes"))?;
        Ok(Self(bytes))
    }

    #[cfg(test)]
    pub(crate) fn from_test_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

pub(crate) fn proof_commitment(
    key: &ProofCommitmentKey,
    label: &str,
    value: &serde_json::Value,
) -> anyhow::Result<String> {
    let canonical_value = crate::jcs::canonicalize_value(value)?;
    let label = label.as_bytes();
    let label_length = u32::try_from(label.len()).context("proof commitment label exceeds u32")?;
    let value_length = u64::try_from(canonical_value.len())
        .context("proof commitment canonical value exceeds u64")?;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(&key.0).context("initialize proof commitment HMAC")?;
    mac.update(DOMAIN);
    mac.update(&label_length.to_be_bytes());
    mac.update(label);
    mac.update(&value_length.to_be_bytes());
    mac.update(&canonical_value);
    Ok(format!("{:x}", mac.finalize().into_bytes()))
}

pub(crate) fn derive_public_run_id(
    key: &ProofCommitmentKey,
    pair_id: &str,
) -> anyhow::Result<String> {
    validate_lowercase_sha256(pair_id, "pair ID")?;
    proof_commitment(
        key,
        "publicRunId",
        &serde_json::Value::String(pair_id.to_owned()),
    )
}

pub(crate) fn proof_merkle_root(leaves: &BTreeMap<String, String>) -> anyhow::Result<String> {
    if leaves.is_empty() {
        bail!("proof Merkle leaves must not be empty");
    }
    let mut leaf_hashes = leaves
        .iter()
        .map(|(name, value)| {
            if name == "proofRootSha256" {
                bail!("proofRootSha256 must not be a proof Merkle leaf");
            }
            validate_lowercase_sha256(value, "proof Merkle leaf value")?;
            let name_bytes = name.as_bytes();
            let name_length =
                u32::try_from(name_bytes.len()).context("proof Merkle leaf name exceeds u32")?;
            let value_bytes = value.as_bytes();
            let value_length =
                u32::try_from(value_bytes.len()).context("proof Merkle leaf value exceeds u32")?;
            let mut hasher = Sha256::new();
            hasher.update([0x00]);
            hasher.update(name_length.to_be_bytes());
            hasher.update(name_bytes);
            hasher.update(value_length.to_be_bytes());
            hasher.update(value_bytes);
            Ok((
                name_bytes,
                hasher.finalize().into_iter().collect::<Vec<_>>(),
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    leaf_hashes.sort_by(|left, right| left.0.cmp(right.0));
    let mut hashes = leaf_hashes
        .into_iter()
        .map(|(_, hash)| hash)
        .collect::<Vec<_>>();
    while hashes.len() > 1 {
        if hashes.len() % 2 == 1 {
            hashes.push(
                hashes
                    .last()
                    .cloned()
                    .context("proof Merkle level unexpectedly empty")?,
            );
        }
        hashes = hashes
            .chunks_exact(2)
            .map(|pair| {
                let mut hasher = Sha256::new();
                hasher.update([0x01]);
                hasher.update(&pair[0]);
                hasher.update(&pair[1]);
                hasher.finalize().into_iter().collect()
            })
            .collect();
    }
    Ok(lowercase_hex(
        &hashes
            .pop()
            .context("proof Merkle root unexpectedly empty")?,
    ))
}

fn validate_lowercase_sha256(value: &str, label: &str) -> anyhow::Result<()> {
    if value.len() != SHA256_HEX_LENGTH
        || !value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        bail!("{label} must be lowercase 64-hex");
    }
    Ok(())
}

fn lowercase_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
