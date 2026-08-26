use codex_ai_ip_domain::HeldOutMissionCase;
use serde_json::Value;

pub const LEAD_SKILL_NAME: &str = "deliver-ai-ip-content-package";
pub const ADDITIONAL_CONTEXT_KEY: &str = "ai_ip_evaluation";
pub const ROOT_PROMPT_MAX_TOKENS: usize = 96;
pub const EVALUATION_CONTEXT_MAX_TOKENS: usize = 900;
const MISSION_CASE_MAX_TOKENS: usize = 640;
const ROOT_PROMPT: &str = "Complete the business task in the supplied additional context. Use any relevant available capabilities as needed and return only the JSON object required by the output schema.";
const TASK: &str = "基于 missionCase 与其中的相对路径材料，交付一份可直接拍摄或发布的短内容成果。先按需检查材料；只输出符合给定 JSON Schema 的对象；不要声称已经执行发布。";

pub fn root_prompt() -> &'static str {
    ROOT_PROMPT
}

pub fn approx_token_count(value: &str) -> usize {
    value.len().div_ceil(4)
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimePromptError {
    #[error("mission validation failed: {0}")]
    InvalidMission(#[from] codex_ai_ip_domain::ValidationErrors),
    #[error("failed to serialize mission context: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("mission case is approximately {actual_tokens} tokens; limit is {max_tokens}")]
    MissionTooLarge {
        actual_tokens: usize,
        max_tokens: usize,
    },
    #[error("evaluation context is approximately {actual_tokens} tokens; limit is {max_tokens}")]
    EvaluationContextTooLarge {
        actual_tokens: usize,
        max_tokens: usize,
    },
}

pub fn evaluation_context(mission: &HeldOutMissionCase) -> Result<String, RuntimePromptError> {
    mission.validate()?;
    let mission = serde_json::to_value(mission)?;
    let mission_tokens = approx_token_count(&serde_json::to_string(&mission)?);
    if mission_tokens > MISSION_CASE_MAX_TOKENS {
        return Err(RuntimePromptError::MissionTooLarge {
            actual_tokens: mission_tokens,
            max_tokens: MISSION_CASE_MAX_TOKENS,
        });
    }

    render_evaluation_context(mission, mission_tokens)
}

pub(crate) fn render_evaluation_context(
    mission: Value,
    mission_tokens: usize,
) -> Result<String, RuntimePromptError> {
    let _ = mission_tokens;
    let evaluation_context = serde_json::json!({"task": TASK, "missionCase": mission});
    let rendered = serde_json::to_string(&evaluation_context)?;
    let actual_tokens = approx_token_count(&rendered);
    if actual_tokens > EVALUATION_CONTEXT_MAX_TOKENS {
        return Err(RuntimePromptError::EvaluationContextTooLarge {
            actual_tokens,
            max_tokens: EVALUATION_CONTEXT_MAX_TOKENS,
        });
    }

    Ok(rendered)
}
