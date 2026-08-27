use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use anyhow::Context;
use anyhow::bail;
use codex_ai_ip_domain::ContentPackage;
use codex_ai_ip_domain::HeldOutMissionCase;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadSource;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::TokenUsageBreakdown;
use codex_app_server_protocol::TurnItemsView;
use codex_app_server_protocol::TurnStatus;
use codex_responses_api_proxy::ObservedUsage;
use codex_responses_api_proxy::ResponseCompletedMetadata;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use crate::ExecutionMode;
use crate::Usage;

pub fn validate_case_boundary(
    repository_root: &Path,
    case_path: &Path,
    mode: ExecutionMode,
) -> anyhow::Result<()> {
    let repository_root = repository_root
        .canonicalize()
        .context("canonicalize case-boundary repository")?;
    let case_path = case_path
        .canonicalize()
        .context("canonicalize case fixture")?;
    let relative = case_path.strip_prefix(&repository_root).ok();
    let tracked = if let Some(relative) = relative {
        let output = Command::new("git")
            .arg("-C")
            .arg(&repository_root)
            .arg("ls-files")
            .arg("--error-unmatch")
            .arg("--")
            .arg(relative)
            .output()
            .context("query Git case boundary")?;
        output.status.success()
    } else {
        false
    };
    match mode {
        ExecutionMode::Replay if !tracked => {
            bail!("replay case must be a committed fixture in the frozen repository")
        }
        ExecutionMode::Live if tracked || relative.is_some() => {
            bail!("live private source case must remain outside the frozen repository")
        }
        ExecutionMode::Replay | ExecutionMode::Live => Ok(()),
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CollectedReplay {
    pub usage: Usage,
    pub raw_response_count: u64,
    pub content_package: ContentPackage,
}

pub struct ReplayCollector {
    root_thread_id: String,
    root_turn_id: String,
    known_threads: HashSet<String>,
    raw_responses: HashMap<(String, String, String), TokenUsageBreakdown>,
    item_final: Option<(String, String)>,
    terminal_final: Option<(String, String)>,
}

impl ReplayCollector {
    pub fn new(
        root_thread_id: impl Into<String>,
        root_turn_id: impl Into<String>,
        known_threads: HashSet<String>,
    ) -> Self {
        Self {
            root_thread_id: root_thread_id.into(),
            root_turn_id: root_turn_id.into(),
            known_threads,
            raw_responses: HashMap::new(),
            item_final: None,
            terminal_final: None,
        }
    }

    pub fn ingest(&mut self, notification: ServerNotification) -> anyhow::Result<()> {
        match notification {
            ServerNotification::RawResponseCompleted(notification) => {
                self.ensure_known_thread(&notification.thread_id)?;
                let usage = notification.usage.context("missing usage")?;
                let key = (
                    notification.thread_id,
                    notification.turn_id,
                    notification.response_id,
                );
                if let Some(previous) = self.raw_responses.get(&key) {
                    if previous == &usage {
                        return Ok(());
                    }
                    bail!("conflicting duplicate raw response usage");
                }
                validate_usage(&usage)?;
                self.raw_responses.insert(key, usage);
            }
            ServerNotification::ItemCompleted(notification) => {
                self.ensure_known_thread(&notification.thread_id)?;
                if notification.thread_id != self.root_thread_id
                    || notification.turn_id != self.root_turn_id
                {
                    return Ok(());
                }
                if let ThreadItem::AgentMessage { id, text, .. } = notification.item
                    && !text.is_empty()
                {
                    if self.item_final.is_some() {
                        bail!("multiple root final messages");
                    }
                    self.item_final = Some((id, text));
                }
            }
            ServerNotification::TurnCompleted(notification) => {
                self.ensure_known_thread(&notification.thread_id)?;
                if notification.thread_id != self.root_thread_id {
                    return Ok(());
                }
                if notification.turn.id != self.root_turn_id {
                    bail!("unexpected root turn terminal");
                }
                if notification.turn.status != TurnStatus::Completed {
                    bail!("root turn did not complete successfully");
                }
                if notification.turn.items_view != TurnItemsView::Full {
                    bail!("root terminal does not contain the full item list");
                }
                if self.terminal_final.is_some() {
                    bail!("multiple root terminal notifications");
                }
                let mut finals = notification.turn.items.into_iter().filter_map(|item| {
                    if let ThreadItem::AgentMessage { id, text, .. } = item
                        && !text.is_empty()
                    {
                        return Some((id, text));
                    }
                    None
                });
                let final_message = finals
                    .next()
                    .context("root terminal has no final message")?;
                if finals.next().is_some() {
                    bail!("multiple root final messages in terminal");
                }
                self.terminal_final = Some(final_message);
            }
            _ => {}
        }
        Ok(())
    }

    pub fn finish(self, mission_case: &HeldOutMissionCase) -> anyhow::Result<CollectedReplay> {
        if self.raw_responses.is_empty() {
            bail!("missing raw response completion");
        }
        let (item_id, item_text) = self.item_final.context("missing root final item")?;
        let (terminal_id, terminal_text) = self
            .terminal_final
            .context("missing root terminal notification")?;
        if item_id != terminal_id || item_text.as_bytes() != terminal_text.as_bytes() {
            bail!("item and terminal final messages are not byte-identical");
        }

        let content_package: ContentPackage =
            serde_json::from_str(&item_text).context("root final is not a ContentPackage")?;
        content_package
            .validate_against(mission_case)
            .map_err(|error| anyhow::anyhow!("invalid ContentPackage: {error}"))?;

        let usage = self
            .raw_responses
            .values()
            .try_fold(Usage::default(), add_usage)?;
        let raw_response_count = u64::try_from(self.raw_responses.len())
            .context("raw response count does not fit u64")?;

        Ok(CollectedReplay {
            usage,
            raw_response_count,
            content_package,
        })
    }

    fn ensure_known_thread(&self, thread_id: &str) -> anyhow::Result<()> {
        if !self.known_threads.contains(thread_id) {
            bail!("notification references unknown thread {thread_id}");
        }
        Ok(())
    }
}

fn validate_usage(usage: &TokenUsageBreakdown) -> anyhow::Result<()> {
    let values = [
        usage.total_tokens,
        usage.input_tokens,
        usage.cached_input_tokens,
        usage.cache_write_input_tokens,
        usage.output_tokens,
        usage.reasoning_output_tokens,
    ];
    if values.into_iter().any(|value| value < 0) {
        bail!("negative token usage");
    }
    let expected_total = usage
        .input_tokens
        .checked_add(usage.output_tokens)
        .context("totalTokens overflow")?;
    if usage.total_tokens != expected_total {
        bail!("totalTokens must equal inputTokens plus outputTokens");
    }
    let cached_total = usage
        .cached_input_tokens
        .checked_add(usage.cache_write_input_tokens)
        .context("cached token usage overflow")?;
    if cached_total > usage.input_tokens {
        bail!("cached token usage exceeds inputTokens");
    }
    if usage.reasoning_output_tokens > usage.output_tokens {
        bail!("reasoning token usage exceeds outputTokens");
    }
    Ok(())
}

fn add_usage(mut aggregate: Usage, next: &TokenUsageBreakdown) -> anyhow::Result<Usage> {
    aggregate.total_tokens = checked_add(aggregate.total_tokens, next.total_tokens)?;
    aggregate.input_tokens = checked_add(aggregate.input_tokens, next.input_tokens)?;
    aggregate.cached_input_tokens =
        checked_add(aggregate.cached_input_tokens, next.cached_input_tokens)?;
    aggregate.cache_write_input_tokens = checked_add(
        aggregate.cache_write_input_tokens,
        next.cache_write_input_tokens,
    )?;
    aggregate.output_tokens = checked_add(aggregate.output_tokens, next.output_tokens)?;
    aggregate.reasoning_output_tokens = checked_add(
        aggregate.reasoning_output_tokens,
        next.reasoning_output_tokens,
    )?;
    Ok(aggregate)
}

fn checked_add(left: i64, right: i64) -> anyhow::Result<i64> {
    left.checked_add(right).context("token usage overflow")
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadRecord {
    id: String,
    parent_thread_id: Option<String>,
    session_id: String,
    status: SnapshotStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
enum SnapshotStatus {
    NotLoaded,
    Idle,
    Active,
    SystemError,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TreeScan {
    root_thread_id: String,
    session_id: String,
    records: BTreeMap<String, ThreadRecord>,
}

impl TreeScan {
    pub fn from_typed_pages(
        root: &codex_app_server_protocol::Thread,
        ancestor_pages: &[ThreadListResponse],
        loaded_pages: &[ThreadLoadedListResponse],
        loaded_reads: &[ThreadReadResponse],
    ) -> anyhow::Result<Self> {
        validate_page_chain(
            ancestor_pages
                .iter()
                .map(|page| page.next_cursor.as_deref()),
            ancestor_pages.len(),
            "thread/list",
        )?;
        validate_page_chain(
            loaded_pages.iter().map(|page| page.next_cursor.as_deref()),
            loaded_pages.len(),
            "thread/loaded/list",
        )?;
        ensure_not_guardian(root)?;
        let mut records = BTreeMap::new();
        insert_record(&mut records, thread_record(root)?)?;
        for thread in ancestor_pages.iter().flat_map(|page| &page.data) {
            ensure_not_guardian(thread)?;
            if thread.id == root.id {
                bail!("ancestor thread/list unexpectedly included the root");
            }
            if thread.session_id != root.session_id {
                bail!("descendant belongs to a different session");
            }
            if thread.thread_source != Some(ThreadSource::Subagent) {
                bail!("descendant is not a persisted Subagent thread");
            }
            insert_record(&mut records, thread_record(thread)?)?;
        }
        for record in records.values() {
            if record.id == root.id {
                if record.parent_thread_id.is_some() {
                    bail!("root thread unexpectedly has a parent");
                }
            } else {
                let parent = record
                    .parent_thread_id
                    .as_ref()
                    .context("descendant thread is missing its parent edge")?;
                if !records.contains_key(parent) {
                    bail!("descendant parent is outside the known tree");
                }
            }
            if matches!(
                record.status,
                SnapshotStatus::Active | SnapshotStatus::SystemError
            ) {
                bail!("tree scan contains a non-quiet thread");
            }
        }

        let reads = loaded_reads
            .iter()
            .map(|read| (read.thread.id.as_str(), &read.thread))
            .collect::<HashMap<_, _>>();
        let mut loaded_ids = HashSet::new();
        for id in loaded_pages.iter().flat_map(|page| &page.data) {
            if !loaded_ids.insert(id) {
                bail!("thread/loaded/list returned a duplicate thread");
            }
            let thread = reads
                .get(id.as_str())
                .with_context(|| format!("missing typed thread/read for loaded thread {id}"))?;
            ensure_not_guardian(thread)?;
            if thread.session_id == root.session_id && !records.contains_key(id) {
                bail!("same-session loaded thread is missing from ancestor thread/list");
            }
            if let Some(record) = records.get(id)
                && *record != thread_record(thread)?
            {
                bail!("thread/read disagrees with the paginated tree scan");
            }
            if id != &root.id
                && (thread.turns.is_empty()
                    || thread
                        .turns
                        .iter()
                        .any(|turn| turn.status != TurnStatus::Completed))
            {
                bail!("descendant thread/read lacks complete terminal turn history");
            }
        }
        for read in loaded_reads {
            if !loaded_ids.contains(&read.thread.id) {
                bail!("thread/read was not requested by the loaded-thread scan");
            }
        }
        Ok(Self {
            root_thread_id: root.id.clone(),
            session_id: root.session_id.clone(),
            records,
        })
    }

    pub fn thread_count(&self) -> usize {
        self.records.len()
    }

    pub(crate) fn verify_broker_thread_ids(
        &self,
        broker_thread_ids: &HashSet<String>,
    ) -> anyhow::Result<()> {
        let scanned = self.records.keys().cloned().collect::<HashSet<_>>();
        if &scanned != broker_thread_ids {
            bail!("App Server quiet tree differs from the broker lifecycle tree");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TreeEvidence {
    pub usage: Usage,
    pub raw_response_count: u64,
    pub thread_count: u64,
    pub tree_closed: bool,
}

#[derive(Debug)]
pub struct TreeEventCollector {
    root_thread_id: String,
    session_id: String,
    records: BTreeMap<String, ThreadRecord>,
    turns: HashMap<(String, String), bool>,
    raw_responses: HashMap<String, TokenUsageBreakdown>,
}

impl TreeEventCollector {
    pub fn new(root: &codex_app_server_protocol::Thread) -> anyhow::Result<Self> {
        ensure_not_guardian(root)?;
        let record = thread_record(root)?;
        let root_thread_id = root.id.clone();
        let session_id = root.session_id.clone();
        Ok(Self {
            root_thread_id,
            session_id,
            records: BTreeMap::from([(record.id.clone(), record)]),
            turns: HashMap::new(),
            raw_responses: HashMap::new(),
        })
    }

    pub fn ingest(&mut self, notification: ServerNotification) -> anyhow::Result<()> {
        match notification {
            ServerNotification::ThreadStarted(notification) => {
                let thread = notification.thread;
                ensure_not_guardian(&thread)?;
                if thread.session_id != self.session_id {
                    bail!("thread/started belongs to a different session");
                }
                if thread.id != self.root_thread_id {
                    if thread.thread_source != Some(ThreadSource::Subagent) {
                        bail!("non-root thread is not a persisted Subagent");
                    }
                    let parent = thread
                        .parent_thread_id
                        .as_ref()
                        .context("Subagent thread is missing a parent")?;
                    if !self.records.contains_key(parent) {
                        bail!("Subagent parent is not in the known tree");
                    }
                }
                insert_record(&mut self.records, thread_record(&thread)?)?;
            }
            ServerNotification::TurnStarted(notification) => {
                self.ensure_known_thread(&notification.thread_id)?;
                let key = (notification.thread_id, notification.turn.id);
                if self.turns.insert(key, false).is_some() {
                    bail!("duplicate turn/started notification");
                }
            }
            ServerNotification::TurnCompleted(notification) => {
                self.ensure_known_thread(&notification.thread_id)?;
                if notification.turn.status != TurnStatus::Completed {
                    bail!("tree turn did not complete successfully");
                }
                let key = (notification.thread_id, notification.turn.id);
                let terminal = self
                    .turns
                    .get_mut(&key)
                    .context("turn/completed arrived without turn/started")?;
                if *terminal {
                    bail!("duplicate turn terminal");
                }
                *terminal = true;
            }
            ServerNotification::ThreadStatusChanged(notification) => {
                self.ensure_known_thread(&notification.thread_id)?;
                self.records
                    .get_mut(&notification.thread_id)
                    .context("known thread record disappeared")?
                    .status = snapshot_status(&notification.status);
            }
            ServerNotification::RawResponseCompleted(notification) => {
                self.ensure_known_thread(&notification.thread_id)?;
                let usage = notification.usage.context("missing usage")?;
                if let Some(previous) = self.raw_responses.get(&notification.response_id) {
                    if previous == &usage {
                        return Ok(());
                    }
                    bail!("conflicting duplicate raw response usage");
                }
                validate_usage(&usage)?;
                self.raw_responses.insert(notification.response_id, usage);
            }
            ServerNotification::ItemGuardianApprovalReviewStarted(_)
            | ServerNotification::ItemGuardianApprovalReviewCompleted(_)
            | ServerNotification::GuardianWarning(_) => {
                bail!("Guardian lifecycle is forbidden in the evaluation tree");
            }
            _ => {}
        }
        Ok(())
    }

    pub fn close(
        self,
        first_scan: &TreeScan,
        second_scan: &TreeScan,
        broker_completions: &[ResponseCompletedMetadata],
        broker_in_flight: u64,
    ) -> anyhow::Result<TreeEvidence> {
        if broker_in_flight != 0 {
            bail!("broker still has in-flight requests");
        }
        if first_scan != second_scan {
            bail!("the two complete quiet tree scans differ");
        }
        if first_scan.root_thread_id != self.root_thread_id
            || first_scan.session_id != self.session_id
            || first_scan.records != self.records
        {
            bail!("event tree differs from the typed quiet scan");
        }
        if self.turns.is_empty() || self.turns.values().any(|terminal| !terminal) {
            bail!("not every seen turn reached terminal");
        }

        let mut broker = HashMap::new();
        for completion in broker_completions {
            let usage = completion
                .usage
                .as_ref()
                .context("broker completion is missing usage")?;
            if broker
                .insert(completion.response_id.as_str(), usage)
                .is_some()
            {
                bail!("duplicate broker response completion");
            }
        }
        if broker.len() != self.raw_responses.len() {
            bail!("broker and App Server completion counts differ");
        }
        for (response_id, app_usage) in &self.raw_responses {
            let broker_usage = broker
                .get(response_id.as_str())
                .with_context(|| format!("broker is missing response {response_id}"))?;
            if !usage_matches(app_usage, broker_usage) {
                bail!("broker and App Server usage differ for response {response_id}");
            }
        }
        let usage = self
            .raw_responses
            .values()
            .try_fold(Usage::default(), add_usage)?;
        Ok(TreeEvidence {
            usage,
            raw_response_count: u64::try_from(self.raw_responses.len())?,
            thread_count: u64::try_from(self.records.len())?,
            tree_closed: true,
        })
    }

    fn ensure_known_thread(&self, thread_id: &str) -> anyhow::Result<()> {
        if !self.records.contains_key(thread_id) {
            bail!("notification references unknown thread {thread_id}");
        }
        Ok(())
    }
}

pub struct SkillUseTracker {
    canonical_skill_path: std::path::PathBuf,
    expected_skill_bytes: Vec<u8>,
    sequence: u64,
    successful_read_sequence: Option<u64>,
    final_sequence: Option<u64>,
}

impl SkillUseTracker {
    pub fn new(canonical_skill_path: &Path, expected_skill_bytes: &[u8]) -> anyhow::Result<Self> {
        let canonical = canonical_skill_path
            .canonicalize()
            .context("canonicalize target SKILL.md")?;
        Ok(Self {
            canonical_skill_path: canonical,
            expected_skill_bytes: expected_skill_bytes.to_vec(),
            sequence: 0,
            successful_read_sequence: None,
            final_sequence: None,
        })
    }

    pub fn ingest(&mut self, notification: &ServerNotification) -> anyhow::Result<()> {
        let ServerNotification::ItemCompleted(notification) = notification else {
            return Ok(());
        };
        self.sequence = self
            .sequence
            .checked_add(1)
            .context("item sequence overflow")?;
        match &notification.item {
            ThreadItem::CommandExecution {
                command,
                cwd,
                status,
                aggregated_output,
                exit_code,
                ..
            } if *status == codex_app_server_protocol::CommandExecutionStatus::Completed
                && *exit_code == Some(0) =>
            {
                let workdir = cwd
                    .to_inferred_path_uri()
                    .context("command cwd is not an absolute path")?;
                let accessed = codex_skills::implicit_skill_accesses_for_command(command, &workdir)
                    .into_iter()
                    .any(|access| match access {
                        codex_skills::ImplicitSkillAccess::Document(path) => path
                            .to_abs_path()
                            .ok()
                            .and_then(|path| path.canonicalize().ok())
                            .is_some_and(|path| {
                                path.as_path() == self.canonical_skill_path.as_path()
                            }),
                        codex_skills::ImplicitSkillAccess::Script(_) => false,
                    });
                if accessed
                    && aggregated_output.as_deref().map(str::as_bytes)
                        == Some(self.expected_skill_bytes.as_slice())
                {
                    self.successful_read_sequence = Some(self.sequence);
                }
            }
            ThreadItem::AgentMessage { text, .. }
                if serde_json::from_str::<ContentPackage>(text).is_ok()
                    && self.final_sequence.replace(self.sequence).is_some() =>
            {
                bail!("multiple final ContentPackage items");
            }
            _ => {}
        }
        Ok(())
    }

    pub fn finish(self) -> anyhow::Result<String> {
        let read = self
            .successful_read_sequence
            .context("candidate did not successfully read the canonical complete SKILL.md")?;
        let final_sequence = self
            .final_sequence
            .context("candidate final package was not observed")?;
        if read >= final_sequence {
            bail!("candidate Skill read did not complete before the final package");
        }
        let evidence = serde_json::json!({
            "schemaVersion": 1,
            "skillSha256": sha256_bytes(&self.expected_skill_bytes),
            "readBeforeFinal": true
        });
        Ok(sha256_bytes(&serde_json::to_vec(&evidence)?))
    }
}

fn validate_page_chain<'a>(
    cursors: impl Iterator<Item = Option<&'a str>>,
    page_count: usize,
    method: &str,
) -> anyhow::Result<()> {
    if page_count == 0 || page_count > 100 {
        bail!("{method} returned an invalid number of pages");
    }
    let mut seen = HashSet::new();
    let cursors = cursors.collect::<Vec<_>>();
    for (index, cursor) in cursors.iter().enumerate() {
        match cursor {
            Some(cursor) => {
                if index + 1 == page_count {
                    bail!("{method} pagination is incomplete");
                }
                if !seen.insert(*cursor) {
                    bail!("{method} repeated a pagination cursor");
                }
            }
            None if index + 1 != page_count => {
                bail!("{method} supplied pages after a terminal cursor");
            }
            None => {}
        }
    }
    Ok(())
}

fn insert_record(
    records: &mut BTreeMap<String, ThreadRecord>,
    record: ThreadRecord,
) -> anyhow::Result<()> {
    if let Some(previous) = records.get(&record.id) {
        if previous == &record {
            return Ok(());
        }
        bail!("conflicting duplicate thread record");
    }
    records.insert(record.id.clone(), record);
    Ok(())
}

fn thread_record(thread: &codex_app_server_protocol::Thread) -> anyhow::Result<ThreadRecord> {
    Ok(ThreadRecord {
        id: thread.id.clone(),
        parent_thread_id: thread.parent_thread_id.clone(),
        session_id: thread.session_id.clone(),
        status: snapshot_status(&thread.status),
    })
}

fn snapshot_status(status: &ThreadStatus) -> SnapshotStatus {
    match status {
        ThreadStatus::NotLoaded => SnapshotStatus::NotLoaded,
        ThreadStatus::Idle => SnapshotStatus::Idle,
        ThreadStatus::Active { .. } => SnapshotStatus::Active,
        ThreadStatus::SystemError => SnapshotStatus::SystemError,
    }
}

fn ensure_not_guardian(thread: &codex_app_server_protocol::Thread) -> anyhow::Result<()> {
    if thread.thread_source == Some(ThreadSource::GuardianReview) {
        bail!("GuardianReview thread is forbidden in the evaluation tree");
    }
    Ok(())
}

fn usage_matches(app: &TokenUsageBreakdown, broker: &ObservedUsage) -> bool {
    app.total_tokens == broker.total_tokens
        && app.input_tokens == broker.input_tokens
        && app.cached_input_tokens == broker.cached_input_tokens
        && app.cache_write_input_tokens == broker.cache_write_input_tokens
        && app.output_tokens == broker.output_tokens
        && app.reasoning_output_tokens == broker.reasoning_output_tokens
}

fn sha256_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
