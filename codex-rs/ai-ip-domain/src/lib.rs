mod content_package;
mod mission;

pub use content_package::ActionFunnelStep;
pub use content_package::Claim;
pub use content_package::ClaimStatus;
pub use content_package::ContentPackage;
pub use content_package::InfluenceRelation;
pub use content_package::MeasurementPlan;
pub use content_package::PublishableContent;
pub use content_package::Readiness;
pub use mission::HeldOutMissionCase;
pub use mission::MAX_CASE_ID_BYTES;
pub use mission::MAX_CONSTRAINT_BYTES;
pub use mission::MAX_CONSTRAINTS;
pub use mission::MAX_MATERIAL_ID_BYTES;
pub use mission::MAX_MATERIAL_PATH_BYTES;
pub use mission::MAX_MATERIALS;
pub use mission::MAX_OBJECTIVE_BYTES;
pub use mission::MaterialKind;
pub use mission::MissionMaterial;
pub use mission::SubjectKind;
pub use mission::ValidationErrors;

#[cfg(test)]
#[path = "domain_tests.rs"]
mod tests;
