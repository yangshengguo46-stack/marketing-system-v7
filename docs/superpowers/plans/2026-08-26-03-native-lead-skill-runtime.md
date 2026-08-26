# Phase 0A Work Package 4 — Native Lead Skill and Bounded Runtime

> **Execution:** Use `superpowers:test-driven-development`, `superpowers:writing-skills`, `skill-creator`, `superpowers:subagent-driven-development`, and `superpowers:verification-before-completion`.

**Goal:** Add one removable Codex-native Lead Skill and a provider-neutral runtime crate that supplies a bounded Mission envelope plus an OpenAI Responses strict-compatible `ContentPackage` JSON Schema. This package does not yet send a model request or modify App Server behavior.

**Authority:** `docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`, Work Package 4. Supporting authorities are `docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md`, `docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md`, `docs/architecture/2026-08-25-legacy-six-version-decision-memory.md`, `docs/architecture/codex-fork-patch-ledger.md`, the pinned parser in `codex-rs/skills/src/parser.rs`, the pinned resource resolver in `codex-rs/utils/cargo-bin/src/lib.rs`, and the [official Structured Outputs guide](https://developers.openai.com/api/docs/guides/structured-outputs).

**Base:** `54538fa3884d11fe13a55828ce5c035a170890a0`

**Provider boundary:** `providerMode=not-run`; `paidProviderCost=0`; do not locate or read an API key.

## Global constraints

- Preserve one Lead responsible for the result and free to read materials, use tools, or create subagents only when useful. Do not encode a fixed roster or workflow.
- Treat the desired audience action as an adaptation and acceptance criterion, never as a rule forcing a product premise or CTA.
- Missing evidence still permits a useful draft plus open questions. Never fabricate publication or performance.
- Keep facts, external evidence, interpretation, creative hypotheses, unknowns, and actual-result receipts distinct through the existing domain contract.
- The Skill contains decision principles only: no Schema copy, inline material bodies, mandatory subagents, held-out answers, fixed questionnaire, or fixed strategy count.
- Runtime production dependencies are only `codex-ai-ip-domain`, `schemars`, `serde_json`, and `thiserror`; `codex-skills` and `codex-utils-cargo-bin` are dev-only.
- Do not depend on `codex-core`, construct context fragments, modify App Server/provider/evaluator code, or run a provider. Work Package 5 owns request wiring and `strict:true` integration proof.
- Do not run complete-workspace `just test`; Rust tests use scoped `just test -p ...` only.

## Files

Create:

- `codex-rs/ai-ip-runtime/Cargo.toml`
- `codex-rs/ai-ip-runtime/BUILD.bazel`
- `codex-rs/ai-ip-runtime/src/{lib.rs,prompt.rs,schema.rs,runtime_tests.rs}`
- `ai-ip-assets/skills/deliver-ai-ip-content-package/{SKILL.md,BUILD.bazel}`

Modify mechanically: `codex-rs/Cargo.toml`, `codex-rs/Cargo.lock`, and `MODULE.bazel.lock` only if Bazel lock update changes it.

## Public API

`lib.rs` privately declares `prompt` and `schema`, declares `#[cfg(test)] #[path = "runtime_tests.rs"] mod tests;`, and re-exports only:

```rust
pub use prompt::{ADDITIONAL_CONTEXT_KEY, EVALUATION_CONTEXT_MAX_TOKENS, LEAD_SKILL_NAME};
pub use prompt::{ROOT_PROMPT_MAX_TOKENS, RuntimePromptError};
pub use prompt::{approx_token_count, evaluation_context, root_prompt};
pub use schema::{StrictSchemaError, content_package_schema, validate_responses_strict_subset};
```

### Prompt contract

Use these exact values:

```rust
pub const LEAD_SKILL_NAME: &str = "deliver-ai-ip-content-package";
pub const ADDITIONAL_CONTEXT_KEY: &str = "ai_ip_evaluation";
pub const ROOT_PROMPT_MAX_TOKENS: usize = 96;
pub const EVALUATION_CONTEXT_MAX_TOKENS: usize = 900;
const MISSION_CASE_MAX_TOKENS: usize = 640;
const ROOT_PROMPT: &str = "Complete the business task in the supplied additional context. Use any relevant available capabilities as needed and return only the JSON object required by the output schema.";
const TASK: &str = "基于 missionCase 与其中的相对路径材料，交付一份可直接拍摄或发布的短内容成果。先按需检查材料；只输出符合给定 JSON Schema 的对象；不要声称已经执行发布。";

pub fn root_prompt() -> &'static str { ROOT_PROMPT }
pub fn approx_token_count(value: &str) -> usize { value.len().div_ceil(4) }
```

The public error is:

```rust
#[derive(Debug, thiserror::Error)]
pub enum RuntimePromptError {
    #[error("mission validation failed: {0}")]
    InvalidMission(#[from] codex_ai_ip_domain::ValidationErrors),
    #[error("failed to serialize mission context: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("mission case is approximately {actual_tokens} tokens; limit is {max_tokens}")]
    MissionTooLarge { actual_tokens: usize, max_tokens: usize },
    #[error("evaluation context is approximately {actual_tokens} tokens; limit is {max_tokens}")]
    EvaluationContextTooLarge { actual_tokens: usize, max_tokens: usize },
}
```

`evaluation_context(&HeldOutMissionCase)` validates before serialization, serializes the case once to `Value`, compact-renders and checks it against 640 approximate tokens, builds exactly `{"task": TASK, "missionCase": mission}`, compact-renders and checks it against 900, then returns the string. It never reads referenced paths.

### Strict Schema contract

`content_package_schema() -> Result<Value, StrictSchemaError>` starts with `schemars::schema_for!(ContentPackage)`, serializes to `Value`, normalizes, validates, and returns it.

```rust
#[derive(Debug, thiserror::Error)]
pub enum StrictSchemaError {
    #[error("failed to serialize generated schema: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid Responses strict schema at {path}: {message}")]
    Invalid { path: String, message: String },
}
```

Normalize deterministically:

1. Require an object root and remove its `$schema`.
2. Remove root `definitions` into a temporary value, reject a pre-existing root `$defs`, recursively process each definition, then insert the table as root `$defs` (atomic rename).
3. Traverse `properties`, `items`, `anyOf`, `oneOf`, and `$defs`. Rewrite `#/definitions/<token>` to `#/$defs/<token>` without changing the escaped token.
4. Every node typed `object` or declaring `properties` gets `additionalProperties:false` and `required` exactly equal to its property keys in map order. Schemars `Option<T>` therefore becomes required while its generated nullable `anyOf` remains intact.
5. Do not inline or erase refs and do not introduce `allOf`.

`validate_responses_strict_subset(&Value)` is read-only and verifies:

- root is object and not root `anyOf`;
- no `$schema`, `definitions`, `allOf`, `not`, `dependentRequired`, `dependentSchemas`, `if`, `then`, `else`, or `patternProperties`;
- keywords are limited to `$defs`, `$ref`, `type`, `properties`, `required`, `additionalProperties`, `items`, `anyOf`, `oneOf`, `enum`, `const`, `description`, `title`, `pattern`, `format`, `multipleOf`, `maximum`, `exclusiveMaximum`, `minimum`, `exclusiveMinimum`, `minItems`, and `maxItems`;
- every object has a properties object, `additionalProperties:false`, and a duplicate-free `required` array equal to all property keys;
- refs are `#` or `#/$defs/<token>`; definition tokens decode JSON Pointer `~1`/`~0` and resolve at the root;
- child containers have the correct JSON type and `anyOf`/`oneOf` are nonempty arrays of schema objects;
- `type` is one supported primitive string or a nonempty duplicate-free array of supported primitive strings.

The allowlist follows the official normal-model Structured Outputs subset and deliberately rejects unsupported composition. It makes no fine-tuned-model claim.

## Canonical Skill

Start `SKILL.md` exactly with:

```yaml
---
name: deliver-ai-ip-content-package
description: Use when the user needs an evidence-aware, publishable AI IP content package tied to a real audience action.
---
```

Use this complete body:

```markdown
# Deliver AI IP Content Package

Turn one mission and its verifiable materials into a directly shootable or publishable content package with a credible audience-action path.

## Judgment loop

- Establish the subject, audience, and desired action. Use the action to adapt and evaluate the work, not to force the content premise into a product pitch or call to action.
- Look for a worthwhile content opportunity in real evidence, events, conflicts, or lived experience. Research named external facts when the supplied materials do not verify them.
- Choose a concrete topic and form that can carry the opportunity, then check that the result still has an honest business connection.
- Revisit or skip any judgment above when the mission makes that useful. These are decision principles, not mandatory stages.

## Evidence and delivery

- Read referenced materials and use available tools or subagents only when they improve the result.
- Keep facts, inferences, and creative hypotheses visibly distinct.
- When evidence is missing, preserve the most useful draft possible and state the unresolved questions.
- Deliver content that can be shot or published, with practical production notes and a measurement plan.
- Describe publication or observed effects only when the mission includes verifiable receipts for them.

Return only the output requested by the active task and its output schema.
```

Do not add reference files, scripts, examples, UI metadata, or `agents/openai.yaml`; this repository-owned runtime asset is intentionally one file and one Bazel target.

`BUILD.bazel` is exactly:

```starlark
filegroup(
    name = "skill",
    srcs = ["SKILL.md"],
    visibility = [
        "//codex-rs/ai-ip-eval:__pkg__",
        "//codex-rs/ai-ip-runtime:__pkg__",
    ],
)
```

## Crate/build skeleton

`Cargo.toml`:

```toml
[package]
edition.workspace = true
license.workspace = true
name = "codex-ai-ip-runtime"
version.workspace = true

[lib]
doctest = false
name = "codex_ai_ip_runtime"
path = "src/lib.rs"

[lints]
workspace = true

[dependencies]
codex-ai-ip-domain = { workspace = true }
schemars = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
codex-skills = { workspace = true }
codex-utils-cargo-bin = { workspace = true }
pretty_assertions = { workspace = true }
```

Add `"ai-ip-runtime"` immediately after `"ai-ip-domain"` in members and `codex-ai-ip-runtime = { path = "ai-ip-runtime" }` immediately after the existing domain workspace dependency.

`BUILD.bazel`:

```starlark
load("//:defs.bzl", "codex_rust_crate")

codex_rust_crate(
    name = "ai-ip-runtime",
    crate_name = "codex_ai_ip_runtime",
    test_data_extra = ["//ai-ip-assets/skills/deliver-ai-ip-content-package:skill"],
)
```

## Task 1 — RED/GREEN bounded prompt

1. Start this plan's SDD workspace/ledger and record the preflight file/interface table. Verify the existing feature branch and clean start.
2. Add the crate/build skeleton, workspace entries, empty modules, and tests before production bodies.
3. Tests use only neutral `case-1`, `material-1`, `evidence/a.txt`, and a lowercase digest. Cover: both token caps and both envelope keys; exact task/wire shape and context key; invalid Mission before serialization; a valid Mission crossing the inner 640 limit; independent outer-limit behavior (test a private renderer/limit helper if the fixed task makes the boundary unreachable); and byte-count rounding for lengths 0/1/4/5.
4. Run `just test -p codex-ai-ip-runtime`. Acceptable RED is unresolved runtime symbols, not member/dependency failures. Record the failure.
5. Implement only the prompt contract and rerun the scoped test to GREEN before adding Schema or Skill tests.

## Task 2 — RED/GREEN strict Schema

1. Add failing tests for: recursive closure/no `$schema`; `required == properties` and nullable required `publishableContent.title` plus `Claim.resultReceiptRef`; `$defs` plus rewritten/resolving refs; rejection of root `anyOf`, nested `$schema`, and `allOf`; rejection of open objects and missing/extra/duplicate required keys; rejection of old/dangling refs; acceptance of a required nullable property.
2. Run the scoped test. Acceptable RED is unresolved Schema API while prompt tests compile and remain GREEN.
3. Implement the converter and validator exactly above, without depending on `codex-tools` or copying its permissive sanitizer. Rerun the scoped test to GREEN.

## Task 3 — Skill RED/GREEN, native parser, build, commit

1. Before creating the Skill, define three neutral pressure scenarios in the SDD report: a brand-awareness mission where an action tempts a sales CTA despite a stronger real founder event; a personal-IP mission with one verified user fact and one unsupported named external fact; and an organization-recruitment mission lacking proof for an outcome but containing enough material for a useful draft.
2. Run each in fresh context without the Skill. Score whether the response roots content in evidence rather than CTA, separates unsupported facts, preserves a useful draft, avoids claiming publication/results, and avoids inventing a fixed team/workflow. At least one miss or material inconsistency is the required Skill RED. If all pass, add one neutral scenario targeting the remaining highest-risk boundary; do not author redundant guidance without a demonstrated gap.
3. Create the canonical Skill and Bazel target. Add a parser test using `codex_utils_cargo_bin::find_resource!("../../ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md")`, `codex_skills::parse_skill_frontmatter_metadata`, exact name/description assertions, and a nonempty body assertion. Never rely on cwd.
4. Run the same scenarios in fresh context with the Skill. Refine only wording tied to an observed failure and rerun the affected case.
5. Run `just test -p codex-ai-ip-runtime`, then the exact prohibited-content gate from the authoritative spec.
6. Run `just bazel-lock-update`, inspect the diff, then run `bazel test //codex-rs/ai-ip-runtime:ai-ip-runtime-unit-tests`, `bazel build //ai-ip-assets/skills/deliver-ai-ip-content-package:skill`, `just bazel-lock-check`, `just fmt`, and `just fix -p codex-ai-ip-runtime`. Per upstream rules, do not rerun the same test after `just fix`; inspect its diff.
7. Inspect `git diff --check`, status, and the full task diff. Commit only declared files and changed locks as `feat: add removable AI IP Lead skill`.

## Review and final verification

- Use a fresh implementer per Task, task-scoped spec/quality review, and the SDD fix loop. Pressure-test agents are evaluators and never edit the worktree.
- Final review covers the entire Work Package 4 range, especially business flexibility, parser compatibility, strict closure/refs, Cargo/Bazel parity, and absence of App Server/provider scope.
- Before any completion claim, freshly run:

```bash
just test -p codex-ai-ip-runtime
bazel test //codex-rs/ai-ip-runtime:ai-ip-runtime-unit-tests
bazel build //ai-ip-assets/skills/deliver-ai-ip-content-package:skill
just bazel-lock-check
git diff --check 54538fa3884d11fe13a55828ce5c035a170890a0..HEAD
git status --short
```

- Report test counts/exit codes, commits, ledger rulings, and no provider call/key/cost. Do not claim Phase 0A baseline PASS, G2, Phase 0B, or live-model compatibility.
