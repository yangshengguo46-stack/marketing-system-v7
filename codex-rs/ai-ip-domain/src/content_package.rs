use std::collections::HashMap;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use crate::HeldOutMissionCase;
use crate::MaterialKind;
use crate::SubjectKind;
use crate::ValidationErrors;

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InfluenceRelation {
    pub subject: String,
    pub subject_kind: SubjectKind,
    pub audience: String,
    pub desired_action: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ActionFunnelStep {
    pub audience_state: String,
    pub intended_change: String,
    pub next_action: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PublishableContent {
    pub format: String,
    pub title: Option<String>,
    pub body: String,
    pub production_notes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub enum ClaimStatus {
    UserFact,
    ExternalEvidence,
    ModelInterpretation,
    CreativeHypothesis,
    Unknown,
    ActualResult,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Claim {
    pub text: String,
    pub status: ClaimStatus,
    pub source_refs: Vec<String>,
    pub result_receipt_ref: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MeasurementPlan {
    pub success_signal: String,
    pub collection_method: String,
    pub observation_window: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub enum Readiness {
    Draft,
    ReadyForHumanReview,
    BlockedByMissingEvidence,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContentPackage {
    pub mission_summary: String,
    pub influence_relation: InfluenceRelation,
    pub action_funnel: Vec<ActionFunnelStep>,
    pub strategic_judgment: String,
    pub publishable_content: PublishableContent,
    pub claims: Vec<Claim>,
    pub open_questions: Vec<String>,
    pub measurement_plan: MeasurementPlan,
    pub readiness: Readiness,
}

impl ContentPackage {
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = Vec::new();

        if self.mission_summary.is_empty() {
            errors.push("missionSummary: must not be empty".into());
        }
        if self.influence_relation.subject.is_empty() {
            errors.push("influenceRelation.subject: must not be empty".into());
        }
        if self.influence_relation.audience.is_empty() {
            errors.push("influenceRelation.audience: must not be empty".into());
        }
        if self.influence_relation.desired_action.is_empty() {
            errors.push("influenceRelation.desiredAction: must not be empty".into());
        }

        if self.action_funnel.is_empty() {
            errors.push("actionFunnel: must contain at least one step".into());
        }
        for (index, step) in self.action_funnel.iter().enumerate() {
            if step.audience_state.is_empty() {
                errors.push(format!(
                    "actionFunnel[{index}].audienceState: must not be empty"
                ));
            }
            if step.intended_change.is_empty() {
                errors.push(format!(
                    "actionFunnel[{index}].intendedChange: must not be empty"
                ));
            }
            if step.next_action.is_empty() {
                errors.push(format!(
                    "actionFunnel[{index}].nextAction: must not be empty"
                ));
            }
        }

        if self.strategic_judgment.is_empty() {
            errors.push("strategicJudgment: must not be empty".into());
        }
        if self.publishable_content.format.is_empty() {
            errors.push("publishableContent.format: must not be empty".into());
        }
        if self
            .publishable_content
            .title
            .as_deref()
            .is_some_and(str::is_empty)
        {
            errors.push("publishableContent.title: must not be empty when present".into());
        }
        if self.publishable_content.body.is_empty() {
            errors.push("publishableContent.body: must not be empty".into());
        }
        for (index, note) in self.publishable_content.production_notes.iter().enumerate() {
            if note.is_empty() {
                errors.push(format!(
                    "publishableContent.productionNotes[{index}]: must not be empty"
                ));
            }
        }

        for (index, claim) in self.claims.iter().enumerate() {
            if claim.text.is_empty() {
                errors.push(format!("claims[{index}].text: must not be empty"));
            }
            for (source_index, source_ref) in claim.source_refs.iter().enumerate() {
                if source_ref.is_empty() {
                    errors.push(format!(
                        "claims[{index}].sourceRefs[{source_index}]: must not be empty"
                    ));
                }
            }
        }
        for (index, question) in self.open_questions.iter().enumerate() {
            if question.is_empty() {
                errors.push(format!("openQuestions[{index}]: must not be empty"));
            }
        }
        if self.measurement_plan.success_signal.is_empty() {
            errors.push("measurementPlan.successSignal: must not be empty".into());
        }
        if self.measurement_plan.collection_method.is_empty() {
            errors.push("measurementPlan.collectionMethod: must not be empty".into());
        }
        if self.measurement_plan.observation_window.is_empty() {
            errors.push("measurementPlan.observationWindow: must not be empty".into());
        }

        for (index, claim) in self.claims.iter().enumerate() {
            if claim.status.requires_source()
                && !claim.source_refs.iter().any(|source| !source.is_empty())
            {
                errors.push(format!(
                    "claims[{index}].sourceRefs: {} requires at least one source",
                    claim.status.wire_name()
                ));
            }
            match (&claim.status, claim.result_receipt_ref.as_deref()) {
                (ClaimStatus::ActualResult, Some(receipt)) if !receipt.is_empty() => {}
                (ClaimStatus::ActualResult, _) => errors.push(format!(
                    "claims[{index}].resultReceiptRef: actualResult requires a nonempty receipt"
                )),
                (_, Some(_)) => errors.push(format!(
                    "claims[{index}].resultReceiptRef: only actualResult may set a receipt"
                )),
                (_, None) => {}
            }
        }

        let has_missing_evidence_marker = !self.open_questions.is_empty()
            || self
                .claims
                .iter()
                .any(|claim| matches!(claim.status, ClaimStatus::Unknown));
        match self.readiness {
            Readiness::Draft => {}
            Readiness::ReadyForHumanReview if has_missing_evidence_marker => errors.push(
                "readiness: readyForHumanReview cannot contain missing-evidence markers".into(),
            ),
            Readiness::ReadyForHumanReview => {}
            Readiness::BlockedByMissingEvidence if !has_missing_evidence_marker => errors.push(
                "readiness: blockedByMissingEvidence requires a missing-evidence marker".into(),
            ),
            Readiness::BlockedByMissingEvidence => {}
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationErrors { errors })
        }
    }

    pub fn validate_against(
        &self,
        mission_case: &HeldOutMissionCase,
    ) -> Result<(), ValidationErrors> {
        let mut errors = Vec::new();
        if let Err(validation_errors) = mission_case.validate() {
            errors.extend(validation_errors.errors);
        }
        if let Err(validation_errors) = self.validate() {
            errors.extend(validation_errors.errors);
        }
        if !errors.is_empty() {
            return Err(ValidationErrors { errors });
        }

        if self.influence_relation.subject_kind != mission_case.subject_kind {
            errors.push("influenceRelation.subjectKind: does not match mission case".into());
        }

        let materials = mission_case
            .materials
            .iter()
            .map(|material| (material.material_id.as_str(), &material.material_kind))
            .collect::<HashMap<_, _>>();
        for (claim_index, claim) in self.claims.iter().enumerate() {
            for (source_index, source_ref) in claim.source_refs.iter().enumerate() {
                let field = format!("claims[{claim_index}].sourceRefs[{source_index}]");
                let Some(material_kind) = materials.get(source_ref.as_str()) else {
                    errors.push(format!("{field}: unknown material id"));
                    continue;
                };
                match claim.status {
                    ClaimStatus::UserFact if **material_kind != MaterialKind::UserInput => {
                        errors.push(format!("{field}: userFact requires userInput material"));
                    }
                    ClaimStatus::ExternalEvidence if **material_kind != MaterialKind::Evidence => {
                        errors.push(format!(
                            "{field}: externalEvidence requires evidence material"
                        ));
                    }
                    ClaimStatus::UserFact
                    | ClaimStatus::ExternalEvidence
                    | ClaimStatus::ModelInterpretation
                    | ClaimStatus::CreativeHypothesis
                    | ClaimStatus::Unknown
                    | ClaimStatus::ActualResult => {}
                }
            }

            if let Some(receipt_ref) = claim.result_receipt_ref.as_deref() {
                let field = format!("claims[{claim_index}].resultReceiptRef");
                let Some(material_kind) = materials.get(receipt_ref) else {
                    errors.push(format!("{field}: unknown material id"));
                    continue;
                };
                if **material_kind != MaterialKind::ActualResultReceipt {
                    errors.push(format!("{field}: requires actualResultReceipt material"));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationErrors { errors })
        }
    }
}

impl ClaimStatus {
    fn requires_source(&self) -> bool {
        matches!(
            self,
            Self::UserFact | Self::ExternalEvidence | Self::ActualResult
        )
    }

    fn wire_name(&self) -> &'static str {
        match self {
            Self::UserFact => "userFact",
            Self::ExternalEvidence => "externalEvidence",
            Self::ModelInterpretation => "modelInterpretation",
            Self::CreativeHypothesis => "creativeHypothesis",
            Self::Unknown => "unknown",
            Self::ActualResult => "actualResult",
        }
    }
}
