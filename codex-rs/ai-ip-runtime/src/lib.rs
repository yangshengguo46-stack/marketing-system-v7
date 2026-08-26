mod prompt;

pub use prompt::{ADDITIONAL_CONTEXT_KEY, EVALUATION_CONTEXT_MAX_TOKENS, LEAD_SKILL_NAME};
pub use prompt::{ROOT_PROMPT_MAX_TOKENS, RuntimePromptError};
pub use prompt::{approx_token_count, evaluation_context, root_prompt};

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
