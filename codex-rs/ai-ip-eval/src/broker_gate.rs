use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use chrono::Utc;
use codex_protocol::ThreadId;
use codex_responses_api_proxy::ExchangeObserver;
use codex_responses_api_proxy::ForwardErrorClass;
use codex_responses_api_proxy::ForwardResult;
use codex_responses_api_proxy::RequestGate;
use codex_responses_api_proxy::RequestInspector;
use codex_responses_api_proxy::RequestMetadata;
use codex_responses_api_proxy::RequestPermit;
use codex_responses_api_proxy::RequestTransformConfig;
use codex_responses_api_proxy::ResponseCompletedMetadata;
use codex_responses_api_proxy::TransformedRequestMetadata;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use crate::EvaluationCondition;
use crate::Usage;
use crate::runner::CommittedArmOrder;

const SCHEMA_VERSION: u32 = 1;

/// Immutable identities and limits used by the paired broker gate.
///
/// The coordinator owns this configuration for the whole pair. Callers may
/// activate exactly two arms but cannot replace identifiers or limits between
/// calls.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BrokerGateConfig {
    pub ledger_path: PathBuf,
    pub receipt_dir: PathBuf,
    pub pair_id: String,
    pub frozen_run_context_sha256: String,
    pub execution_context_sha256: String,
    pub arm_order_commitment: String,
    pub require_proof_bindings: bool,
    pub runtime: Arc<BrokerRuntimeConfig>,
}

/// One immutable source for gate caps and the proxy's request transform.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BrokerRuntimeConfig {
    max_attempts_per_arm: u64,
    max_output_tokens: u64,
    max_body_bytes: usize,
    max_total_tokens_per_pair: u64,
}

impl BrokerRuntimeConfig {
    pub fn new(
        max_attempts_per_arm: u64,
        max_output_tokens: u64,
        max_body_bytes: usize,
    ) -> Result<Self> {
        if max_attempts_per_arm == 0 || max_output_tokens == 0 || max_body_bytes == 0 {
            return Err(anyhow!("broker runtime limits must be positive"));
        }
        Self::with_run_limits(
            max_attempts_per_arm,
            max_output_tokens,
            max_body_bytes,
            u64::MAX,
        )
    }

    pub fn with_run_limits(
        max_attempts_per_arm: u64,
        max_output_tokens: u64,
        max_body_bytes: usize,
        max_total_tokens_per_pair: u64,
    ) -> Result<Self> {
        if max_attempts_per_arm == 0
            || max_output_tokens == 0
            || max_body_bytes == 0
            || max_total_tokens_per_pair == 0
        {
            return Err(anyhow!("broker runtime limits must be positive"));
        }
        Ok(Self {
            max_attempts_per_arm,
            max_output_tokens,
            max_body_bytes,
            max_total_tokens_per_pair,
        })
    }

    pub fn request_transform(
        &self,
        inspector: Arc<dyn RequestInspector>,
    ) -> RequestTransformConfig {
        RequestTransformConfig {
            max_output_tokens: self.max_output_tokens,
            max_body_bytes: self.max_body_bytes,
            inspector,
        }
    }
}

/// The externally observable state of one atomic pair.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PairPhase {
    Created,
    OrderCommitted {
        first: EvaluationCondition,
        second: EvaluationCondition,
    },
    Active1 {
        condition: EvaluationCondition,
    },
    Sealed1,
    Active2 {
        condition: EvaluationCondition,
    },
    Finished,
    Poisoned {
        reason: String,
    },
}

/// Run-local facts supplied after App Server returns the arm's root thread.
#[derive(Debug, Clone)]
pub struct ArmActivation {
    pub run_ordinal: u8,
    pub condition: EvaluationCondition,
    pub root_thread_id: String,
    pub deadline: Instant,
    pub deadline_rfc3339: String,
}

/// Lifecycle events that can invalidate descendant accounting.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ThreadLifecycle {
    Subagent {
        thread_id: String,
        parent_thread_id: String,
    },
    GuardianReview,
    Guardian,
    InvalidThread,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmReceipt {
    pub schema_version: u32,
    pub pair_id: String,
    pub frozen_run_context_sha256: String,
    pub execution_context_sha256: String,
    pub run_ordinal: u8,
    pub condition: EvaluationCondition,
    pub first_condition: EvaluationCondition,
    pub second_condition: EvaluationCondition,
    pub attempt_index_file_sha256: String,
    pub attempt_index_merkle_root: String,
    pub global_attempt_start_inclusive: u64,
    pub global_attempt_end_exclusive: u64,
    pub attempt_count: u64,
    pub completion_count: u64,
    pub failure_count: u64,
    pub timeout_count: u64,
    pub in_flight: u64,
    pub sealed_at: String,
    pub previous_arm_receipt_sha256: Option<String>,
    pub run_manifest_sha256: Option<String>,
    #[serde(skip)]
    pub receipt_sha256: String,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairReceipt {
    pub schema_version: u32,
    pub pair_id: String,
    pub frozen_run_context_sha256: String,
    pub execution_context_sha256: String,
    pub first_arm_receipt_sha256: String,
    pub second_arm_receipt_sha256: String,
    pub first_run_manifest_sha256: Option<String>,
    pub second_run_manifest_sha256: Option<String>,
    pub total_attempt_count: u64,
    pub total_completion_count: u64,
    pub total_failure_count: u64,
    pub total_timeout_count: u64,
    pub arm_order_commitment: String,
    pub final_attempt_index_root: String,
    pub finished_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestRecord {
    schema_version: u32,
    record_type: &'static str,
    status: &'static str,
    pair_id: String,
    frozen_run_context_sha256: String,
    execution_context_sha256: String,
    global_attempt_index: u64,
    run_ordinal: u8,
    condition: EvaluationCondition,
    arm_attempt_index: u64,
    request_started_at: String,
    request_commitment: String,
    normalized_request_commitment: String,
    normalized_base_commitment: String,
    treatment_diff_commitment: Option<String>,
    thread_commitment: String,
    window_commitment: String,
    parent_thread_commitment: Option<String>,
    deadline: String,
    max_output_tokens: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalRecord {
    schema_version: u32,
    record_type: &'static str,
    global_attempt_index: u64,
    request_record_sha256: String,
    ended_at: String,
    status: &'static str,
    response_id_commitment: Option<String>,
    actual_model_revision: Option<String>,
    deployment_commitment: Option<String>,
    usage: Option<Usage>,
    failure_class: Option<String>,
}

struct ActiveArm {
    run_ordinal: u8,
    condition: EvaluationCondition,
    root_thread_id: String,
    known_threads: HashMap<String, Option<String>>,
    deadline: Instant,
    deadline_rfc3339: String,
    global_start: u64,
    arm_attempt_count: u64,
    completion_start: u64,
    failure_start: u64,
    timeout_start: u64,
    accepting: bool,
    completions: Vec<ResponseCompletedMetadata>,
    first_root_request: Option<FirstRootRequestEvidence>,
    run_manifest_sha256: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FirstRootRequestEvidence {
    pub raw_commitment: String,
    pub normalized_commitment: String,
    pub normalized_base_commitment: String,
    pub treatment_diff_commitment: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ActiveArmProofSnapshot {
    pub run_ordinal: u8,
    pub condition: EvaluationCondition,
    pub first_root_request: FirstRootRequestEvidence,
    pub attempt_index_file_sha256: String,
    pub attempt_index_merkle_root: String,
    pub arm_attempt_count: u64,
    pub completion_count: u64,
}

struct InFlight {
    request_record_sha256: String,
}

#[derive(Clone, Default)]
struct TerminalMetadata {
    response_id_commitment: Option<String>,
    actual_model_revision: Option<String>,
    deployment_commitment: Option<String>,
    usage: Option<Usage>,
    completion: Option<ResponseCompletedMetadata>,
}

struct Counts {
    completed: u64,
    failed: u64,
    timeout: u64,
}

struct CoordinatorState {
    phase: PairPhase,
    order: Option<(EvaluationCondition, EvaluationCondition)>,
    active: Option<ActiveArm>,
    ledger: File,
    ledger_directory: File,
    ledger_leaf: String,
    receipt_directory: File,
    ledger_bytes: Vec<u8>,
    record_hashes: Vec<[u8; 32]>,
    in_flight: HashMap<u64, InFlight>,
    terminal_metadata: HashMap<u64, TerminalMetadata>,
    invalid_completions: HashSet<u64>,
    invalid_usage: HashSet<u64>,
    completed_attempts: HashSet<u64>,
    counts: Counts,
    total_tokens: u64,
    first_receipt: Option<ArmReceipt>,
    second_receipt: Option<ArmReceipt>,
    #[cfg(test)]
    fail_next_terminal_append: bool,
}

/// Thread-safe two-arm state machine used concurrently by the proxy workers.
///
/// `before_forward` is the sole authorization point and durably appends a
/// request record before returning a permit. `response_completed` may precede
/// `after_forward`; the latter is the sole terminal writer. Any illegal call or
/// transition permanently poisons the pair.
pub struct PairCoordinator {
    config: BrokerGateConfig,
    state: Arc<Mutex<CoordinatorState>>,
}

impl PairCoordinator {
    pub fn create(config: BrokerGateConfig) -> Result<Self> {
        if let Some(parent) = config.ledger_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create ledger parent {}", parent.display()))?;
        }
        if config.receipt_dir.exists() {
            let metadata = fs::symlink_metadata(&config.receipt_dir).with_context(|| {
                format!("stat receipt directory {}", config.receipt_dir.display())
            })?;
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err(anyhow!("receipt directory must be a non-symlink directory"));
            }
        } else {
            fs::create_dir_all(&config.receipt_dir).with_context(|| {
                format!("create receipt directory {}", config.receipt_dir.display())
            })?;
        }
        set_owner_only_directory(&config.receipt_dir)?;
        let mut receipt_options = OpenOptions::new();
        receipt_options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            receipt_options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW);
        }
        let receipt_directory = receipt_options.open(&config.receipt_dir).with_context(|| {
            format!(
                "open anchored receipt directory {}",
                config.receipt_dir.display()
            )
        })?;
        let (ledger, ledger_directory, ledger_leaf) = create_anchored_ledger(&config.ledger_path)?;
        ledger.sync_all().context("fsync empty attempt ledger")?;
        Ok(Self {
            config,
            state: Arc::new(Mutex::new(CoordinatorState {
                phase: PairPhase::Created,
                order: None,
                active: None,
                ledger,
                ledger_directory,
                ledger_leaf,
                receipt_directory,
                ledger_bytes: Vec::new(),
                record_hashes: Vec::new(),
                in_flight: HashMap::new(),
                terminal_metadata: HashMap::new(),
                invalid_completions: HashSet::new(),
                invalid_usage: HashSet::new(),
                completed_attempts: HashSet::new(),
                counts: Counts {
                    completed: 0,
                    failed: 0,
                    timeout: 0,
                },
                total_tokens: 0,
                first_receipt: None,
                second_receipt: None,
                #[cfg(test)]
                fail_next_terminal_append: false,
            })),
        })
    }

    pub fn phase(&self) -> PairPhase {
        self.state.lock().map_or_else(
            |_| PairPhase::Poisoned {
                reason: "coordinator mutex poisoned".to_string(),
            },
            |state| state.phase.clone(),
        )
    }

    /// Durably and permanently invalidates this pair after any coordinator-side failure.
    pub fn poison_permanently(&self, reason: &str) -> Result<()> {
        let mut state = self.lock_state()?;
        poison(&self.config, &mut state, reason)
    }

    pub fn commit_order(&self, order: CommittedArmOrder) -> Result<()> {
        if order.frozen_run_context_sha256() != self.config.frozen_run_context_sha256 {
            return Err(anyhow!("arm order is bound to a different frozen context"));
        }
        if order.seed_commitment() != self.config.arm_order_commitment {
            return Err(anyhow!("arm order commitment does not match broker config"));
        }
        self.commit_order_inner(order.first(), order.second())
    }

    #[cfg(test)]
    pub(crate) fn commit_order_for_test(
        &self,
        first: EvaluationCondition,
        second: EvaluationCondition,
    ) -> Result<()> {
        self.commit_order_inner(first, second)
    }

    fn commit_order_inner(
        &self,
        first: EvaluationCondition,
        second: EvaluationCondition,
    ) -> Result<()> {
        let mut state = self.lock_state()?;
        if state.phase != PairPhase::Created {
            return poison(&self.config, &mut state, "order commitment outside Created");
        }
        if first == second {
            return poison(
                &self.config,
                &mut state,
                "paired conditions must be distinct",
            );
        }
        state.order = Some((first, second));
        state.phase = PairPhase::OrderCommitted { first, second };
        Ok(())
    }

    pub fn activate_arm(&self, activation: ArmActivation) -> Result<()> {
        let mut state = self.lock_state()?;
        let Some((first, second)) = state.order else {
            return poison(
                &self.config,
                &mut state,
                "arm activated before order commitment",
            );
        };
        let valid = match (&state.phase, activation.run_ordinal, activation.condition) {
            (PairPhase::OrderCommitted { .. }, 1, condition) => condition == first,
            (PairPhase::Sealed1, 2, condition) => condition == second,
            _ => false,
        };
        if !valid {
            return poison(&self.config, &mut state, "illegal arm activation");
        }
        let canonical_root = match parse_canonical_thread_id(&activation.root_thread_id) {
            Ok(root) => root,
            Err(error) => return poison(&self.config, &mut state, &error.to_string()),
        };
        let run_ordinal = activation.run_ordinal;
        let condition = activation.condition;
        let root_thread_id = canonical_root;
        let global_start = u64::try_from(state.record_hashes.len() / 2)
            .context("attempt index does not fit u64")?;
        state.active = Some(ActiveArm {
            run_ordinal,
            condition,
            root_thread_id: root_thread_id.clone(),
            known_threads: HashMap::from([(root_thread_id, None)]),
            deadline: activation.deadline,
            deadline_rfc3339: activation.deadline_rfc3339,
            global_start,
            arm_attempt_count: 0,
            completion_start: state.counts.completed,
            failure_start: state.counts.failed,
            timeout_start: state.counts.timeout,
            accepting: true,
            completions: Vec::new(),
            first_root_request: None,
            run_manifest_sha256: None,
        });
        state.phase = if run_ordinal == 1 {
            PairPhase::Active1 { condition }
        } else {
            PairPhase::Active2 { condition }
        };
        Ok(())
    }

    pub fn observe_lifecycle(&self, lifecycle: ThreadLifecycle) -> Result<()> {
        let mut state = self.lock_state()?;
        if !matches!(
            state.phase,
            PairPhase::Active1 { .. } | PairPhase::Active2 { .. }
        ) {
            return poison(
                &self.config,
                &mut state,
                "lifecycle outside exact active phase",
            );
        }
        let Some(active) = state.active.as_mut() else {
            return poison(&self.config, &mut state, "lifecycle outside active arm");
        };
        if !active.accepting {
            return poison(&self.config, &mut state, "lifecycle after arm seal");
        }
        match lifecycle {
            ThreadLifecycle::GuardianReview | ThreadLifecycle::Guardian => {
                poison(&self.config, &mut state, "Guardian lifecycle is forbidden")
            }
            ThreadLifecycle::InvalidThread => poison(
                &self.config,
                &mut state,
                "non-Subagent descendant lifecycle is forbidden",
            ),
            ThreadLifecycle::Subagent {
                thread_id,
                parent_thread_id,
            } => {
                let thread_id = match parse_canonical_thread_id(&thread_id) {
                    Ok(thread_id) => thread_id,
                    Err(error) => return poison(&self.config, &mut state, &error.to_string()),
                };
                let parent_thread_id = match parse_canonical_thread_id(&parent_thread_id) {
                    Ok(parent_thread_id) => parent_thread_id,
                    Err(error) => return poison(&self.config, &mut state, &error.to_string()),
                };
                if !active.known_threads.contains_key(&parent_thread_id) {
                    return poison(&self.config, &mut state, "unknown descendant parent");
                }
                if active.known_threads.contains_key(&thread_id) {
                    return poison(
                        &self.config,
                        &mut state,
                        "duplicate or conflicting descendant registration",
                    );
                }
                active
                    .known_threads
                    .insert(thread_id, Some(parent_thread_id));
                Ok(())
            }
        }
    }

    pub fn known_threads(&self) -> HashSet<String> {
        self.state
            .lock()
            .ok()
            .and_then(|state| {
                state
                    .active
                    .as_ref()
                    .map(|active| active.known_threads.keys().cloned().collect())
            })
            .unwrap_or_default()
    }

    pub fn attempt_count(&self) -> u64 {
        self.state
            .lock()
            .ok()
            .and_then(|state| u64::try_from(state.completed_attempts.len()).ok())
            .unwrap_or(0)
    }

    pub fn in_flight_count(&self) -> u64 {
        self.state
            .lock()
            .ok()
            .and_then(|state| u64::try_from(state.in_flight.len()).ok())
            .unwrap_or(u64::MAX)
    }

    pub(crate) fn active_completions(&self) -> Result<Vec<ResponseCompletedMetadata>> {
        let state = self.lock_state()?;
        state
            .active
            .as_ref()
            .map(|active| active.completions.clone())
            .context("completion snapshot requested outside an active arm")
    }

    pub(crate) fn active_thread_ids(&self) -> Result<HashSet<String>> {
        let state = self.lock_state()?;
        state
            .active
            .as_ref()
            .map(|active| active.known_threads.keys().cloned().collect())
            .context("thread snapshot requested outside an active arm")
    }

    pub(crate) fn active_inspection_context(
        &self,
    ) -> Result<(EvaluationCondition, u64, HashSet<String>)> {
        let state = self.lock_state()?;
        state
            .active
            .as_ref()
            .map(|active| {
                (
                    active.condition,
                    active.arm_attempt_count,
                    active.known_threads.keys().cloned().collect(),
                )
            })
            .context("inspection context requested outside an active arm")
    }

    pub(crate) fn active_arm_proof_snapshot(&self) -> Result<ActiveArmProofSnapshot> {
        let state = self.lock_state()?;
        let active = state
            .active
            .as_ref()
            .context("proof snapshot requested outside an active arm")?;
        Ok(ActiveArmProofSnapshot {
            run_ordinal: active.run_ordinal,
            condition: active.condition,
            first_root_request: active
                .first_root_request
                .clone()
                .context("active arm has no first-root request evidence")?,
            attempt_index_file_sha256: sha256_hex(&state.ledger_bytes),
            attempt_index_merkle_root: merkle_root(&state.record_hashes),
            arm_attempt_count: active.arm_attempt_count,
            completion_count: u64::try_from(active.completions.len())?,
        })
    }

    pub(crate) fn bind_active_run_manifest(&self, sha256: String) -> Result<()> {
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(anyhow!(
                "run manifest commitment must be lowercase SHA-256 hex"
            ));
        }
        let mut state = self.lock_state()?;
        let active = state
            .active
            .as_mut()
            .context("run manifest bound outside an active arm")?;
        if active.run_manifest_sha256.replace(sha256).is_some() {
            return poison(
                &self.config,
                &mut state,
                "run manifest was bound more than once",
            );
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn fail_next_terminal_append_for_test(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.fail_next_terminal_append = true;
        }
    }

    pub fn seal_arm(&self) -> Result<ArmReceipt> {
        let mut state = self.lock_state()?;
        if let Err(error) = verify_anchored_ledger(&self.config, &state) {
            return poison(
                &self.config,
                &mut state,
                &format!("attempt ledger identity or prefix changed: {error:#}"),
            );
        }
        let phase = state.phase.clone();
        if !state.in_flight.is_empty() {
            return poison(
                &self.config,
                &mut state,
                "cannot seal with requests in flight",
            );
        }
        let Some(active) = state.active.as_ref() else {
            return poison(&self.config, &mut state, "seal outside active arm");
        };
        if !matches!(phase, PairPhase::Active1 { .. } | PairPhase::Active2 { .. })
            || !active.accepting
        {
            return poison(&self.config, &mut state, "duplicate or illegal arm seal");
        }
        let run_ordinal = active.run_ordinal;
        let condition = active.condition;
        let global_start = active.global_start;
        let arm_attempt_count = active.arm_attempt_count;
        let completion_start = active.completion_start;
        let failure_start = active.failure_start;
        let timeout_start = active.timeout_start;
        let run_manifest_sha256 = active.run_manifest_sha256.clone();
        if self.config.require_proof_bindings && run_manifest_sha256.is_none() {
            return poison(
                &self.config,
                &mut state,
                "cannot seal arm without a bound run manifest",
            );
        }
        let (first_condition, second_condition) = state
            .order
            .ok_or_else(|| anyhow!("missing committed order"))?;
        let attempt_index_file_sha256 = sha256_hex(&state.ledger_bytes);
        let attempt_index_merkle_root = merkle_root(&state.record_hashes);
        let previous_arm_receipt_sha256 = state
            .first_receipt
            .as_ref()
            .map(|receipt| receipt.receipt_sha256.clone());
        if run_ordinal == 2
            && let Some(first) = state.first_receipt.as_ref()
            && let Err(error) =
                verify_anchored_receipt(&state, "arm-1-receipt.json", &first.receipt_sha256)
        {
            return poison(
                &self.config,
                &mut state,
                &format!("first arm receipt changed before second seal: {error:#}"),
            );
        }
        let mut receipt = ArmReceipt {
            schema_version: SCHEMA_VERSION,
            pair_id: self.config.pair_id.clone(),
            frozen_run_context_sha256: self.config.frozen_run_context_sha256.clone(),
            execution_context_sha256: self.config.execution_context_sha256.clone(),
            run_ordinal,
            condition,
            first_condition,
            second_condition,
            attempt_index_file_sha256,
            attempt_index_merkle_root,
            global_attempt_start_inclusive: global_start,
            global_attempt_end_exclusive: global_start + arm_attempt_count,
            attempt_count: arm_attempt_count,
            completion_count: state.counts.completed - completion_start,
            failure_count: state.counts.failed - failure_start,
            timeout_count: state.counts.timeout - timeout_start,
            in_flight: 0,
            sealed_at: now(),
            previous_arm_receipt_sha256,
            run_manifest_sha256,
            receipt_sha256: String::new(),
        };
        let bytes = serde_json::to_vec(&receipt).context("serialize arm receipt")?;
        let mut file_bytes = bytes.clone();
        file_bytes.push(b'\n');
        receipt.receipt_sha256 = sha256_hex(&file_bytes);
        if let Err(error) = write_receipt_new_synced(
            &self.config,
            &state,
            &format!("arm-{run_ordinal}-receipt.json"),
            &bytes,
        ) {
            return poison(
                &self.config,
                &mut state,
                &format!("arm receipt persistence failed: {error:#}"),
            );
        }
        if let Some(active) = state.active.as_mut() {
            active.accepting = false;
        }
        if run_ordinal == 1 {
            state.first_receipt = Some(receipt.clone());
            state.phase = PairPhase::Sealed1;
            state.active = None;
        } else {
            state.second_receipt = Some(receipt.clone());
        }
        Ok(receipt)
    }

    pub fn finish(&self) -> Result<PairReceipt> {
        let mut state = self.lock_state()?;
        if let Err(error) = verify_anchored_ledger(&self.config, &state) {
            return poison(
                &self.config,
                &mut state,
                &format!("attempt ledger identity or prefix changed: {error:#}"),
            );
        }
        if !matches!(state.phase, PairPhase::Active2 { .. })
            || state.active.as_ref().is_none_or(|active| active.accepting)
        {
            return poison(&self.config, &mut state, "finish before second arm seal");
        }
        let first = state
            .first_receipt
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow!("missing first arm receipt"))?;
        let second = state
            .second_receipt
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow!("missing second arm receipt"))?;
        for (leaf, expected) in [
            ("arm-1-receipt.json", first.receipt_sha256.as_str()),
            ("arm-2-receipt.json", second.receipt_sha256.as_str()),
        ] {
            if let Err(error) = verify_anchored_receipt(&state, leaf, expected) {
                return poison(
                    &self.config,
                    &mut state,
                    &format!("arm receipt changed before pair finish: {error:#}"),
                );
            }
        }
        let total_attempt_count = u64::try_from(state.completed_attempts.len())
            .context("attempt count does not fit u64")?;
        let receipt = PairReceipt {
            schema_version: SCHEMA_VERSION,
            pair_id: self.config.pair_id.clone(),
            frozen_run_context_sha256: self.config.frozen_run_context_sha256.clone(),
            execution_context_sha256: self.config.execution_context_sha256.clone(),
            first_arm_receipt_sha256: first.receipt_sha256,
            second_arm_receipt_sha256: second.receipt_sha256,
            first_run_manifest_sha256: first.run_manifest_sha256,
            second_run_manifest_sha256: second.run_manifest_sha256,
            total_attempt_count,
            total_completion_count: state.counts.completed,
            total_failure_count: state.counts.failed,
            total_timeout_count: state.counts.timeout,
            arm_order_commitment: self.config.arm_order_commitment.clone(),
            final_attempt_index_root: merkle_root(&state.record_hashes),
            finished_at: now(),
        };
        let bytes = serde_json::to_vec(&receipt).context("serialize pair receipt")?;
        if let Err(error) =
            write_receipt_new_synced(&self.config, &state, "pair-receipt.json", &bytes)
        {
            return poison(
                &self.config,
                &mut state,
                &format!("pair receipt persistence failed: {error:#}"),
            );
        }
        state.phase = PairPhase::Finished;
        state.active = None;
        Ok(receipt)
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, CoordinatorState>> {
        self.state
            .lock()
            .map_err(|_| anyhow!("coordinator mutex poisoned"))
    }
}

impl RequestGate for PairCoordinator {
    fn before_forward(
        &self,
        request: &RequestMetadata,
        transformed: &TransformedRequestMetadata,
    ) -> Result<RequestPermit> {
        let mut state = self.lock_state()?;
        let result = authorize_and_record(&self.config, &mut state, request, transformed);
        match result {
            Ok(permit) => Ok(permit),
            Err(error) => {
                let reason = error.to_string();
                let _ = poison::<()>(&self.config, &mut state, &reason);
                Err(error)
            }
        }
    }

    fn after_forward(&self, permit: RequestPermit, result: &ForwardResult) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        match append_terminal(&self.config, &mut state, permit.attempt_id(), result) {
            Err(error) => {
                let _ = poison::<()>(&self.config, &mut state, &error.to_string());
            }
            Ok(true) => {
                let _ = poison::<()>(
                    &self.config,
                    &mut state,
                    "provider attempt failed; retries are disabled",
                );
            }
            Ok(false) => {}
        }
    }
}

impl ExchangeObserver for PairCoordinator {
    fn response_completed(&self, permit: &RequestPermit, event: &ResponseCompletedMetadata) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !state.in_flight.contains_key(&permit.attempt_id())
            || state.terminal_metadata.contains_key(&permit.attempt_id())
        {
            state.invalid_completions.insert(permit.attempt_id());
            let _ = poison::<()>(
                &self.config,
                &mut state,
                "duplicate or unknown response event",
            );
            return;
        }
        if validate_observed_usage(event.usage.as_ref()).is_err() {
            state.invalid_usage.insert(permit.attempt_id());
            let _ = poison::<()>(&self.config, &mut state, "invalid response usage");
            return;
        }
        state.terminal_metadata.insert(
            permit.attempt_id(),
            TerminalMetadata {
                response_id_commitment: Some(sha256_hex(event.response_id.as_bytes())),
                actual_model_revision: event.actual_model.clone(),
                deployment_commitment: event
                    .deployment_or_fingerprint
                    .as_ref()
                    .map(|value| sha256_hex(value.as_bytes())),
                usage: event.usage.as_ref().map(|usage| Usage {
                    total_tokens: usage.total_tokens,
                    input_tokens: usage.input_tokens,
                    cached_input_tokens: usage.cached_input_tokens,
                    cache_write_input_tokens: usage.cache_write_input_tokens,
                    output_tokens: usage.output_tokens,
                    reasoning_output_tokens: usage.reasoning_output_tokens,
                }),
                completion: Some(event.clone()),
            },
        );
    }
}

fn authorize_and_record(
    config: &BrokerGateConfig,
    state: &mut CoordinatorState,
    request: &RequestMetadata,
    transformed: &TransformedRequestMetadata,
) -> Result<RequestPermit> {
    if transformed.sha256 != transformed.evidence.raw_sha256 {
        return Err(anyhow!(
            "request inspector raw commitment differs from the forwarded body"
        ));
    }
    if !matches!(
        state.phase,
        PairPhase::Active1 { .. } | PairPhase::Active2 { .. }
    ) {
        return Err(anyhow!("request outside the exact active pair phase"));
    }
    let (
        run_ordinal,
        condition,
        arm_attempt_index,
        global_attempt_index,
        deadline,
        deadline_rfc3339,
    ) = {
        let Some(active) = state.active.as_mut() else {
            return Err(anyhow!("request outside active arm"));
        };
        if !active.accepting {
            return Err(anyhow!("request between arm seal and pair transition"));
        }
        if request.method != "POST" || request.path != "/v1/responses" {
            return Err(anyhow!("only POST /v1/responses is allowed"));
        }
        if Instant::now() >= active.deadline {
            return Err(anyhow!("arm deadline expired"));
        }
        if active.arm_attempt_count >= config.runtime.max_attempts_per_arm {
            return Err(anyhow!("per-arm attempt cap exceeded"));
        }
        let window = request
            .window_id
            .as_deref()
            .ok_or_else(|| anyhow!("missing x-codex-window-id"))?;
        let thread_id = parse_window_thread(window)?;
        if request.is_subagent {
            let parent = parse_canonical_thread_id(
                request
                    .parent_thread_id
                    .as_deref()
                    .ok_or_else(|| anyhow!("parentless descendant"))?,
            )?;
            match active.known_threads.get(&thread_id) {
                Some(Some(committed_parent)) if committed_parent == &parent => {}
                Some(_) => return Err(anyhow!("descendant lifecycle parent mismatch")),
                None => return Err(anyhow!("descendant lifecycle was not committed")),
            }
        } else if thread_id != active.root_thread_id || request.parent_thread_id.is_some() {
            return Err(anyhow!("root thread does not match active root"));
        }
        (
            active.run_ordinal,
            active.condition,
            active.arm_attempt_count,
            active.global_start + active.arm_attempt_count,
            active.deadline,
            active.deadline_rfc3339.clone(),
        )
    };
    let window = request
        .window_id
        .as_deref()
        .ok_or_else(|| anyhow!("missing window"))?;
    let thread_id = parse_window_thread(window)?;
    let first_root_request =
        (arm_attempt_index == 0 && !request.is_subagent).then(|| FirstRootRequestEvidence {
            raw_commitment: hex(transformed.evidence.raw_sha256),
            normalized_commitment: hex(transformed.evidence.normalized_sha256),
            normalized_base_commitment: hex(transformed.evidence.normalized_base_commitment),
            treatment_diff_commitment: transformed.evidence.treatment_diff_commitment.map(hex),
        });
    let record = RequestRecord {
        schema_version: SCHEMA_VERSION,
        record_type: "request",
        status: "forwarding",
        pair_id: config.pair_id.clone(),
        frozen_run_context_sha256: config.frozen_run_context_sha256.clone(),
        execution_context_sha256: config.execution_context_sha256.clone(),
        global_attempt_index,
        run_ordinal,
        condition,
        arm_attempt_index,
        request_started_at: now(),
        request_commitment: hex(transformed.evidence.raw_sha256),
        normalized_request_commitment: hex(transformed.evidence.normalized_sha256),
        normalized_base_commitment: hex(transformed.evidence.normalized_base_commitment),
        treatment_diff_commitment: transformed.evidence.treatment_diff_commitment.map(hex),
        thread_commitment: sha256_hex(thread_id.as_bytes()),
        window_commitment: sha256_hex(window.as_bytes()),
        parent_thread_commitment: request
            .parent_thread_id
            .as_ref()
            .map(|parent| sha256_hex(parent.as_bytes())),
        deadline: deadline_rfc3339,
        max_output_tokens: config.runtime.max_output_tokens,
    };
    let bytes = serde_json::to_vec(&record).context("serialize request record")?;
    let record_hash: [u8; 32] = Sha256::digest(&bytes).into();
    append_synced(state, &bytes)?;
    if let Some(active) = state.active.as_mut() {
        if let Some(first_root_request) = first_root_request
            && active
                .first_root_request
                .replace(first_root_request)
                .is_some()
        {
            return Err(anyhow!(
                "first-root request evidence was recorded more than once"
            ));
        }
        active.arm_attempt_count += 1;
    }
    state.record_hashes.push(record_hash);
    state.in_flight.insert(
        global_attempt_index,
        InFlight {
            request_record_sha256: hex(record_hash),
        },
    );
    Ok(RequestPermit::new(global_attempt_index, deadline))
}

fn append_terminal(
    config: &BrokerGateConfig,
    state: &mut CoordinatorState,
    attempt_id: u64,
    result: &ForwardResult,
) -> Result<bool> {
    if state.completed_attempts.contains(&attempt_id) {
        return Err(anyhow!("attempt already has a terminal record"));
    }
    let in_flight = state
        .in_flight
        .get(&attempt_id)
        .ok_or_else(|| anyhow!("terminal record has no request"))?;
    let metadata = state.terminal_metadata.get(&attempt_id).cloned();
    let invalid_completion = state.invalid_completions.contains(&attempt_id);
    let invalid_usage = state.invalid_usage.contains(&attempt_id);
    let observed_total_tokens = metadata
        .as_ref()
        .and_then(|metadata| metadata.usage.as_ref())
        .map(|usage| usage.total_tokens);
    let token_cap_exceeded = match observed_total_tokens {
        Some(tokens) if tokens < 0 => true,
        Some(tokens) => u64::try_from(tokens)
            .ok()
            .and_then(|tokens| state.total_tokens.checked_add(tokens))
            .is_none_or(|total| total > config.runtime.max_total_tokens_per_pair),
        _ => false,
    };
    let (status, failure_class, count_kind) = match result {
        ForwardResult::Completed { .. } if token_cap_exceeded => {
            ("failed", Some("maxTotalTokensExceeded".to_string()), 1_u8)
        }
        ForwardResult::Completed { .. } if invalid_usage => {
            ("failed", Some("invalidResponseUsage".to_string()), 1_u8)
        }
        ForwardResult::Completed { .. } if invalid_completion => (
            "failed",
            Some("duplicateResponseCompleted".to_string()),
            1_u8,
        ),
        ForwardResult::Completed { .. } if metadata.is_none() => {
            ("failed", Some("missingResponseCompleted".to_string()), 1_u8)
        }
        ForwardResult::Completed { .. } => ("completed", None, 0_u8),
        ForwardResult::Failed {
            class: ForwardErrorClass::DeadlineExceeded,
        } => ("timeout", Some("deadlineExceeded".to_string()), 2_u8),
        ForwardResult::Failed { class } => ("failed", Some(format!("{class:?}")), 1_u8),
    };
    let metadata = metadata.unwrap_or_default();
    let completion = metadata.completion.clone();
    let record = TerminalRecord {
        schema_version: SCHEMA_VERSION,
        record_type: "terminal",
        global_attempt_index: attempt_id,
        request_record_sha256: in_flight.request_record_sha256.clone(),
        ended_at: now(),
        status,
        response_id_commitment: metadata.response_id_commitment,
        actual_model_revision: metadata.actual_model_revision,
        deployment_commitment: metadata.deployment_commitment,
        usage: metadata.usage,
        failure_class,
    };
    let bytes = serde_json::to_vec(&record).context("serialize terminal record")?;
    #[cfg(test)]
    if state.fail_next_terminal_append {
        state.fail_next_terminal_append = false;
        return Err(anyhow!("injected terminal append failure"));
    }
    append_synced(state, &bytes)?;
    state.in_flight.remove(&attempt_id);
    state.terminal_metadata.remove(&attempt_id);
    state.invalid_completions.remove(&attempt_id);
    state.invalid_usage.remove(&attempt_id);
    if let Some(tokens) = observed_total_tokens
        && let Ok(tokens) = u64::try_from(tokens)
    {
        state.total_tokens = state.total_tokens.saturating_add(tokens);
    }
    if count_kind == 0
        && let (Some(active), Some(completion)) = (state.active.as_mut(), completion)
    {
        active.completions.push(completion);
    }
    match count_kind {
        0 => state.counts.completed += 1,
        1 => state.counts.failed += 1,
        _ => state.counts.timeout += 1,
    }
    state.record_hashes.push(Sha256::digest(&bytes).into());
    state.completed_attempts.insert(attempt_id);
    Ok(count_kind != 0)
}

fn validate_observed_usage(usage: Option<&codex_responses_api_proxy::ObservedUsage>) -> Result<()> {
    let usage = usage.context("response completion is missing usage")?;
    let values = [
        usage.total_tokens,
        usage.input_tokens,
        usage.cached_input_tokens,
        usage.cache_write_input_tokens,
        usage.output_tokens,
        usage.reasoning_output_tokens,
    ];
    if values.into_iter().any(|value| value < 0) {
        bail!("negative response usage");
    }
    if usage.input_tokens.checked_add(usage.output_tokens) != Some(usage.total_tokens) {
        bail!("response total usage is inconsistent");
    }
    if usage
        .cached_input_tokens
        .checked_add(usage.cache_write_input_tokens)
        .is_none_or(|cached| cached > usage.input_tokens)
        || usage.reasoning_output_tokens > usage.output_tokens
    {
        bail!("response usage breakdown is inconsistent");
    }
    Ok(())
}

fn append_synced(state: &mut CoordinatorState, bytes: &[u8]) -> Result<()> {
    state
        .ledger
        .write_all(bytes)
        .context("append attempt record")?;
    state
        .ledger
        .write_all(b"\n")
        .context("append attempt newline")?;
    state.ledger.sync_all().context("fsync attempt ledger")?;
    state.ledger_bytes.extend_from_slice(bytes);
    state.ledger_bytes.push(b'\n');
    Ok(())
}

#[cfg(unix)]
fn verify_anchored_ledger(_config: &BrokerGateConfig, state: &CoordinatorState) -> Result<()> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::unix::fs::FileExt;
    use std::os::unix::fs::MetadataExt;

    let leaf = CString::new(state.ledger_leaf.as_bytes()).context("ledger leaf contains NUL")?;
    let descriptor = unsafe {
        libc::openat(
            state.ledger_directory.as_raw_fd(),
            leaf.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err(std::io::Error::last_os_error()).context("reopen anchored attempt ledger");
    }
    let current = unsafe { File::from_raw_fd(descriptor) };
    let retained_metadata = state.ledger.metadata()?;
    let current_metadata = current.metadata()?;
    if retained_metadata.dev() != current_metadata.dev()
        || retained_metadata.ino() != current_metadata.ino()
    {
        bail!("attempt ledger pathname no longer names the retained file");
    }
    if retained_metadata.len() != u64::try_from(state.ledger_bytes.len())? {
        bail!("attempt ledger was truncated or extended outside the coordinator");
    }
    let mut bytes = vec![0_u8; state.ledger_bytes.len()];
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let read = state
            .ledger
            .read_at(&mut bytes[offset..], u64::try_from(offset)?)?;
        if read == 0 {
            bail!("attempt ledger ended before the committed prefix");
        }
        offset += read;
    }
    if bytes != state.ledger_bytes {
        bail!("attempt ledger bytes differ from the committed append-only prefix");
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_anchored_ledger(_config: &BrokerGateConfig, _state: &CoordinatorState) -> Result<()> {
    bail!("attempt ledger verification requires Unix openat semantics")
}

#[cfg(unix)]
fn create_anchored_ledger(path: &std::path::Path) -> Result<(File, File, String)> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path.parent().context("attempt ledger has no parent")?;
    let leaf = path
        .file_name()
        .context("attempt ledger has no leaf name")?;
    let leaf_text = leaf
        .to_str()
        .context("attempt ledger leaf is not UTF-8")?
        .to_string();
    if leaf_text.contains('/') || leaf_text.contains('\\') {
        bail!("attempt ledger name is not a leaf");
    }
    let mut directory_options = OpenOptions::new();
    directory_options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW);
    let directory = directory_options
        .open(parent)
        .with_context(|| format!("open anchored ledger directory {}", parent.display()))?;
    let leaf_c = CString::new(leaf.as_bytes()).context("attempt ledger leaf contains NUL")?;
    let descriptor = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            leaf_c.as_ptr(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if descriptor < 0 {
        return Err(std::io::Error::last_os_error()).context("create anchored attempt ledger");
    }
    Ok((
        unsafe { File::from_raw_fd(descriptor) },
        directory,
        leaf_text,
    ))
}

#[cfg(not(unix))]
fn create_anchored_ledger(_path: &std::path::Path) -> Result<(File, File, String)> {
    bail!("attempt ledger creation requires Unix openat semantics")
}

fn poison<T>(config: &BrokerGateConfig, state: &mut CoordinatorState, reason: &str) -> Result<T> {
    if !matches!(state.phase, PairPhase::Poisoned { .. }) {
        state.phase = PairPhase::Poisoned {
            reason: reason.to_string(),
        };
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schemaVersion": SCHEMA_VERSION,
            "pairId": config.pair_id,
            "poisonedAt": now(),
            "reason": reason,
        }))?;
        write_receipt_new_synced(config, state, "poison.json", &bytes)?;
    }
    Err(anyhow!(reason.to_string()))
}

fn parse_window_thread(window: &str) -> Result<String> {
    let (thread, sequence) = window
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("malformed x-codex-window-id"))?;
    let parsed_sequence = sequence
        .parse::<u64>()
        .map_err(|_| anyhow!("malformed x-codex-window-id"))?;
    if parsed_sequence.to_string() != sequence {
        return Err(anyhow!("malformed x-codex-window-id"));
    }
    parse_canonical_thread_id(thread)
}

fn parse_canonical_thread_id(value: &str) -> Result<String> {
    let parsed = ThreadId::from_string(value)
        .map_err(|error| anyhow!("invalid protocol thread id: {error}"))?;
    let canonical = parsed.to_string();
    if canonical != value {
        return Err(anyhow!("non-canonical protocol thread id"));
    }
    Ok(canonical)
}

#[cfg(not(unix))]
fn write_new_synced(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write {}", path.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("terminate {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("fsync {}", path.display()))
}

#[cfg(unix)]
fn write_receipt_new_synced(
    _config: &BrokerGateConfig,
    state: &CoordinatorState,
    leaf: &str,
    bytes: &[u8],
) -> Result<()> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;

    if leaf.contains('/') || leaf.contains('\\') {
        return Err(anyhow!("receipt name is not a leaf"));
    }
    let leaf = CString::new(leaf).context("receipt name contains NUL")?;
    let descriptor = unsafe {
        libc::openat(
            state.receipt_directory.as_raw_fd(),
            leaf.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if descriptor < 0 {
        return Err(std::io::Error::last_os_error()).context("create anchored receipt");
    }
    let mut file = unsafe { File::from_raw_fd(descriptor) };
    file.write_all(bytes).context("write anchored receipt")?;
    file.write_all(b"\n")
        .context("terminate anchored receipt")?;
    file.sync_all().context("fsync anchored receipt")
}

#[cfg(unix)]
fn verify_anchored_receipt(
    state: &CoordinatorState,
    leaf: &str,
    expected_sha256: &str,
) -> Result<()> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::os::unix::fs::FileExt;

    let leaf = CString::new(leaf).context("receipt leaf contains NUL")?;
    let descriptor = unsafe {
        libc::openat(
            state.receipt_directory.as_raw_fd(),
            leaf.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err(std::io::Error::last_os_error()).context("open anchored receipt for verify");
    }
    let file = unsafe { File::from_raw_fd(descriptor) };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        bail!("anchored receipt is not a bounded regular file");
    }
    let mut bytes = vec![0_u8; usize::try_from(metadata.len())?];
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let read = file.read_at(&mut bytes[offset..], u64::try_from(offset)?)?;
        if read == 0 {
            bail!("anchored receipt was truncated while reading");
        }
        offset += read;
    }
    if sha256_hex(&bytes) != expected_sha256 {
        bail!("anchored receipt digest differs from the staged receipt material");
    }
    Ok(())
}

#[cfg(not(unix))]
fn verify_anchored_receipt(
    _state: &CoordinatorState,
    _leaf: &str,
    _expected_sha256: &str,
) -> Result<()> {
    bail!("receipt verification requires Unix openat semantics")
}

#[cfg(not(unix))]
fn write_receipt_new_synced(
    config: &BrokerGateConfig,
    _state: &CoordinatorState,
    leaf: &str,
    bytes: &[u8],
) -> Result<()> {
    write_new_synced(&config.receipt_dir.join(leaf), bytes)
}

fn set_owner_only_directory(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("set owner-only permissions on {}", path.display()))?;
    }
    Ok(())
}

fn merkle_root(record_hashes: &[[u8; 32]]) -> String {
    let mut root = [0; 32];
    for hash in record_hashes {
        let mut hasher = Sha256::new();
        hasher.update(root);
        hasher.update(hash);
        root = hasher.finalize().into();
    }
    hex(root)
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(Sha256::digest(bytes).into())
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
