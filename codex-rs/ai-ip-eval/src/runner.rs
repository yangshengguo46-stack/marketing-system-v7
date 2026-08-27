use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use rand::TryRngCore;
use rand::rngs::OsRng;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use crate::AppServerClient;
use crate::ArmActivation;
use crate::BrokerGateConfig;
use crate::BrokerRuntimeConfig;
use crate::ConfigAuditExpectation;
use crate::EvaluationCondition;
use crate::PairCoordinator;
use crate::ThreadLifecycle;
use crate::audit_config;
use crate::build_shared_config;
use crate::build_thread_start;
use crate::build_turn_start;
use crate::model::LiveFreezeArgs;
use crate::model::ReplayFreezeArgs;

/// A frozen context whose current bytes were re-read and hashed successfully.
#[derive(Debug, Clone)]
pub struct VerifiedFrozenContext {
    canonical_path: PathBuf,
    sha256: String,
    context: FrozenRunContext,
    frozen_file: ArtifactCommitment,
    artifacts: ArtifactCommitments,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FrozenArtifactReference {
    path: PathBuf,
    sha256: String,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FrozenRunContext {
    schema_version: u32,
    execution_mode: String,
    provider_mode: String,
    pair_id: String,
    public_run_id: String,
    candidate_sha: String,
    private_root: PathBuf,
    repo_root: PathBuf,
    repo_head: String,
    provider_upstream_url: String,
    model_label: String,
    max_output_tokens: u64,
    max_attempts_per_arm: u64,
    max_total_tokens: u64,
    max_elapsed_seconds: u64,
    artifacts: BTreeMap<String, FrozenArtifactReference>,
}

impl VerifiedFrozenContext {
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }

    pub fn pair_id(&self) -> &str {
        &self.context.pair_id
    }

    pub fn max_output_tokens(&self) -> u64 {
        self.context.max_output_tokens
    }

    pub fn max_attempts_per_arm(&self) -> u64 {
        self.context.max_attempts_per_arm
    }

    pub fn provider_upstream_url(&self) -> &str {
        &self.context.provider_upstream_url
    }

    fn execution_guard(&self) -> Result<FrozenExecutionGuard> {
        FrozenExecutionGuard::from_commitments(self.artifacts.clone(), &self.context.repo_root)
    }

    fn artifact_bytes(&self, name: &str) -> Result<Vec<u8>> {
        self.artifacts.read_verified(name)
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ReplayFrozenContext {
    schema_version: u32,
    execution_mode: &'static str,
    provider_mode: &'static str,
    repo_root: PathBuf,
    fork_sha: String,
    private_root: PathBuf,
    codex_binary_sha256: String,
    case_sha256: String,
    transcript_sha256: String,
    fixture_set_manifest_sha256: String,
}

/// Writes a typed replay freeze record with no provider-capable fields.
pub fn freeze_replay_context(args: ReplayFreezeArgs) -> Result<()> {
    let record = ReplayFrozenContext {
        schema_version: 1,
        execution_mode: "replay",
        provider_mode: "not-run",
        repo_root: args
            .repo_root
            .canonicalize()
            .context("canonicalize replay repo")?,
        fork_sha: args.fork_sha,
        private_root: args.private_root,
        codex_binary_sha256: sha256(&read_regular_file_no_follow(&args.codex_bin)?),
        case_sha256: sha256(&read_regular_file_no_follow(&args.case)?),
        transcript_sha256: sha256(&read_regular_file_no_follow(&args.transcript)?),
        fixture_set_manifest_sha256: sha256(&read_regular_file_no_follow(
            &args.fixture_set_manifest,
        )?),
    };
    write_owner_only_new(&args.output, &serde_json::to_vec_pretty(&record)?)
}

/// Freezes the strict local-mock live context consumed by [`run_local_mock_pair`].
pub fn freeze_live_context(args: LiveFreezeArgs) -> Result<()> {
    validate_local_mock_upstream(&args.provider_upstream_url)?;
    if args.authorized_total_cost_fen != 0 || args.authorized_per_run_cost_fen != 0 {
        bail!("local mock evaluation must have zero authorized cost");
    }
    let repo_root = args
        .repo_root
        .canonicalize()
        .context("canonicalize live repo")?;
    let repo = GitWorktreeCommitment::freeze(&repo_root)?;
    if repo.head() != args.fork_sha {
        bail!("live freeze fork SHA does not match clean worktree HEAD");
    }
    let canonical_private_root = args
        .private_root
        .canonicalize()
        .context("canonicalize evaluation private root")?;
    if args.output.file_name() != Some(OsStr::new("frozen-run-context.json"))
        || args
            .output
            .parent()
            .context("live frozen context output has no parent")?
            .canonicalize()?
            != canonical_private_root
    {
        bail!("live frozen context must use the canonical private-root filename");
    }
    let generated_dir = canonical_private_root.join("frozen-inputs");
    create_owner_only_dir(&generated_dir)?;
    let imported = BTreeMap::from([
        ("source", (args.case.as_path(), "source.json")),
        ("materials", (args.attestation.as_path(), "materials.json")),
        (
            "providerBudgetReceipt",
            (
                args.provider_budget_evidence.as_path(),
                "provider-budget-receipt.json",
            ),
        ),
        ("rateCard", (args.rate_card.as_path(), "rate-card.json")),
        (
            "billingPolicy",
            (args.billing_policy.as_path(), "billing-policy.json"),
        ),
        ("fxPolicy", (args.fx_policy.as_path(), "fx-policy.json")),
        ("skill", (args.lead_skill.as_path(), "lead-skill.md")),
    ]);
    let mut imported_paths = BTreeMap::new();
    for (name, (source, leaf)) in imported {
        let destination = generated_dir.join(leaf);
        write_owner_only_new(&destination, &read_regular_file_no_follow(source)?)?;
        imported_paths.insert(name.to_string(), destination);
    }
    let case_bytes = read_regular_file_no_follow(&imported_paths["source"])?;
    let mission_case: codex_ai_ip_domain::HeldOutMissionCase =
        serde_json::from_slice(&case_bytes).context("parse held-out live case")?;
    let schema_path = generated_dir.join("content-package-schema.json");
    write_owner_only_new(
        &schema_path,
        &serde_json::to_vec_pretty(&codex_ai_ip_runtime::content_package_schema()?)?,
    )?;
    let prompt_path = generated_dir.join("root-prompt.txt");
    write_owner_only_new(&prompt_path, codex_ai_ip_runtime::root_prompt().as_bytes())?;
    let canonical_eval_tree = canonical_private_root.clone();
    let thread_projection = build_thread_start(
        &args.model_label,
        "ai-ip-proof-broker",
        &canonical_eval_tree,
    )?;
    let thread_path = generated_dir.join("thread-start-request.json");
    write_owner_only_new(
        &thread_path,
        &serde_json::to_vec_pretty(&thread_projection)?,
    )?;
    let turn_projection = build_turn_start("00000000-0000-7000-8000-000000000000", &mission_case)?;
    let turn_path = generated_dir.join("turn-start-request.json");
    write_owner_only_new(&turn_path, &serde_json::to_vec_pretty(&turn_projection)?)?;
    let evaluator_binary = evaluator_binary_for_freeze(&generated_dir)?;
    let broker_source = repo_root.join("codex-rs/responses-api-proxy/src/broker.rs");
    let named = BTreeMap::from([
        ("source".to_string(), imported_paths["source"].clone()),
        ("materials".to_string(), imported_paths["materials"].clone()),
        ("codexBinary".to_string(), args.codex_bin),
        ("evaluatorBinary".to_string(), evaluator_binary),
        ("brokerSource".to_string(), broker_source),
        ("schema".to_string(), schema_path),
        ("prompt".to_string(), prompt_path),
        ("skill".to_string(), imported_paths["skill"].clone()),
        ("threadStartRequest".to_string(), thread_path),
        ("turnStartRequest".to_string(), turn_path),
        (
            "providerBudgetReceipt".to_string(),
            imported_paths["providerBudgetReceipt"].clone(),
        ),
        ("rateCard".to_string(), imported_paths["rateCard"].clone()),
        (
            "billingPolicy".to_string(),
            imported_paths["billingPolicy"].clone(),
        ),
        ("fxPolicy".to_string(), imported_paths["fxPolicy"].clone()),
    ]);
    let frozen_artifacts = ArtifactCommitments::freeze(named)?;
    let mut artifacts = BTreeMap::new();
    for (name, artifact) in frozen_artifacts.artifacts {
        artifacts.insert(
            name,
            FrozenArtifactReference {
                path: artifact.canonical_path,
                sha256: artifact.sha256,
            },
        );
    }
    let pair_material = format!("{}:{}", args.fork_sha, args.model_label);
    let context = FrozenRunContext {
        schema_version: 1,
        execution_mode: "live".to_string(),
        provider_mode: "not-run".to_string(),
        pair_id: sha256(pair_material.as_bytes()),
        public_run_id: sha256(args.provider_label.as_bytes()),
        candidate_sha: args.fork_sha.clone(),
        private_root: canonical_private_root,
        repo_root,
        repo_head: args.fork_sha,
        provider_upstream_url: args.provider_upstream_url,
        model_label: args.model_label,
        max_output_tokens: args.max_output_tokens_per_request,
        max_attempts_per_arm: args.max_provider_request_attempts_per_run,
        max_total_tokens: args.max_total_tokens_per_run,
        max_elapsed_seconds: args.max_elapsed_seconds_per_run,
        artifacts,
    };
    validate_frozen_context(&context)?;
    write_owner_only_new(&args.output, &serde_json::to_vec_pretty(&context)?)
}

struct RuntimeInspector;

impl codex_responses_api_proxy::RequestInspector for RuntimeInspector {
    fn inspect(
        &self,
        body: &serde_json::Value,
    ) -> Result<codex_responses_api_proxy::TransformedRequestEvidence> {
        let digest: [u8; 32] = Sha256::digest(serde_json::to_vec(body)?).into();
        Ok(codex_responses_api_proxy::TransformedRequestEvidence {
            raw_sha256: digest,
            normalized_sha256: digest,
            normalized_base_commitment: digest,
            treatment_diff_commitment: None,
        })
    }
}

/// Executes exactly two arms against the frozen loopback mock upstream.
pub fn run_local_mock_pair(path: &Path) -> Result<()> {
    // Binding is deliberately first. `bind` creates the loopback listener but
    // cannot accept or forward until the fully validated gate is activated.
    let placeholder_config = codex_responses_api_proxy::ProxyConfig {
        listen_port: None,
        upstream_url: reqwest::Url::parse("http://127.0.0.1:1/v1/responses")?,
        dump_dir: None,
        http_shutdown: false,
        default_request_timeout: None,
        request_transform: None,
    };
    let bound = codex_responses_api_proxy::bind(&placeholder_config)?;
    let broker_port = bound.addr().port();
    let frozen = verify_frozen_context(path)?;
    validate_local_mock_upstream(&frozen.context.provider_upstream_url)?;
    let pair_duration = Duration::from_secs(frozen.context.max_elapsed_seconds);
    let started_at = chrono::Utc::now();
    let pair_deadline = Instant::now()
        .checked_add(pair_duration)
        .context("pair deadline overflow")?;
    let frozen_path = std::env::var("PATH").context("pinned App Server PATH is unavailable")?;
    let mut guard = frozen.execution_guard()?;
    let coordinator_dir = frozen.context.private_root.join("coordinator");
    let skill_bytes = frozen.artifact_bytes("skill")?;
    let runtime = Arc::new(BrokerRuntimeConfig::with_run_limits(
        frozen.max_attempts_per_arm(),
        frozen.max_output_tokens(),
        1024 * 1024,
        frozen.context.max_total_tokens,
    )?);
    let proxy_config = codex_responses_api_proxy::ProxyConfig {
        listen_port: None,
        upstream_url: reqwest::Url::parse(frozen.provider_upstream_url())?,
        dump_dir: None,
        http_shutdown: false,
        default_request_timeout: None,
        request_transform: Some(runtime.request_transform(Arc::new(RuntimeInspector))),
    };
    let shared_config = build_shared_config(&frozen.context.model_label, broker_port)?;
    let homes = prepare_isolated_homes(
        &frozen.context.private_root,
        &shared_config.bytes,
        "candidate-skill",
        &skill_bytes,
    )?;
    verify_isolated_home_parity(&homes, "candidate-skill", &skill_bytes)?;
    let mut generated = GeneratedExecutionArtifacts::new(&homes)?;
    guard.advance(ExecutionBoundary::ContextFrozen)?;
    generated.advance(ExecutionBoundary::ContextFrozen, &coordinator_dir)?;

    let order = commit_arm_order(&frozen, &coordinator_dir)?;
    let first = order.first();
    let second = order.second();
    let deadline = started_at
        .checked_add_signed(chrono::Duration::seconds(i64::try_from(
            frozen.context.max_elapsed_seconds,
        )?))
        .context("pair deadline is out of range")?;
    let execution_bytes = serde_json::to_vec_pretty(&serde_json::json!({
        "schemaVersion": 1,
        "providerMode": "not-run",
        "frozenRunContextSha256": frozen.sha256(),
        "armOrderCommitment": order.seed_commitment(),
        "firstCondition": first,
        "secondCondition": second,
        "startedAt": started_at.to_rfc3339(),
        "deadline": deadline.to_rfc3339(),
        "broker": {"host": "127.0.0.1", "port": broker_port, "path": "/v1/responses"},
        "modelLabel": frozen.context.model_label,
        "maxOutputTokensPerRequest": frozen.context.max_output_tokens,
        "maxProviderRequestAttemptsPerRun": frozen.context.max_attempts_per_arm,
        "maxTotalTokensPerRun": frozen.context.max_total_tokens,
        "maxElapsedSecondsPerRun": frozen.context.max_elapsed_seconds,
        "genericHome": homes.generic_home,
        "genericCodexHome": homes.generic_codex_home,
        "candidateHome": homes.candidate_home,
        "candidateCodexHome": homes.candidate_codex_home,
        "sharedConfigSha256": sha256(&shared_config.bytes),
        "pathSha256": sha256(frozen_path.as_bytes()),
    }))?;
    let execution_context_sha256 = sha256(&execution_bytes);
    write_owner_only_new(
        &coordinator_dir.join("execution-context.json"),
        &execution_bytes,
    )?;
    let gate = Arc::new(PairCoordinator::create(BrokerGateConfig {
        ledger_path: coordinator_dir.join("attempt-index.jsonl"),
        receipt_dir: coordinator_dir.join("receipts"),
        pair_id: frozen.pair_id().to_string(),
        frozen_run_context_sha256: frozen.sha256().to_string(),
        execution_context_sha256,
        arm_order_commitment: order.seed_commitment().to_string(),
        runtime,
    })?);
    poison_on_error(&gate, "commit arm order", gate.commit_order(order))?;
    let order_guard = guard.advance(ExecutionBoundary::OrderCommitted);
    poison_on_error(&gate, "rehash after arm-order commitment", order_guard)?;
    let generated_order = generated.advance(ExecutionBoundary::OrderCommitted, &coordinator_dir);
    poison_on_error(
        &gate,
        "verify generated execution context after arm order",
        generated_order,
    )?;
    let deadline_check = remaining_pair_duration(pair_deadline);
    poison_on_error(
        &gate,
        "check deadline before broker activation",
        deadline_check,
    )?;
    let proxy_result = codex_responses_api_proxy::activate(
        bound,
        proxy_config,
        codex_responses_api_proxy::local_mock_auth_header(),
        gate.clone(),
        gate.clone(),
    );
    let proxy = poison_on_error(&gate, "activate bound proof broker", proxy_result)?;
    let run_result = (|| -> Result<()> {
        let tokio_runtime = tokio::runtime::Runtime::new()?;
        for (index, condition) in [(1_u8, first), (2_u8, second)] {
            let pre = if index == 1 {
                ExecutionBoundary::Arm1Pre
            } else {
                ExecutionBoundary::Arm2Pre
            };
            guard.advance(pre)?;
            generated.advance(pre, &coordinator_dir)?;
            let verified_config_bytes = generated.config_bytes(condition)?;
            tokio_runtime.block_on(run_app_server_arm(
                &frozen,
                &homes,
                &shared_config,
                &gate,
                index,
                condition,
                &coordinator_dir,
                &verified_config_bytes,
                &frozen_path,
                pair_deadline,
                &deadline.to_rfc3339(),
            ))?;
            let post = if index == 1 {
                ExecutionBoundary::Arm1Post
            } else {
                ExecutionBoundary::Arm2Post
            };
            guard.advance(post)?;
            generated.advance(post, &coordinator_dir)?;
            gate.seal_arm()?;
        }
        guard.advance(ExecutionBoundary::Finished)?;
        generated.advance(ExecutionBoundary::Finished, &coordinator_dir)?;
        gate.finish()?;
        Ok(())
    })();
    if let Err(error) = &run_result {
        let _ = gate.poison_permanently(&format!("paired evaluator failed: {error:#}"));
    }
    let shutdown_result = proxy.shutdown_with_timeout(
        remaining_pair_duration(pair_deadline).unwrap_or(Duration::from_millis(1)),
    );
    if let Err(error) = &shutdown_result {
        let _ = gate.poison_permanently(&format!("broker teardown failed: {error:#}"));
    }
    run_result?;
    shutdown_result
}

fn poison_on_error<T>(gate: &PairCoordinator, step: &str, result: Result<T>) -> Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let _ = gate.poison_permanently(&format!("{step} failed: {error:#}"));
            Err(error)
        }
    }
}

async fn run_app_server_arm(
    frozen: &VerifiedFrozenContext,
    homes: &IsolatedHomes,
    shared_config: &crate::FrozenSharedConfig,
    gate: &Arc<PairCoordinator>,
    run_ordinal: u8,
    condition: EvaluationCondition,
    coordinator_dir: &Path,
    verified_config_bytes: &[u8],
    frozen_path: &str,
    pair_deadline: Instant,
    pair_deadline_rfc3339: &str,
) -> Result<()> {
    use codex_app_server_protocol::ApprovalsReviewer;
    use codex_app_server_protocol::AskForApproval;
    use codex_app_server_protocol::ThreadStartResponse;
    use codex_app_server_protocol::TurnStartResponse;
    use codex_app_server_protocol::TurnStatus;

    let (home, codex_home) = match condition {
        EvaluationCondition::Generic => (&homes.generic_home, &homes.generic_codex_home),
        EvaluationCondition::Candidate => (&homes.candidate_home, &homes.candidate_codex_home),
    };
    let temporary = home.join("tmp");
    let environment = ChildEnvironment::from_environment(
        &BTreeMap::from([("PATH".to_string(), frozen_path.to_string())]),
        home.to_str().context("non-UTF-8 isolated Home")?,
        codex_home
            .to_str()
            .context("non-UTF-8 isolated CODEX_HOME")?,
        temporary.to_str().context("non-UTF-8 isolated temp")?,
    )?;
    let codex_binary = frozen
        .artifacts
        .artifacts
        .get("codexBinary")
        .context("missing codex binary commitment")?;
    let retained_executable_descriptor = codex_binary.verified_exec_descriptor()?;
    let mut app_server = AppServerClient::spawn_verified(
        &codex_binary.canonical_path,
        retained_executable_descriptor,
        &coordinator_dir.join(format!("app-server-{run_ordinal}.stderr")),
        &environment,
    )
    .await?;
    let handshake_timeout = remaining_pair_duration(pair_deadline)?;
    let handshake = run_before_deadline(
        pair_deadline,
        app_server.handshake(codex_home, &frozen.context.private_root, handshake_timeout),
    )
    .await?;
    audit_config(
        &handshake.config,
        &handshake.requirements,
        &ConfigAuditExpectation {
            canonical_config_path: codex_home.join("config.toml").canonicalize()?,
            expected_config_bytes: verified_config_bytes.to_vec(),
            expected_layer_config: shared_config.layer_json.clone(),
            expected_effective_config: serde_json::to_value(&handshake.config.config)?,
        },
    )?;
    let thread_params = build_thread_start(
        &frozen.context.model_label,
        "ai-ip-proof-broker",
        &frozen.context.private_root,
    )?;
    if serde_json::to_vec_pretty(&thread_params)? != frozen.artifact_bytes("threadStartRequest")? {
        bail!("runtime thread/start request differs from frozen projection");
    }
    let started: ThreadStartResponse = app_server
        .protocol_mut()?
        .request(
            "thread/start",
            Some(&thread_params),
            remaining_pair_duration(pair_deadline)?,
        )
        .await?;
    if started.model != frozen.context.model_label
        || started.model_provider != "ai-ip-proof-broker"
        || started.cwd.as_path() != frozen.context.private_root
        || started.approval_policy != AskForApproval::Never
        || started.approvals_reviewer != ApprovalsReviewer::User
        || started.service_tier.is_some()
        || started
            .active_permission_profile
            .as_ref()
            .is_none_or(|profile| profile.id != crate::app_server::EVALUATION_PERMISSION_PROFILE)
    {
        bail!("thread/start response differs from the frozen execution controls");
    }
    let mut tree = crate::TreeEventCollector::new(&started.thread)?;
    gate.activate_arm(ArmActivation {
        run_ordinal,
        condition,
        root_thread_id: started.thread.id.clone(),
        deadline: pair_deadline,
        deadline_rfc3339: pair_deadline_rfc3339.to_string(),
    })?;
    let mission_case: codex_ai_ip_domain::HeldOutMissionCase =
        serde_json::from_slice(&frozen.artifact_bytes("source")?)?;
    let turn_params = build_turn_start(&started.thread.id, &mission_case)?;
    let mut normalized_turn = turn_params.clone();
    normalized_turn.thread_id = "00000000-0000-7000-8000-000000000000".to_string();
    if serde_json::to_vec_pretty(&normalized_turn)? != frozen.artifact_bytes("turnStartRequest")? {
        bail!("runtime turn/start request differs from frozen projection");
    }
    let started_turn: TurnStartResponse = app_server
        .protocol_mut()?
        .request(
            "turn/start",
            Some(&turn_params),
            remaining_pair_duration(pair_deadline)?,
        )
        .await?;
    if started_turn.turn.status != TurnStatus::InProgress || !started_turn.turn.items.is_empty() {
        bail!("turn/start response is not a fresh in-progress turn");
    }
    let completed = app_server
        .protocol_mut()?
        .wait_for_turn_completion(
            &started.thread.id,
            &started_turn.turn.id,
            remaining_pair_duration(pair_deadline)?,
            |notification| {
                observe_app_server_lifecycle(gate, &started.thread.id, notification)?;
                tree.ingest(notification.clone())
            },
        )
        .await?;
    if completed.turn.status != TurnStatus::Completed {
        bail!("App Server turn did not complete successfully");
    }
    tree.ingest(codex_app_server_protocol::ServerNotification::TurnCompleted(completed))?;
    let first_scan =
        complete_quiet_tree_scan(app_server.protocol_mut()?, &started.thread, pair_deadline)
            .await?;
    let second_scan =
        complete_quiet_tree_scan(app_server.protocol_mut()?, &started.thread, pair_deadline)
            .await?;
    first_scan.verify_broker_thread_ids(&gate.active_thread_ids()?)?;
    tree.close(
        &first_scan,
        &second_scan,
        &gate.active_completions()?,
        gate.in_flight_count(),
    )?;
    let close_timeout = remaining_pair_duration(pair_deadline)?;
    let status = run_before_deadline(pair_deadline, app_server.close(close_timeout)).await?;
    if !status.success() {
        bail!("pinned App Server exited unsuccessfully");
    }
    Ok(())
}

async fn complete_quiet_tree_scan<R, W>(
    client: &mut crate::JsonLineClient<R, W>,
    root: &codex_app_server_protocol::Thread,
    deadline: Instant,
) -> Result<crate::TreeScan>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use codex_app_server_protocol::ThreadListParams;
    use codex_app_server_protocol::ThreadListResponse;
    use codex_app_server_protocol::ThreadLoadedListParams;
    use codex_app_server_protocol::ThreadLoadedListResponse;
    use codex_app_server_protocol::ThreadReadParams;
    use codex_app_server_protocol::ThreadReadResponse;
    use codex_app_server_protocol::ThreadSourceKind;

    let mut ancestor_pages = Vec::new();
    let mut cursor = None;
    loop {
        if ancestor_pages.len() >= 100 {
            bail!("thread/list pagination exceeded the frozen bound");
        }
        let params = ThreadListParams {
            cursor: cursor.clone(),
            limit: Some(100),
            sort_key: None,
            sort_direction: None,
            model_providers: Some(vec!["ai-ip-proof-broker".to_string()]),
            source_kinds: Some(vec![
                ThreadSourceKind::SubAgent,
                ThreadSourceKind::SubAgentThreadSpawn,
                ThreadSourceKind::SubAgentOther,
            ]),
            archived: Some(false),
            section_id: None,
            project_id: None,
            cwd: None,
            use_state_db_only: false,
            search_term: None,
            parent_thread_id: None,
            ancestor_thread_id: Some(root.id.clone()),
        };
        let page: ThreadListResponse = client
            .request(
                "thread/list",
                Some(&params),
                remaining_pair_duration(deadline)?,
            )
            .await?;
        cursor = page.next_cursor.clone();
        ancestor_pages.push(page);
        if cursor.is_none() {
            break;
        }
    }

    let mut loaded_pages = Vec::new();
    let mut cursor = None;
    loop {
        if loaded_pages.len() >= 100 {
            bail!("thread/loaded/list pagination exceeded the frozen bound");
        }
        let params = ThreadLoadedListParams {
            cursor: cursor.clone(),
            limit: Some(100),
        };
        let page: ThreadLoadedListResponse = client
            .request(
                "thread/loaded/list",
                Some(&params),
                remaining_pair_duration(deadline)?,
            )
            .await?;
        cursor = page.next_cursor.clone();
        loaded_pages.push(page);
        if cursor.is_none() {
            break;
        }
    }

    let mut loaded_reads = Vec::new();
    for thread_id in loaded_pages.iter().flat_map(|page| &page.data) {
        let response: ThreadReadResponse = client
            .request(
                "thread/read",
                Some(&ThreadReadParams {
                    thread_id: thread_id.clone(),
                    include_turns: true,
                }),
                remaining_pair_duration(deadline)?,
            )
            .await?;
        loaded_reads.push(response);
    }
    crate::TreeScan::from_typed_pages(root, &ancestor_pages, &loaded_pages, &loaded_reads)
}

fn remaining_pair_duration(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .context("frozen absolute pair deadline expired")
}

pub(crate) async fn run_before_deadline<T>(
    deadline: Instant,
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::time::timeout(remaining_pair_duration(deadline)?, future)
        .await
        .context("frozen absolute pair deadline expired")?
}

fn observe_app_server_lifecycle(
    gate: &PairCoordinator,
    root_thread_id: &str,
    notification: &codex_app_server_protocol::ServerNotification,
) -> Result<()> {
    use codex_app_server_protocol::ServerNotification;
    use codex_app_server_protocol::ThreadSource;

    let ServerNotification::ThreadStarted(started) = notification else {
        return Ok(());
    };
    if started.thread.id == root_thread_id && started.thread.parent_thread_id.is_none() {
        return Ok(());
    }
    match (
        started.thread.thread_source.as_ref(),
        started.thread.parent_thread_id.as_ref(),
    ) {
        (Some(ThreadSource::GuardianReview), _) => {
            gate.observe_lifecycle(ThreadLifecycle::GuardianReview)
        }
        (Some(ThreadSource::Subagent), Some(parent_thread_id)) => {
            gate.observe_lifecycle(ThreadLifecycle::Subagent {
                thread_id: started.thread.id.clone(),
                parent_thread_id: parent_thread_id.clone(),
            })
        }
        _ => gate.observe_lifecycle(ThreadLifecycle::InvalidThread),
    }
}

fn validate_local_mock_upstream(raw: &str) -> Result<()> {
    let upstream = reqwest::Url::parse(raw).context("parse local mock upstream URL")?;
    if upstream.scheme() != "http"
        || !upstream.host_str().is_some_and(is_loopback_host)
        || upstream.port().is_none_or(|port| port == 0)
        || upstream.path() != "/v1/responses"
        || upstream.query().is_some()
        || upstream.fragment().is_some()
        || !upstream.username().is_empty()
        || upstream.password().is_some()
    {
        bail!("Phase 0A accepts only an explicit-port HTTP loopback /v1/responses upstream");
    }
    Ok(())
}

fn evaluator_binary_for_freeze(_generated_dir: &Path) -> Result<PathBuf> {
    let current = std::env::current_exe()
        .context("resolve evaluator executable")?
        .canonicalize()
        .context("canonicalize evaluator executable")?;
    #[cfg(test)]
    if has_multiple_links(&fs::metadata(&current)?) {
        let copied = _generated_dir.join("evaluator-binary-under-test");
        write_owner_only_new(&copied, &fs::read(&current)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&copied, fs::Permissions::from_mode(0o700))?;
        }
        return copied
            .canonicalize()
            .context("canonicalize copied evaluator test binary");
    }
    Ok(current)
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// Re-reads the private frozen context and returns its bytes commitment.
pub fn verify_frozen_context(path: &Path) -> Result<VerifiedFrozenContext> {
    let supplied_metadata = fs::symlink_metadata(path)
        .with_context(|| format!("stat supplied frozen context {}", path.display()))?;
    if supplied_metadata.file_type().is_symlink() || has_multiple_links(&supplied_metadata) {
        bail!("frozen context path must not be a symlink or hardlink");
    }
    let canonical_path = path
        .canonicalize()
        .with_context(|| format!("canonicalize frozen context {}", path.display()))?;
    let metadata = fs::symlink_metadata(&canonical_path)
        .with_context(|| format!("stat frozen context {}", canonical_path.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        bail!("frozen context must be a regular non-symlink file");
    }
    let frozen_file = ArtifactCommitment::freeze(&canonical_path)?;
    let bytes = frozen_file.read_verified()?;
    let context: FrozenRunContext =
        serde_json::from_slice(&bytes).context("parse strict frozen context JSON")?;
    if canonical_path != context.private_root.join("frozen-run-context.json") {
        bail!("frozen context is outside its canonical private-root location");
    }
    require_owner_only_file(&canonical_path)?;
    validate_frozen_context(&context)?;
    let artifacts = ArtifactCommitments::from_references(&context.artifacts)?;
    Ok(VerifiedFrozenContext {
        canonical_path,
        sha256: sha256(&bytes),
        context,
        frozen_file,
        artifacts,
    })
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CommittedArmOrder {
    first: EvaluationCondition,
    second: EvaluationCondition,
    seed_commitment: String,
    frozen_run_context_sha256: String,
}

impl CommittedArmOrder {
    pub fn first(&self) -> EvaluationCondition {
        self.first
    }

    pub fn second(&self) -> EvaluationCondition {
        self.second
    }

    pub fn seed_commitment(&self) -> &str {
        &self.seed_commitment
    }

    pub fn frozen_run_context_sha256(&self) -> &str {
        &self.frozen_run_context_sha256
    }
}

/// Commits an unbiased arm order using only the operating system CSPRNG.
///
/// The verified frozen context is required by type, so arm order cannot be
/// selected before context freeze. No seed or order is accepted from callers.
pub fn commit_arm_order(
    frozen: &VerifiedFrozenContext,
    coordinator_directory: &Path,
) -> Result<CommittedArmOrder> {
    let bytes = frozen.frozen_file.read_verified()?;
    if sha256(&bytes) != frozen.sha256() {
        bail!("frozen run context changed before arm-order commitment");
    }
    create_owner_only_dir(coordinator_directory)?;
    let mut seed = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut seed)
        .map_err(|error| anyhow!("operating system CSPRNG failed: {error}"))?;
    let path = coordinator_directory.join("arm-order-seed.bin");
    write_owner_only_new(&path, &seed)?;
    let seed_commitment = sha256(&seed);
    let first = if seed[0] & 1 == 0 {
        EvaluationCondition::Generic
    } else {
        EvaluationCondition::Candidate
    };
    let second = match first {
        EvaluationCondition::Generic => EvaluationCondition::Candidate,
        EvaluationCondition::Candidate => EvaluationCondition::Generic,
    };
    Ok(CommittedArmOrder {
        first,
        second,
        seed_commitment,
        frozen_run_context_sha256: frozen.sha256().to_string(),
    })
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct IsolatedHomes {
    pub generic_home: PathBuf,
    pub generic_codex_home: PathBuf,
    pub candidate_home: PathBuf,
    pub candidate_codex_home: PathBuf,
}

/// Creates two owner-only Homes from one shared config byte vector.
///
/// The generic Skill catalog is empty. The candidate catalog differs only by
/// the one supplied `SKILL.md`; no authentication file is created.
pub fn prepare_isolated_homes(
    private_root: &Path,
    shared_config_bytes: &[u8],
    skill_name: &str,
    skill_bytes: &[u8],
) -> Result<IsolatedHomes> {
    validate_leaf_name(skill_name)?;
    create_owner_only_dir(private_root)?;
    let generic_home = private_root.join("generic-home");
    let candidate_home = private_root.join("candidate-home");
    for path in [&generic_home, &candidate_home] {
        if path.exists() {
            bail!("isolated Home already exists: {}", path.display());
        }
    }
    let generic_codex_home = generic_home.join(".codex");
    let candidate_codex_home = candidate_home.join(".codex");
    for path in [
        &generic_home,
        &candidate_home,
        &generic_codex_home,
        &candidate_codex_home,
        &generic_codex_home.join("skills"),
        &candidate_codex_home.join("skills"),
        &generic_home.join("tmp"),
        &candidate_home.join("tmp"),
    ] {
        create_owner_only_dir(path)?;
    }
    write_owner_only_new(&generic_codex_home.join("config.toml"), shared_config_bytes)?;
    write_owner_only_new(
        &candidate_codex_home.join("config.toml"),
        shared_config_bytes,
    )?;
    let candidate_skill = candidate_codex_home.join("skills").join(skill_name);
    create_owner_only_dir(&candidate_skill)?;
    write_owner_only_new(&candidate_skill.join("SKILL.md"), skill_bytes)?;
    Ok(IsolatedHomes {
        generic_home,
        generic_codex_home,
        candidate_home,
        candidate_codex_home,
    })
}

/// Verifies complete relative-tree parity, allowing only the target Skill in
/// the candidate Home.
pub fn verify_isolated_home_parity(
    homes: &IsolatedHomes,
    skill_name: &str,
    expected_skill_bytes: &[u8],
) -> Result<()> {
    validate_leaf_name(skill_name)?;
    let generic = collect_tree(&homes.generic_home)?;
    let candidate = collect_tree(&homes.candidate_home)?;
    let mut expected_candidate = generic;
    expected_candidate.insert(format!(".codex/skills/{skill_name}"), None);
    expected_candidate.insert(
        format!(".codex/skills/{skill_name}/SKILL.md"),
        Some(expected_skill_bytes.to_vec()),
    );
    if candidate != expected_candidate {
        bail!("isolated Home trees differ outside the target Skill");
    }
    Ok(())
}

/// Explicit allowlist applied to an evaluator child after `env_clear()`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ChildEnvironment {
    variables: BTreeMap<String, String>,
}

impl ChildEnvironment {
    pub fn from_environment(
        host: &BTreeMap<String, String>,
        home: &str,
        codex_home: &str,
        temp: &str,
    ) -> Result<Self> {
        let path = host
            .get("PATH")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow!("frozen PATH is required"))?;
        let variables = BTreeMap::from([
            ("PATH".to_string(), path.clone()),
            ("HOME".to_string(), home.to_string()),
            ("CODEX_HOME".to_string(), codex_home.to_string()),
            ("TMP".to_string(), temp.to_string()),
            ("TEMP".to_string(), temp.to_string()),
            ("TMPDIR".to_string(), temp.to_string()),
        ]);
        #[cfg(windows)]
        let variables = {
            let mut variables = variables;
            variables.insert("USERPROFILE".to_string(), home.to_string());
            for name in ["SYSTEMROOT", "WINDIR", "COMSPEC", "PATHEXT"] {
                if let Some(value) = host.get(name) {
                    variables.insert(name.to_string(), value.clone());
                }
            }
            variables
        };
        reject_secret_names(&variables)?;
        Ok(Self { variables })
    }

    pub fn variables(&self) -> &BTreeMap<String, String> {
        &self.variables
    }

    /// Builds a process with no inherited environment and only the allowlist.
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut command = Command::new(program);
        command.env_clear();
        command.envs(&self.variables);
        command
    }

    /// Applies the same clear-then-allowlist policy to a Tokio child command.
    pub fn apply_tokio(&self, command: &mut tokio::process::Command) {
        command.env_clear();
        command.envs(&self.variables);
    }
}

#[derive(Debug, Clone)]
struct ArtifactCommitment {
    canonical_path: PathBuf,
    sha256: String,
    handle: Arc<File>,
}

/// Named byte commitments revalidated at coordinator boundaries.
///
/// Callers use explicit semantic names (source, materials, binaries, config,
/// Schema, prompt, Skill, request projections, receipts) so an error identifies
/// the exact mutable input. Verification always re-opens each canonical path.
#[derive(Debug, Clone)]
pub struct ArtifactCommitments {
    artifacts: BTreeMap<String, ArtifactCommitment>,
}

pub const REQUIRED_EXECUTION_ARTIFACTS: &[&str] = &[
    "source",
    "materials",
    "codexBinary",
    "evaluatorBinary",
    "brokerSource",
    "schema",
    "prompt",
    "skill",
    "threadStartRequest",
    "turnStartRequest",
    "providerBudgetReceipt",
    "rateCard",
    "billingPolicy",
    "fxPolicy",
];

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ExecutionBoundary {
    ContextFrozen,
    OrderCommitted,
    Arm1Pre,
    Arm1Post,
    Arm2Pre,
    Arm2Post,
    Finished,
}

struct GeneratedExecutionArtifacts {
    immutable: BTreeMap<String, ArtifactCommitment>,
}

impl GeneratedExecutionArtifacts {
    fn new(homes: &IsolatedHomes) -> Result<Self> {
        let generic = ArtifactCommitment::freeze(&homes.generic_codex_home.join("config.toml"))?;
        let candidate =
            ArtifactCommitment::freeze(&homes.candidate_codex_home.join("config.toml"))?;
        if generic.sha256 != candidate.sha256 {
            bail!("isolated generated config commitments differ");
        }
        Ok(Self {
            immutable: BTreeMap::from([
                ("genericConfig".to_string(), generic),
                ("candidateConfig".to_string(), candidate),
            ]),
        })
    }

    fn advance(&mut self, boundary: ExecutionBoundary, coordinator_dir: &Path) -> Result<()> {
        for (name, artifact) in &self.immutable {
            artifact
                .read_verified()
                .with_context(|| format!("verify generated {name}"))?;
        }
        if boundary != ExecutionBoundary::ContextFrozen {
            self.capture_once(
                "executionContext",
                &coordinator_dir.join("execution-context.json"),
            )?;
        }
        Ok(())
    }

    fn config_bytes(&self, condition: EvaluationCondition) -> Result<Vec<u8>> {
        let name = match condition {
            EvaluationCondition::Generic => "genericConfig",
            EvaluationCondition::Candidate => "candidateConfig",
        };
        self.immutable
            .get(name)
            .context("missing generated config commitment")?
            .read_verified()
    }

    fn capture_once(&mut self, name: &str, path: &Path) -> Result<()> {
        if let Some(existing) = self.immutable.get(name) {
            existing.read_verified()?;
        } else {
            self.immutable
                .insert(name.to_string(), ArtifactCommitment::freeze(path)?);
        }
        Ok(())
    }
}

/// Coordinator-owned immutable inputs and the only valid rehash sequence.
pub struct FrozenExecutionGuard {
    artifacts: ArtifactCommitments,
    worktree: GitWorktreeCommitment,
    next: usize,
}

impl FrozenExecutionGuard {
    pub fn create(paths: BTreeMap<String, PathBuf>, repository: &Path) -> Result<Self> {
        require_artifact_names(paths.keys().map(String::as_str))?;
        Ok(Self {
            artifacts: ArtifactCommitments::freeze(paths)?,
            worktree: GitWorktreeCommitment::freeze(repository)?,
            next: 0,
        })
    }

    fn from_commitments(artifacts: ArtifactCommitments, repository: &Path) -> Result<Self> {
        require_artifact_names(artifacts.artifacts.keys().map(String::as_str))?;
        Ok(Self {
            artifacts,
            worktree: GitWorktreeCommitment::freeze(repository)?,
            next: 0,
        })
    }

    pub fn advance(&mut self, boundary: ExecutionBoundary) -> Result<()> {
        const ORDER: [ExecutionBoundary; 7] = [
            ExecutionBoundary::ContextFrozen,
            ExecutionBoundary::OrderCommitted,
            ExecutionBoundary::Arm1Pre,
            ExecutionBoundary::Arm1Post,
            ExecutionBoundary::Arm2Pre,
            ExecutionBoundary::Arm2Post,
            ExecutionBoundary::Finished,
        ];
        if ORDER.get(self.next) != Some(&boundary) {
            bail!("execution rehash boundary is skipped, duplicated, or out of order");
        }
        self.artifacts.verify()?;
        self.worktree.verify()?;
        self.next += 1;
        Ok(())
    }
}

/// Exact Git HEAD and cleanliness commitment for a frozen source worktree.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GitWorktreeCommitment {
    canonical_root: PathBuf,
    head: String,
}

impl GitWorktreeCommitment {
    pub fn freeze(repository: &Path) -> Result<Self> {
        let canonical_root = repository
            .canonicalize()
            .with_context(|| format!("canonicalize worktree {}", repository.display()))?;
        let head = git_stdout(&canonical_root, &["rev-parse", "HEAD"])?;
        let status = git_stdout(
            &canonical_root,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?;
        if !status.is_empty() {
            bail!("frozen worktree is not clean");
        }
        Ok(Self {
            canonical_root,
            head,
        })
    }

    pub fn verify(&self) -> Result<()> {
        let metadata =
            fs::symlink_metadata(&self.canonical_root).context("re-stat frozen worktree")?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            bail!("frozen worktree root type changed");
        }
        let head = git_stdout(&self.canonical_root, &["rev-parse", "HEAD"])?;
        if head != self.head {
            bail!("frozen worktree HEAD changed");
        }
        let status = git_stdout(
            &self.canonical_root,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?;
        if !status.is_empty() {
            bail!("frozen worktree is not clean");
        }
        Ok(())
    }

    pub fn head(&self) -> &str {
        &self.head
    }
}

impl ArtifactCommitments {
    pub fn freeze(paths: BTreeMap<String, PathBuf>) -> Result<Self> {
        if paths.is_empty() {
            bail!("at least one artifact commitment is required");
        }
        let mut artifacts = BTreeMap::new();
        for (name, path) in paths {
            if name.is_empty() {
                bail!("artifact commitment name is empty");
            }
            let supplied_metadata = fs::symlink_metadata(&path)
                .with_context(|| format!("stat supplied {name} artifact {}", path.display()))?;
            if supplied_metadata.file_type().is_symlink() || has_multiple_links(&supplied_metadata)
            {
                bail!("{name} artifact path is a symlink or hardlink");
            }
            let canonical_path = path
                .canonicalize()
                .with_context(|| format!("canonicalize {name} artifact {}", path.display()))?;
            artifacts.insert(name, ArtifactCommitment::freeze(&canonical_path)?);
        }
        Ok(Self { artifacts })
    }

    fn from_references(references: &BTreeMap<String, FrozenArtifactReference>) -> Result<Self> {
        let mut artifacts = BTreeMap::new();
        for (name, reference) in references {
            let canonical_path = reference
                .path
                .canonicalize()
                .with_context(|| format!("canonicalize frozen {name} artifact"))?;
            if canonical_path != reference.path {
                bail!("frozen {name} artifact path is not canonical");
            }
            let artifact = ArtifactCommitment::freeze(&reference.path)
                .with_context(|| format!("open frozen {name} artifact"))?;
            if artifact.sha256 != reference.sha256 {
                bail!("frozen {name} artifact commitment mismatch");
            }
            artifacts.insert(name.clone(), artifact);
        }
        Ok(Self { artifacts })
    }

    pub fn verify(&self) -> Result<()> {
        for (name, frozen) in &self.artifacts {
            frozen
                .read_verified()
                .with_context(|| format!("re-read {name} artifact"))?;
        }
        Ok(())
    }

    pub fn read_verified(&self, name: &str) -> Result<Vec<u8>> {
        self.artifacts
            .get(name)
            .ok_or_else(|| anyhow!("missing frozen artifact {name}"))?
            .read_verified()
    }

    pub fn sha256(&self, name: &str) -> Option<&str> {
        self.artifacts
            .get(name)
            .map(|artifact| artifact.sha256.as_str())
    }
}

impl ArtifactCommitment {
    fn freeze(path: &Path) -> Result<Self> {
        let canonical_path = path
            .canonicalize()
            .with_context(|| format!("canonicalize artifact {}", path.display()))?;
        let handle = Arc::new(open_anchored_regular(&canonical_path)?);
        let bytes = read_handle(&handle)?;
        Ok(Self {
            canonical_path,
            sha256: sha256(&bytes),
            handle,
        })
    }

    fn read_verified(&self) -> Result<Vec<u8>> {
        let current = open_anchored_regular(&self.canonical_path)?;
        if !same_file(&self.handle.metadata()?, &current.metadata()?) {
            bail!("artifact path identity changed after freeze");
        }
        let bytes = read_handle(&self.handle)?;
        if sha256(&bytes) != self.sha256 {
            bail!("artifact bytes changed after freeze");
        }
        Ok(bytes)
    }

    #[cfg(unix)]
    fn verified_exec_descriptor(&self) -> Result<File> {
        self.read_verified()?;
        self.handle
            .try_clone()
            .context("duplicate verified executable descriptor")
    }

    #[cfg(not(unix))]
    fn verified_exec_descriptor(&self) -> Result<File> {
        bail!("descriptor-backed executable launch requires Unix fexec semantics")
    }
}

#[cfg(unix)]
fn has_multiple_links(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    metadata.nlink() != 1
}

#[cfg(not(unix))]
fn has_multiple_links(_metadata: &fs::Metadata) -> bool {
    false
}

fn reject_secret_names(variables: &BTreeMap<String, String>) -> Result<()> {
    for name in variables.keys() {
        let upper = name.to_ascii_uppercase();
        if upper.contains("KEY")
            || upper.contains("TOKEN")
            || upper.contains("SECRET")
            || upper.contains("PROXY")
            || upper.starts_with("AWS_")
            || upper.starts_with("AZURE_")
            || upper.starts_with("GOOGLE_")
        {
            bail!("secret or proxy environment variable is forbidden: {name}");
        }
    }
    Ok(())
}

fn git_stdout(repository: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .with_context(|| format!("run git in {}", repository.display()))?;
    if !output.status.success() {
        bail!("git command failed in frozen worktree");
    }
    String::from_utf8(output.stdout)
        .context("git output is not UTF-8")
        .map(|output| output.trim_end_matches(['\r', '\n']).to_string())
}

fn validate_leaf_name(name: &str) -> Result<()> {
    let path = Path::new(name);
    if name.is_empty()
        || path.components().count() != 1
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
    {
        bail!("Skill name must be one safe path component");
    }
    Ok(())
}

fn require_artifact_names<'a>(names: impl Iterator<Item = &'a str>) -> Result<()> {
    let names: std::collections::HashSet<&str> = names.collect();
    if names.len() != REQUIRED_EXECUTION_ARTIFACTS.len()
        || REQUIRED_EXECUTION_ARTIFACTS
            .iter()
            .any(|required| !names.contains(required))
    {
        bail!("frozen execution artifact set is incomplete or has extras");
    }
    Ok(())
}

fn validate_frozen_context(context: &FrozenRunContext) -> Result<()> {
    if context.schema_version != 1
        || context.execution_mode != "live"
        || context.provider_mode != "not-run"
        || !is_lower_hex(&context.pair_id, 64)
        || !is_lower_hex(&context.public_run_id, 64)
        || context.candidate_sha.is_empty()
        || context.max_output_tokens == 0
        || context.max_attempts_per_arm == 0
        || context.max_total_tokens == 0
        || context.max_elapsed_seconds == 0
        || context.model_label.is_empty()
    {
        bail!("strict live frozen context has invalid required fields");
    }
    if context.candidate_sha != context.repo_head {
        bail!("candidate SHA must equal the frozen repository HEAD");
    }
    validate_local_mock_upstream(&context.provider_upstream_url)?;
    require_artifact_names(context.artifacts.keys().map(String::as_str))?;
    let current_exe = std::env::current_exe()
        .context("resolve current evaluator executable")?
        .canonicalize()
        .context("canonicalize current evaluator executable")?;
    let evaluator_matches = context.artifacts["evaluatorBinary"].path == current_exe;
    #[cfg(test)]
    let evaluator_matches = evaluator_matches
        || (context.artifacts["evaluatorBinary"].path
            == context
                .private_root
                .join("frozen-inputs/evaluator-binary-under-test")
            && context.artifacts["evaluatorBinary"].sha256 == sha256(&fs::read(&current_exe)?));
    if !evaluator_matches {
        bail!("frozen evaluator binary is not the running evaluator");
    }
    let expected_broker = context
        .repo_root
        .join("codex-rs/responses-api-proxy/src/broker.rs")
        .canonicalize()
        .context("canonicalize frozen broker source")?;
    if context.artifacts["brokerSource"].path != expected_broker {
        bail!("frozen broker source is not the committed broker component");
    }
    let canonical_repo = context
        .repo_root
        .canonicalize()
        .context("canonicalize frozen repository root")?;
    if canonical_repo != context.repo_root {
        bail!("frozen repository root path is not canonical");
    }
    let worktree = GitWorktreeCommitment::freeze(&context.repo_root)?;
    if worktree.head() != context.repo_head {
        bail!("frozen worktree HEAD does not match context");
    }
    let canonical_private_root = context
        .private_root
        .canonicalize()
        .context("canonicalize private root")?;
    if canonical_private_root != context.private_root {
        bail!("private root path must be canonical");
    }
    let private_metadata = fs::symlink_metadata(&context.private_root)?;
    if !private_metadata.is_dir() || private_metadata.file_type().is_symlink() {
        bail!("private root must be a non-symlink directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if private_metadata.permissions().mode() & 0o077 != 0 {
            bail!("private root must be owner-only");
        }
    }
    let frozen_inputs = context.private_root.join("frozen-inputs");
    require_owner_only_directory(&frozen_inputs)?;
    let expected_private_inputs = [
        ("source", "source.json"),
        ("materials", "materials.json"),
        ("providerBudgetReceipt", "provider-budget-receipt.json"),
        ("rateCard", "rate-card.json"),
        ("billingPolicy", "billing-policy.json"),
        ("fxPolicy", "fx-policy.json"),
        ("skill", "lead-skill.md"),
        ("schema", "content-package-schema.json"),
        ("prompt", "root-prompt.txt"),
        ("threadStartRequest", "thread-start-request.json"),
        ("turnStartRequest", "turn-start-request.json"),
    ];
    for (name, leaf) in expected_private_inputs {
        let expected = frozen_inputs
            .join(leaf)
            .canonicalize()
            .with_context(|| format!("canonicalize private frozen input {name}"))?;
        if context.artifacts[name].path != expected {
            bail!("frozen {name} reference is outside its canonical private input slot");
        }
        require_owner_only_file(&expected)?;
    }
    Ok(())
}

fn require_owner_only_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || has_multiple_links(&metadata)
    {
        bail!(
            "private input must be a single-link regular file: {}",
            path.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!("private input must be owner-only: {}", path.display());
        }
    }
    Ok(())
}

fn require_owner_only_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        bail!("private input root must be a non-symlink directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!("private input root must be owner-only");
        }
    }
    Ok(())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn collect_tree(root: &Path) -> Result<BTreeMap<String, Option<Vec<u8>>>> {
    fn walk(
        root: &Path,
        current: &Path,
        output: &mut BTreeMap<String, Option<Vec<u8>>>,
    ) -> Result<()> {
        for entry in fs::read_dir(current)
            .with_context(|| format!("read isolated Home directory {}", current.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            let relative = path
                .strip_prefix(root)
                .context("isolated Home entry escaped root")?
                .to_string_lossy()
                .replace('\\', "/");
            if metadata.file_type().is_symlink()
                || (metadata.is_file() && has_multiple_links(&metadata))
            {
                bail!("isolated Home contains a link: {relative}");
            }
            if metadata.is_dir() {
                output.insert(relative, None);
                walk(root, &path, output)?;
            } else if metadata.is_file() {
                output.insert(relative, Some(read_regular_file_no_follow(&path)?));
            } else {
                bail!("isolated Home contains a non-regular entry");
            }
        }
        Ok(())
    }

    let mut output = BTreeMap::new();
    walk(root, root, &mut output)?;
    Ok(output)
}

fn read_regular_file_no_follow(path: &Path) -> Result<Vec<u8>> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalize regular file {}", path.display()))?;
    read_handle(&open_anchored_regular(&canonical)?)
}

#[cfg(unix)]
fn open_anchored_regular(path: &Path) -> Result<File> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::fd::FromRawFd;
    use std::path::Component;

    if !path.is_absolute() {
        bail!("anchored artifact path must be absolute");
    }
    let root = CString::new("/")?;
    let root_fd = unsafe {
        libc::open(
            root.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    };
    if root_fd < 0 {
        return Err(std::io::Error::last_os_error()).context("open filesystem root");
    }
    let mut current = unsafe { File::from_raw_fd(root_fd) };
    let components: Vec<_> = path.components().collect();
    let normal_count = components
        .iter()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count();
    let mut seen = 0_usize;
    for component in components {
        match component {
            Component::RootDir => continue,
            Component::Normal(name) => {
                seen += 1;
                let name = CString::new(name.as_encoded_bytes())?;
                let final_component = seen == normal_count;
                let flags = libc::O_RDONLY
                    | libc::O_CLOEXEC
                    | libc::O_NOFOLLOW
                    | if final_component {
                        0
                    } else {
                        libc::O_DIRECTORY
                    };
                let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
                if fd < 0 {
                    return Err(std::io::Error::last_os_error())
                        .with_context(|| format!("open anchored component in {}", path.display()));
                }
                current = unsafe { File::from_raw_fd(fd) };
            }
            _ => bail!("artifact path contains unsupported components"),
        }
    }
    let metadata = current.metadata()?;
    if !metadata.is_file() || has_multiple_links(&metadata) {
        bail!("anchored artifact is not a single-link regular file");
    }
    Ok(current)
}

#[cfg(not(unix))]
fn open_anchored_regular(_path: &Path) -> Result<File> {
    bail!("live artifact access requires descriptor-anchored Unix openat support")
}

#[cfg(unix)]
fn read_handle(file: &File) -> Result<Vec<u8>> {
    use std::os::unix::fs::FileExt;

    let before = file.metadata()?;
    let mut bytes = Vec::new();
    let mut offset = 0_u64;
    loop {
        let mut chunk = [0_u8; 8192];
        let read = file.read_at(&mut chunk, offset)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
        offset = offset
            .checked_add(u64::try_from(read)?)
            .context("artifact length overflow")?;
    }
    let after = file.metadata()?;
    if !same_file(&before, &after) || before.len() != after.len() || after.len() != offset {
        bail!("artifact changed while reading verified handle");
    }
    Ok(bytes)
}

#[cfg(not(unix))]
fn read_handle(_file: &File) -> Result<Vec<u8>> {
    bail!("live artifact access requires descriptor-anchored Unix reads")
}

#[cfg(unix)]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_file(_left: &fs::Metadata, _right: &fs::Metadata) -> bool {
    false
}

fn create_owner_only_dir(path: &Path) -> Result<()> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)
            .with_context(|| format!("stat directory {}", path.display()))?;
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            bail!("path is not a non-symlink directory: {}", path.display());
        }
    } else {
        fs::create_dir(path).with_context(|| format!("create directory {}", path.display()))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("set owner-only permissions on {}", path.display()))?;
    }
    Ok(())
}

fn write_owner_only_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("create private file {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write private file {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("fsync private file {}", path.display()))
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
