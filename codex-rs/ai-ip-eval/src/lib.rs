#![recursion_limit = "256"]

mod app_server;
mod broker_gate;
mod catalog;
mod evidence;
mod model;
mod runner;

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
pub use evidence::CollectedReplay;
pub use evidence::ReplayCollector;
pub use evidence::SkillUseTracker;
pub use evidence::TreeEventCollector;
pub use evidence::TreeEvidence;
pub use evidence::TreeScan;
pub use evidence::validate_case_boundary;
pub use model::Cli;
pub use model::EvalCommand;
pub use model::EvaluationCondition;
pub use model::ExecutionMode;
pub use model::FreezeRunContextArgs;
pub use model::ModeEvidence;
pub use model::ProofBrokerCompatibilityName;
pub use model::ProviderRole;
pub use model::ReplayPairArgs;
pub use model::RunManifest;
pub use model::Usage;
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

pub fn execute_cli(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        EvalCommand::FreezeRunContext(command) => match command.mode {
            FreezeRunContextArgs::Replay(args) => freeze_replay_context(args),
            FreezeRunContextArgs::Live(args) => freeze_live_context(args),
        },
        EvalCommand::ReplayPair(args) => run_replay_pair(args),
        EvalCommand::LivePair(args) => run_local_mock_pair(&args.frozen_run_context),
        _ => anyhow::bail!("selected evaluator workflow is not implemented in this work package"),
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
#[path = "app_server_tests.rs"]
mod app_server_tests;

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod catalog_tests;

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tree_tests;
