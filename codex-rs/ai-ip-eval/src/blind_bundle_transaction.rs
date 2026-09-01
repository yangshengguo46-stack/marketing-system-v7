use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use crate::ExecutionMode;
use crate::blind_bundle_model::BlindPackReceipt;
use crate::blind_bundle_model::PreparedBlindBundles;
use crate::blind_bundle_model::ReviewerMappingCommitment;
use crate::blind_finalize::BlindBundleTransactionCursor;
use crate::blind_finalize::VerifiedBlindPair;
use crate::private_inventory::InventoryKind;
use crate::private_inventory::batch::ExpectedInventoryEntry;
use crate::private_inventory::batch::append_private_inventory_batch_from_root;
use crate::secure_fs::create_owner_only_dir_new;
use crate::secure_fs::fsync_directory;
use crate::secure_fs::resolve_private_relative;
use crate::secure_fs::validate_private_relative_path;
use crate::secure_fs::write_owner_only_new;
use crate::secure_fs_publish::publish_private_tree_no_replace;

const REVIEWER_STAGING: &str = ".blind-pack-staging-reviewer";
const REVIEWER_FINAL: &str = "reviewer";
const MAPPINGS_STAGING: &str = "coordinator/.blind-pack-staging-mappings";
const MAPPINGS_FINAL: &str = "coordinator/mappings";
const SEEDS_STAGING: &str = "coordinator/.blind-pack-staging-seeds";
const SEEDS_FINAL: &str = "coordinator/blind-seeds";
const REVIEWS_STAGING: &str = ".blind-pack-staging-reviews";
const REVIEWS_FINAL: &str = "reviews";
const RECEIPT: &str = "coordinator/blind-pack-receipt.json";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum BlindBundleTree {
    Reviewer,
    Mappings,
    Seeds,
    Reviews,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum BlindBundleCheckpoint {
    BeforeFirstPublish,
    AfterTreeRenameBeforeInventoryAppend(BlindBundleTree),
    AfterReviewerPublish,
    AfterMappingsPublish,
    AfterSeedsPublish,
    AfterReviewsPublish,
    BeforeReceiptCreate,
    AfterReceiptCreateBeforeInventoryAppend,
}

pub(crate) fn commit_blind_bundles(
    pair: &VerifiedBlindPair,
    prepared: &PreparedBlindBundles,
) -> Result<()> {
    commit_blind_bundles_with(pair, prepared, &mut || Ok(Utc::now()), &mut |_| Ok(()))
}

#[cfg(test)]
pub(crate) fn commit_blind_bundles_for_test(
    pair: &VerifiedBlindPair,
    prepared: &PreparedBlindBundles,
    clock: &mut dyn FnMut() -> Result<DateTime<Utc>>,
    hook: &mut dyn FnMut(BlindBundleCheckpoint) -> Result<()>,
) -> Result<()> {
    commit_blind_bundles_with(pair, prepared, clock, hook)
}

fn commit_blind_bundles_with(
    pair: &VerifiedBlindPair,
    prepared: &PreparedBlindBundles,
    clock: &mut dyn FnMut() -> Result<DateTime<Utc>>,
    hook: &mut dyn FnMut(BlindBundleCheckpoint) -> Result<()>,
) -> Result<()> {
    crate::blind_bundle_transaction_validate::validate_prepared_binding(pair, prepared)?;
    preflight(&prepared.private_root)?;
    let transaction = pair.begin_bundle_transaction()?;
    let mut inventory_root = transaction.inventory_root_sha256().to_string();
    preflight(&prepared.private_root)?;
    transaction.reverify_root_unchanged()?;

    inventory_root = publish_and_record(
        &prepared.private_root,
        reviewer_plan(prepared)?,
        &inventory_root,
        BlindBundleTree::Reviewer,
        BlindBundleCheckpoint::AfterReviewerPublish,
        &transaction,
        hook,
    )?;
    inventory_root = publish_and_record(
        &prepared.private_root,
        mappings_plan(prepared)?,
        &inventory_root,
        BlindBundleTree::Mappings,
        BlindBundleCheckpoint::AfterMappingsPublish,
        &transaction,
        hook,
    )?;
    if prepared.mode != ExecutionMode::Replay {
        inventory_root = publish_and_record(
            &prepared.private_root,
            seeds_plan(prepared)?,
            &inventory_root,
            BlindBundleTree::Seeds,
            BlindBundleCheckpoint::AfterSeedsPublish,
            &transaction,
            hook,
        )?;
    }
    inventory_root = publish_and_record(
        &prepared.private_root,
        TreePlan::new(REVIEWS_STAGING, REVIEWS_FINAL),
        &inventory_root,
        BlindBundleTree::Reviews,
        BlindBundleCheckpoint::AfterReviewsPublish,
        &transaction,
        hook,
    )?;

    transaction.reverify_root_unchanged()?;
    hook(BlindBundleCheckpoint::BeforeReceiptCreate)?;
    transaction.reverify_root_unchanged()?;
    let generated_at = clock()?.to_rfc3339_opts(SecondsFormat::Millis, true);
    transaction.reverify_root_unchanged()?;
    let receipt = receipt(prepared, inventory_root.clone(), generated_at)?;
    let receipt_bytes = canonical(&receipt)?;
    let receipt_path = resolve_private_relative(&prepared.private_root, Path::new(RECEIPT))?;
    write_owner_only_new(&receipt_path, &receipt_bytes)?;
    fsync_directory(&resolve_private_relative(
        &prepared.private_root,
        Path::new("coordinator"),
    )?)?;
    transaction.reverify_root_unchanged()?;
    hook(BlindBundleCheckpoint::AfterReceiptCreateBeforeInventoryAppend)?;
    transaction.reverify_root_unchanged()?;
    let final_root = append_private_inventory_batch_from_root(
        &prepared.private_root,
        &inventory_root,
        &[ExpectedInventoryEntry {
            relative_path: RECEIPT.to_string(),
            kind: InventoryKind::File,
            sha256: Some(sha256(&receipt_bytes)),
        }],
    )?;
    transaction.reverify_root_unchanged()?;
    let actual_final_root =
        crate::private_inventory::verify_private_inventory(&prepared.private_root)?;
    transaction.reverify_root_unchanged()?;
    if actual_final_root != final_root {
        bail!("blind bundle final inventory root changed after receipt append");
    }
    Ok(())
}

fn preflight(root: &Path) -> Result<()> {
    for relative in [
        REVIEWER_STAGING,
        REVIEWER_FINAL,
        MAPPINGS_STAGING,
        MAPPINGS_FINAL,
        SEEDS_STAGING,
        SEEDS_FINAL,
        REVIEWS_STAGING,
        REVIEWS_FINAL,
        RECEIPT,
    ] {
        let path = resolve_private_relative(root, Path::new(relative))?;
        match std::fs::symlink_metadata(path) {
            Ok(_) => bail!("blind-pack transaction path already exists: {relative}"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("inspect blind-pack transaction path"),
        }
    }
    Ok(())
}

fn reviewer_plan(prepared: &PreparedBlindBundles) -> Result<TreePlan<'_>> {
    let mut plan = TreePlan::new(REVIEWER_STAGING, REVIEWER_FINAL);
    for reviewer in &prepared.reviewers {
        let base = reviewer.reviewer_id.clone();
        plan.dir(&base)?;
        plan.dir(&format!("{base}/materials"))?;
        plan.file(&format!("{base}/A.json"), &reviewer.a_bytes)?;
        plan.file(&format!("{base}/B.json"), &reviewer.b_bytes)?;
        plan.file(&format!("{base}/case.json"), &prepared.case_bytes)?;
        plan.file(
            &format!("{base}/materials-manifest.json"),
            &prepared.materials_manifest_bytes,
        )?;
        plan.file(&format!("{base}/rubric.json"), prepared.rubric_bytes)?;
        plan.file(
            &format!("{base}/reviewer-submission.schema.json"),
            prepared.reviewer_submission_schema_bytes,
        )?;
        plan.file(
            &format!("{base}/review-bundle.json"),
            &reviewer.review_bundle_bytes,
        )?;
        for material in &prepared.materials {
            let material_path = format!("{base}/materials/{}", material.relative_path);
            plan.parents(&material_path)?;
            plan.file(&material_path, &material.bytes)?;
        }
    }
    Ok(plan)
}

fn mappings_plan(prepared: &PreparedBlindBundles) -> Result<TreePlan<'_>> {
    let mut plan = TreePlan::new(MAPPINGS_STAGING, MAPPINGS_FINAL);
    for reviewer in &prepared.reviewers {
        plan.file(
            &format!("{}.json", reviewer.reviewer_id),
            &reviewer.mapping_bytes,
        )?;
    }
    Ok(plan)
}

fn seeds_plan(prepared: &PreparedBlindBundles) -> Result<TreePlan<'_>> {
    let mut plan = TreePlan::new(SEEDS_STAGING, SEEDS_FINAL);
    for reviewer in &prepared.reviewers {
        let seed = reviewer
            .native_seed
            .as_ref()
            .context("prepared Native reviewer seed is absent")?;
        plan.file(&format!("{}.seed", reviewer.reviewer_id), seed)?;
    }
    Ok(plan)
}

fn publish_and_record(
    root: &Path,
    plan: TreePlan<'_>,
    inventory_root: &str,
    tree: BlindBundleTree,
    after: BlindBundleCheckpoint,
    transaction: &BlindBundleTransactionCursor,
    hook: &mut dyn FnMut(BlindBundleCheckpoint) -> Result<()>,
) -> Result<String> {
    transaction.reverify_root_unchanged()?;
    plan.build(root)?;
    transaction.reverify_root_unchanged()?;
    if tree == BlindBundleTree::Reviewer {
        hook(BlindBundleCheckpoint::BeforeFirstPublish)?;
        transaction.reverify_root_unchanged()?;
    }
    publish_private_tree_no_replace(root, Path::new(plan.staging), Path::new(plan.final_path))?;
    transaction.reverify_root_unchanged()?;
    hook(BlindBundleCheckpoint::AfterTreeRenameBeforeInventoryAppend(
        tree,
    ))?;
    transaction.reverify_root_unchanged()?;
    let inventory_root =
        append_private_inventory_batch_from_root(root, inventory_root, &plan.inventory_entries())?;
    transaction.reverify_root_unchanged()?;
    hook(after)?;
    transaction.reverify_root_unchanged()?;
    Ok(inventory_root)
}

fn receipt(
    prepared: &PreparedBlindBundles,
    inventory_root_sha256: String,
    generated_at: String,
) -> Result<BlindPackReceipt> {
    let parsed = DateTime::parse_from_rfc3339(&generated_at)?;
    if parsed.to_utc().to_rfc3339_opts(SecondsFormat::Millis, true) != generated_at {
        bail!("blind bundle receipt clock is not canonical UTC milliseconds");
    }
    Ok(BlindPackReceipt {
        schema_version: 1,
        pair_id: prepared.pair_id.clone(),
        frozen_run_context_sha256: prepared.frozen_run_context_sha256.clone(),
        pair_receipt_sha256: prepared.pair_receipt_sha256.clone(),
        pair_verification_sha256: prepared.pair_verification_sha256.clone(),
        rubric_sha256: prepared.rubric_sha256.clone(),
        decision_policy_sha256: prepared.decision_policy_sha256.clone(),
        reviewer_submission_schema_sha256: prepared.reviewer_submission_schema_sha256.clone(),
        reviewer_mappings: prepared
            .reviewers
            .iter()
            .map(|reviewer| ReviewerMappingCommitment {
                reviewer_id: reviewer.reviewer_id.clone(),
                review_bundle_sha256: reviewer.review_bundle_sha256.clone(),
                mapping_sha256: reviewer.mapping_sha256.clone(),
                seed_commitment: reviewer.seed_commitment.clone(),
            })
            .collect(),
        inventory_root_sha256,
        reviews_drop_sha256: sha256(br#"{"entries":[]}"#),
        generated_at,
    })
}

struct TreePlan<'a> {
    staging: &'static str,
    final_path: &'static str,
    directories: BTreeSet<String>,
    files: BTreeMap<String, &'a [u8]>,
}

impl<'a> TreePlan<'a> {
    fn new(staging: &'static str, final_path: &'static str) -> Self {
        Self {
            staging,
            final_path,
            directories: BTreeSet::new(),
            files: BTreeMap::new(),
        }
    }

    fn dir(&mut self, relative: &str) -> Result<()> {
        validate_private_relative_path(Path::new(relative))?;
        let relative = path_wire(Path::new(relative))?;
        if self.files.contains_key(&relative) || !self.directories.insert(relative) {
            bail!("blind bundle tree plan contains a duplicate path");
        }
        Ok(())
    }

    fn parents(&mut self, relative: &str) -> Result<()> {
        let mut parents = Path::new(relative)
            .ancestors()
            .skip(1)
            .take_while(|path| !path.as_os_str().is_empty())
            .map(path_wire)
            .collect::<Result<Vec<_>>>()?;
        parents.reverse();
        for parent in parents {
            if !self.directories.contains(&parent) {
                self.dir(&parent)?;
            }
        }
        Ok(())
    }

    fn file(&mut self, relative: &str, bytes: &'a [u8]) -> Result<()> {
        validate_private_relative_path(Path::new(relative))?;
        let relative = path_wire(Path::new(relative))?;
        if self.directories.contains(&relative) || self.files.insert(relative, bytes).is_some() {
            bail!("blind bundle tree plan contains a duplicate path");
        }
        Ok(())
    }

    fn build(&self, root: &Path) -> Result<()> {
        let staging = resolve_private_relative(root, Path::new(self.staging))?;
        create_owner_only_dir_new(&staging)?;
        let mut directories = self.directories.iter().collect::<Vec<_>>();
        directories.sort_by_key(|path| (Path::new(path).components().count(), *path));
        for relative in &directories {
            create_owner_only_dir_new(&staging.join(relative))?;
        }
        for (relative, bytes) in &self.files {
            write_owner_only_new(&staging.join(relative), bytes)?;
        }
        directories.reverse();
        for relative in directories {
            fsync_directory(&staging.join(relative))?;
        }
        fsync_directory(&staging)
    }

    fn inventory_entries(&self) -> Vec<ExpectedInventoryEntry> {
        let mut entries = vec![ExpectedInventoryEntry {
            relative_path: self.final_path.to_string(),
            kind: InventoryKind::Directory,
            sha256: None,
        }];
        entries.extend(
            self.directories
                .iter()
                .map(|relative| ExpectedInventoryEntry {
                    relative_path: format!("{}/{relative}", self.final_path),
                    kind: InventoryKind::Directory,
                    sha256: None,
                }),
        );
        entries.extend(
            self.files
                .iter()
                .map(|(relative, bytes)| ExpectedInventoryEntry {
                    relative_path: format!("{}/{relative}", self.final_path),
                    kind: InventoryKind::File,
                    sha256: Some(sha256(bytes)),
                }),
        );
        entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        entries
    }
}

fn path_wire(path: &Path) -> Result<String> {
    let value = path.to_str().context("blind bundle path is not UTF-8")?;
    Ok(value.replace(std::path::MAIN_SEPARATOR, "/"))
}

fn canonical(value: &impl Serialize) -> Result<Vec<u8>> {
    Ok(crate::jcs::canonicalize_value(&serde_json::to_value(
        value,
    )?)?)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
