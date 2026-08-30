#![recursion_limit = "256"]

mod app_server;
mod blind;
mod blind_bundle;
mod blind_bundle_model;
mod blind_bundle_transaction;
mod blind_bundle_transaction_validate;
mod blind_finalize;
mod blind_verify;
mod broker_gate;
mod catalog;
mod contracts;
mod cost;
mod cost_authority;
mod cost_contracts;
mod cost_inputs;
mod evidence;
mod jcs;
mod model;
#[cfg(test)]
mod native_app_server_fixture;
mod private_inventory;
mod proof_archive;
mod proof_archive_seal;
mod proof_ledger;
mod proof_commitment;
mod runner;
mod score;
mod score_authority;
mod score_decision;
mod score_transaction;
mod score_validation;
mod secure_fs;
mod secure_fs_publish;
mod secure_fs_retain;

pub use app_server::AppServerClient;
pub use app_server::AppServerHandshake;
pub use app_server::ConfigAuditEvidence;
pub use app_server::ConfigAuditExpectation;
pub use app_server::FrozenSharedConfig;
pub use app_server::JsonLineClient;
pub use app_server::audit_config;
pub use app_server::audit_frozen_config;
pub use app_server::build_shared_config;
pub use app_server::build_thread_start;
pub use app_server::build_turn_start;
pub use app_server::config_read_params;
pub use app_server::initialize_params;
pub use broker_gate::ArmActivation;
pub use broker_gate::ArmReceipt;
pub use broker_gate::BrokerGateConfig;
pub use broker_gate::BrokerRuntimeConfig;
pub use broker_gate::PairCoordinator;
pub use broker_gate::PairPhase;
pub use broker_gate::PairReceipt;
pub use broker_gate::ThreadLifecycle;
pub use catalog::CatalogParity;
pub use catalog::CatalogRoots;
pub use catalog::CatalogSnapshot;
pub use catalog::compare_catalogs;
pub use catalog::normalize_catalog;
pub use catalog::validate_stable_catalog;
pub use contracts::ArmReview;
pub use contracts::DimensionScores;
pub use contracts::FrozenContracts;
pub use contracts::NativeHeldOutAttestation;
pub use contracts::PreferredArm;
pub use contracts::ReplayReviewAttestation;
pub use contracts::ReviewerArms;
pub use contracts::ReviewerDeclaration;
pub use contracts::ReviewerQualificationBinding;
pub use contracts::ReviewerSubmission;
pub use contracts::SevereFlags;
pub use contracts::validate_reviewer_submission;
pub use evidence::CollectedReplay;
pub use evidence::ReplayCollector;
pub use evidence::SkillUseExpectation;
pub use evidence::SkillUseOutcome;
pub use evidence::SkillUseTracker;
pub use evidence::TreeEventCollector;
pub use evidence::TreeEvidence;
pub use evidence::TreeScan;
pub use evidence::validate_case_boundary;
pub use model::BlindPackArgs;
pub use model::Cli;
pub use model::EvalCommand;
pub use model::EvaluationCondition;
pub use model::ExecutionMode;
pub use model::FreezeRunContextArgs;
pub use model::MockProviderMode;
pub use model::ModeEvidence;
pub use model::ProofBrokerCompatibilityName;
pub use model::ProviderRole;
pub use model::ReplayPairArgs;
pub use model::RunManifest;
pub use model::ScoreArgs;
pub use model::Usage;
pub use proof_archive::ArchiveEvidenceSource;
pub use proof_archive::ArmPostprocessIndex;
pub use proof_archive::PostprocessSidecarEntry;
pub use proof_archive::PostprocessSidecarKind;
pub use proof_archive::verify_postprocess_archive;
pub use runner::ArtifactCommitments;
pub use runner::ChildEnvironment;
pub use runner::CommittedArmOrder;
pub use runner::ExecutionBoundary;
pub use runner::FrozenExecutionGuard;
pub use runner::GitWorktreeCommitment;
pub use runner::IsolatedHomes;
pub use runner::REQUIRED_EXECUTION_ARTIFACTS;
pub use runner::VerifiedFrozenContext;
pub use runner::commit_arm_order;
pub use runner::freeze_live_context;
pub use runner::freeze_replay_context;
pub use runner::prepare_isolated_homes;
pub use runner::run_local_mock_pair;
pub use runner::run_replay_pair;
pub use runner::verify_frozen_context;
pub use runner::verify_isolated_home_parity;
pub use score::BusinessDecision;
pub use score::CandidateSevereFlagCounts;
pub use score::DecisionMetrics;
pub use score_decision::BlindDecision;
pub use score_decision::BlindDecisionFailure;

pub fn execute_cli(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        EvalCommand::FreezeRunContext(command) => match command.mode {
            FreezeRunContextArgs::Replay(args) => freeze_replay_context(args),
            FreezeRunContextArgs::Live(args) => freeze_live_context(args),
        },
        EvalCommand::ReplayPair(args) => run_replay_pair(args),
        EvalCommand::LivePair(args) => run_local_mock_pair(&args.frozen_run_context),
        EvalCommand::BlindPack(args) => blind::run_blind_pack(args),
        EvalCommand::Score(args) => score_authority::run_score_authority(args),
        EvalCommand::MakeCostReceipt(_)
        | EvalCommand::AnnotateCost(_)
        | EvalCommand::Summarize(_)
        | EvalCommand::VerifyReport(_)
        | EvalCommand::PublishReport(_)
        | EvalCommand::VerifyLiveProof(_)
        | EvalCommand::FinalizeCheckpoint(_)
        | EvalCommand::RetentionCloseout(_) => {
            anyhow::bail!("selected evaluator workflow is not implemented in this work package")
        }
    }
}

pub fn run_main() -> anyhow::Result<()> {
    use clap::Parser;

    execute_cli(Cli::parse())
}

#[cfg(test)]
#[path = "eval_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "contracts_tests.rs"]
mod contracts_tests;

#[cfg(test)]
#[path = "cost_tests.rs"]
mod cost_tests;

#[cfg(test)]
#[path = "cost_contracts_tests.rs"]
mod cost_contracts_tests;

#[cfg(test)]
#[path = "cost_inputs_tests.rs"]
mod cost_inputs_tests;

#[cfg(test)]
#[path = "jcs_tests.rs"]
mod jcs_tests;

#[cfg(test)]
#[path = "private_inventory_tests.rs"]
mod private_inventory_tests;

#[cfg(test)]
#[path = "private_inventory_batch_tests.rs"]
mod private_inventory_batch_tests;

#[cfg(test)]
#[path = "proof_archive_tests.rs"]
mod proof_archive_tests;

#[cfg(test)]
#[path = "proof_ledger_tests.rs"]
mod proof_ledger_tests;

#[cfg(test)]
#[path = "proof_commitment_tests.rs"]
mod proof_commitment_tests;

#[cfg(test)]
#[path = "secure_fs_tests.rs"]
mod secure_fs_tests;

#[cfg(test)]
#[path = "secure_fs_publish_tests.rs"]
mod secure_fs_publish_tests;

#[cfg(test)]
#[path = "secure_fs_retain_tests.rs"]
mod secure_fs_retain_tests;

#[cfg(test)]
#[path = "app_server_tests.rs"]
mod app_server_tests;

#[cfg(test)]
#[path = "blind_tests.rs"]
mod blind_tests;

#[cfg(test)]
#[path = "score_tests.rs"]
mod score_tests;

#[cfg(test)]
#[path = "score_authority_tests.rs"]
mod score_authority_tests;

#[cfg(test)]
#[path = "score_decision_tests.rs"]
mod score_decision_tests;

#[cfg(test)]
#[path = "score_validation_tests.rs"]
mod score_validation_tests;

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod catalog_tests;

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tree_tests;
