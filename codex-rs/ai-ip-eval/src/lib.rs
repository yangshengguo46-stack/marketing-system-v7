#![recursion_limit = "256"]

mod app_server;
mod catalog;
mod evidence;
mod model;

pub use app_server::AppServerClient;
pub use app_server::AppServerHandshake;
pub use app_server::ConfigAuditEvidence;
pub use app_server::ConfigAuditExpectation;
pub use app_server::FrozenSharedConfig;
pub use app_server::JsonLineClient;
pub use app_server::audit_config;
pub use app_server::build_shared_config;
pub use app_server::build_thread_start;
pub use app_server::build_turn_start;
pub use app_server::config_read_params;
pub use app_server::initialize_params;
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
pub use model::EvaluationCondition;
pub use model::ExecutionMode;
pub use model::ModeEvidence;
pub use model::ProofBrokerCompatibilityName;
pub use model::ProviderRole;
pub use model::RunManifest;
pub use model::Usage;

pub fn run_main() -> anyhow::Result<()> {
    anyhow::bail!("no evaluator command selected")
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
