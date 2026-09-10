//! Proposed native completion contract, not an existing upstream API or a ready-made fork patch.
//! Move into codex-extension-api and adapt String identifiers to the fork's actual ID types.
//! This draft uses std only. It has NOT been compiled against the v7 workspace in this session.
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type CompletionFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateIdentity {
    pub candidate_id: String,
    pub candidate_sha256: String,
    pub acceptance_sha256: String,
    pub mission_revision: u64,
    pub input_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewBudget {
    pub remaining_tokens: u64,
    pub max_output_tokens: u32,
    pub attempts_remaining: u8,
    pub deadline_unix_ms: u64,
}

/// The host implements this using the existing cancellation tree.
/// It is not an instruction the model can set or clear.
pub trait CancellationView: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WaitKind {
    UserInput,
    UserApproval,
    ProviderAvailability,
    Budget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IssueSeverity { Advisory, Blocking }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionIssue {
    pub issue_id: String,
    pub category: String,
    pub severity: IssueSeverity,
    pub artifact_version_id: Option<String>,
    pub location: Option<String>,
    pub problem: String,
    pub basis_refs: Vec<String>,
    /// Condition to satisfy, NOT a fixed next tool/workflow instruction.
    pub acceptance_condition: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompletionDecision {
    NotApplicable,
    Allow { report_ref: String },
    Continue { feedback: Vec<CompletionIssue> },
    Wait { kind: WaitKind, message: String },
    Block { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompletionError {
    Cancelled,
    StaleCandidate,
    BudgetExceeded,
    InvalidInput(String),
    ProviderUnavailable(String),
    StoreUnavailable(String),
}

/// Source excerpts are tagged data; they must not be merged into trusted instructions.
#[derive(Clone, Debug)]
pub struct ReviewPacket {
    pub identity: Option<CandidateIdentity>,
    pub task_contract_json: String,
    pub candidate_content_json: String,
    pub source_excerpts_json: String,
    pub host_constraints_json: String,
}

/// New bounded adapter over the existing provider, with no tool access and no agent loop.
pub trait ReviewModelClient: Send + Sync {
    fn review_once<'a>(
        &'a self,
        packet: ReviewPacket,
        budget: ReviewBudget,
        cancel: Arc<dyn CancellationView>,
    ) -> CompletionFuture<'a, Result<ReviewModelResult, CompletionError>>;
}

#[derive(Clone, Debug)]
pub struct ReviewModelResult {
    pub result_json: String,
    pub actual_provider_profile_id: String,
    pub actual_model_id: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub provider_receipt_ref: Option<String>,
}

pub struct CompletionInput<'a> {
    pub thread_id: &'a str,
    pub turn_id: &'a str,
    pub identity: Option<&'a CandidateIdentity>,
    /// Always supplied, even before a mission exists: prevents skipping review by not creating work.
    pub bounded_user_request: &'a str,
    pub bounded_candidate_text: &'a str,
    pub packet: &'a ReviewPacket,
    pub budget: ReviewBudget,
    pub cancelled: Arc<dyn CancellationView>,
    pub model: &'a dyn ReviewModelClient,
}

pub trait CompletionContributor: Send + Sync {
    fn review<'a>(
        &'a self,
        input: CompletionInput<'a>,
    ) -> CompletionFuture<'a, Result<CompletionDecision, CompletionError>>;
}

impl CompletionDecision {
    /// A deterministic boundary check; does not decide business or factual quality.
    pub fn validate(&self) -> Result<(), CompletionError> {
        match self {
            Self::Continue { feedback } if feedback.is_empty() =>
                Err(CompletionError::InvalidInput("Continue requires actionable feedback".into())),
            Self::Continue { feedback } if feedback.len() > 16 =>
                Err(CompletionError::InvalidInput("At most 16 bounded findings are allowed".into())),
            Self::Wait { message, .. } if message.trim().is_empty() =>
                Err(CompletionError::InvalidInput("Wait requires a user-readable reason".into())),
            Self::Block { reason } if reason.trim().is_empty() =>
                Err(CompletionError::InvalidInput("Block requires a reason".into())),
            Self::Allow { report_ref } if report_ref.trim().is_empty() =>
                Err(CompletionError::InvalidInput("Allow requires a persisted report reference".into())),
            _ => Ok(()),
        }
    }
}
