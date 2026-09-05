> **HISTORICAL_COMPLETED — NOT EXECUTABLE (2026-09-05).** This completed provenance/business-primitive record is preserved as history. Its commands, follow-on authorizations and proof prerequisites are superseded by the approved [business-first cleanup](../specs/2026-09-05-business-first-cleanup-design.md) and current [roadmap](2026-08-25-00-codex-ai-ip-master-roadmap.md). Retained implementation remains; the old Phase 0A/06A/06B/07A/07B gates are `RETIRED_NOT_PASSED`. No historical child is executable and no business PASS or model/customer-release qualification is claimed.

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

The final `lib.rs` privately declares `prompt` and `schema`, declares `#[cfg(test)] #[path = "runtime_tests.rs"] mod tests;`, and re-exports only:

```rust
pub use prompt::{ADDITIONAL_CONTEXT_KEY, EVALUATION_CONTEXT_MAX_TOKENS, LEAD_SKILL_NAME};
pub use prompt::{ROOT_PROMPT_MAX_TOKENS, RuntimePromptError};
pub use prompt::{approx_token_count, evaluation_context, root_prompt};
pub use schema::{StrictSchemaError, content_package_schema, validate_responses_strict_subset};
```

Task 1 uses this exact compilable intermediate `lib.rs`; Schema is not declared or exported yet:

```rust
mod prompt;

pub use prompt::{ADDITIONAL_CONTEXT_KEY, EVALUATION_CONTEXT_MAX_TOKENS, LEAD_SKILL_NAME};
pub use prompt::{ROOT_PROMPT_MAX_TOKENS, RuntimePromptError};
pub use prompt::{approx_token_count, evaluation_context, root_prompt};

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
```

Only after Task 1 is GREEN does Task 2 add `mod schema;` plus the three final Schema re-exports. Task 1 RED must be attributable to unresolved prompt symbols in `prompt.rs`; Task 2 RED must be attributable to unresolved Schema symbols after the Schema module/export lines are added.

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
3. Traverse `properties`, `items`, `anyOf`, `oneOf`, `allOf`, and `$defs` so unsupported compositions cannot hide nested metadata. Rewrite `#/definitions/<token>` to `#/$defs/<token>` without changing the escaped token.
4. Every node typed `object` or declaring `properties` gets `additionalProperties:false` and `required` exactly equal to its property keys in map order. Schemars `Option<T>` therefore becomes required while its generated nullable `anyOf` remains intact.
5. Do not inline or erase refs and do not introduce `allOf`.

`validate_responses_strict_subset(&Value)` is read-only and verifies:

- root is object and not root `anyOf`;
- no `$schema`, `definitions`, `oneOf`, `allOf`, `not`, `dependentRequired`, `dependentSchemas`, `if`, `then`, `else`, or `patternProperties`;
- keywords are limited to `$defs`, `$ref`, `type`, `properties`, `required`, `additionalProperties`, `items`, `anyOf`, `enum`, `const`, `description`, `title`, `pattern`, `format`, `multipleOf`, `maximum`, `exclusiveMaximum`, `minimum`, `exclusiveMinimum`, `minItems`, and `maxItems`;
- every object has a properties object, `additionalProperties:false`, and a duplicate-free `required` array equal to all property keys;
- refs are `#` or `#/$defs/<token>`; definition tokens decode JSON Pointer `~1`/`~0` and resolve at the root;
- child containers have the correct JSON type and `anyOf` is a nonempty array of schema objects;
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
- Before drafting, build an internal evidence ledger keyed by source ID. Record each supported fact as subject, relation, object, and modifiers; preserve which action or entity every role, quantity, date, duration, sequence, and ordinal modifies, and never transfer a modifier to a neighboring action or entity.
- Before returning, compare every factual phrase in the premise, draft, and claims against that ledger. Every factual element in the premise must map to supplied evidence; never use a placeholder there. If a draft or claim phrase does not map exactly, remove it, label it as an inference or creative hypothesis, or turn it into an explicit bracketed placeholder or open question.
- Treat source entailment as the release gate. Review the premise, draft, and claims sentence by sentence; every unlabeled factual phrase must have one source span that entails the same relation and modifier attachments. If none does, remove or relabel the phrase. Facts from separate materials may appear together, but their co-occurrence does not establish a relationship between their entities or events.
- For factual language presented as supported, preserve predicate scope and attachment: do not substitute a neighboring action, participant, object, time, or ordinal, and do not infer a relation from co-occurrence or collective completion. This does not bar clearly labeled inference, creative hypothesis, prospective staging, bracketed draft placeholders, or open questions.
- Before release, run a final atomic-proposition audit for factual language presented as supported. Resolve implicit subjects, ellipsis, possessive or appositive links, coreference, temporal anchors and aspect, and causal, progress, or result relations; each resulting atomic proposition must be entailed with the same attachments by one source span. When combining independent facts, keep them independently attributable. If an atomic proposition lacks that entailment, split it, remove it, or explicitly label it as an inference.
- Keep facts, inferences, and creative hypotheses visibly distinct.
- When evidence is missing, preserve the most useful draft possible and state the unresolved questions.
- Deliver content that can be shot or published, with practical production notes and a measurement plan.
- Describe publication or observed effects only when the mission includes verifiable receipts for them.

Return only the output requested by the active task and its output schema.
```

Do not add reference files, scripts, examples, UI metadata, or `agents/openai.yaml`; this repository-owned runtime asset is intentionally one file and one Bazel target.

`skill-creator`'s frontmatter/scaffold validator is still required; lack of generated UI metadata does not supersede it:

```bash
python3 /Users/yangyucheng/.codex/skills/.system/skill-creator/scripts/quick_validate.py ai-ip-assets/skills/deliver-ai-ip-content-package
```

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
2. Add the crate/build skeleton, workspace entries, the exact intermediate `lib.rs`, empty `prompt.rs`, and tests before production bodies. Do not create or declare `schema.rs` yet. This Task's brief is self-contained by carrying the exact `Cargo.toml` and `BUILD.bazel` blocks from **Crate/build skeleton**, the exact intermediate `lib.rs` from **Public API**, and every exact prompt constant/error/function signature from **Prompt contract**; use them verbatim rather than reading the whole plan.
3. Tests use only neutral `case-1`, `material-1`, `evidence/a.txt`, and a lowercase digest. Define `fn valid_case() -> HeldOutMissionCase`, `fn mission_over_inner_limit() -> HeldOutMissionCase`, and the private production seam `fn render_evaluation_context(mission: Value, mission_tokens: usize) -> Result<String, RuntimePromptError>`. `evaluation_context` alone performs domain validation/serialization and then calls that seam. Add compilable tests equivalent to:

```rust
#[test]
fn root_and_both_context_layers_are_bounded_before_submission() {
    assert!(approx_token_count(root_prompt()) <= ROOT_PROMPT_MAX_TOKENS);
    let rendered = evaluation_context(&valid_case()).unwrap();
    assert!(approx_token_count(&rendered) <= EVALUATION_CONTEXT_MAX_TOKENS);
    let envelope: Value = serde_json::from_str(&rendered).unwrap();
    let keys = envelope.as_object().unwrap().keys().map(String::as_str).collect::<BTreeSet<_>>();
    assert_eq!(keys, BTreeSet::from(["missionCase", "task"]));
}

#[test]
fn oversized_mission_is_rejected_before_envelope_serialization() {
    assert!(matches!(evaluation_context(&mission_over_inner_limit()),
        Err(RuntimePromptError::MissionTooLarge { max_tokens: 640, .. })));
}

#[test]
fn outer_envelope_has_an_independent_limit() {
    let mission = json!({"objective": "x".repeat(3_600)});
    assert!(matches!(render_evaluation_context(mission, 640),
        Err(RuntimePromptError::EvaluationContextTooLarge { max_tokens: 900, .. })));
}

#[test]
fn approximate_token_count_rounds_bytes_up() {
    assert_eq!([0, 1, 1, 2], ["", "x", "xxxx", "xxxxx"].map(approx_token_count));
}
```

Also assert the exact canonical task literal, camelCase `missionCase` fields, `ADDITIONAL_CONTEXT_KEY`, and `InvalidMission` for an empty objective. Expected values are literals, not production helper output. The crate-private outer renderer is imported by the sibling test module with `use super::prompt::render_evaluation_context`; it is `pub(crate)`, not part of the public re-export surface.
4. Run `just test -p codex-ai-ip-runtime`. Acceptable RED is unresolved runtime symbols, not member/dependency failures. Record the failure.
5. Implement only the prompt contract and rerun the scoped test to GREEN before adding Schema or Skill tests.

## Task 2 — RED/GREEN strict Schema

1. Create `schema.rs`, add `mod schema;` plus `pub use schema::{StrictSchemaError, content_package_schema, validate_responses_strict_subset};` to `lib.rs`, then add failing tests. The Task brief also carries verbatim the exact `StrictSchemaError`, public function signatures, normalization order, validator rules, and keyword lists from **Strict Schema contract**; do not reconstruct them from memory. Test helpers are `fn assert_closed_objects(&Value)`, `fn assert_required_equals_properties_recursively(&Value)`, `fn collect_refs(&Value, &mut Vec<String>)`, and `fn strict_error(Value) -> StrictSchemaError`. The first three recurse only through schema-bearing keys (`properties`, `$defs`, `items`, `anyOf`, `oneOf`, `allOf`), never through arbitrary strings. Required positive tests cover recursive closure/no `$schema`; `required == properties`; nullable required `publishableContent.title` and `$defs/Claim/properties/resultReceiptRef`; root `$defs`; at least one rewritten and resolving `#/$defs/` ref; acceptance of a required nullable property; and acceptance/resolution of an escaped definition token such as `a~1b~0c`.

Name the crate-private production seam `pub(crate) fn normalize_schema(schema: &mut Value) -> Result<(), StrictSchemaError>`. Before implementing it, add this direct RED shape (expand assertions literally rather than snapshotting the whole document):

```rust
#[test]
fn normalization_descends_into_unsupported_compositions_before_validation() {
    let mut schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": {
            "Leaf": {"type": "object", "properties": {"value": {"type": "string"}}}
        },
        "type": "object",
        "properties": {
            "choice": {"oneOf": [
                {"type": "object", "properties": {"leaf": {"$ref": "#/definitions/Leaf"}}}
            ]},
            "combined": {"allOf": [
                {"type": "object", "properties": {"leaf": {"$ref": "#/definitions/Leaf"}}}
            ]}
        }
    });
    normalize_schema(&mut schema).unwrap();
    assert!(schema.get("$schema").is_none());
    assert!(schema.get("definitions").is_none());
    assert!(schema.get("$defs").is_some());
    for (pointer, required) in [
        ("/properties/choice/oneOf/0", json!(["leaf"])),
        ("/properties/combined/allOf/0", json!(["leaf"])),
        ("/$defs/Leaf", json!(["value"])),
    ] {
        let object = schema.pointer(pointer).unwrap();
        assert_eq!(object["additionalProperties"], false);
        assert_eq!(object["required"], required);
    }
    assert_eq!(schema.pointer("/properties/choice/oneOf/0/properties/leaf/$ref"),
        Some(&json!("#/$defs/Leaf")));
    assert_eq!(schema.pointer("/properties/combined/allOf/0/properties/leaf/$ref"),
        Some(&json!("#/$defs/Leaf")));
    let StrictSchemaError::Invalid { path, message } = strict_error(schema) else {
        panic!("expected invalid strict schema");
    };
    assert_eq!(
        (path, message),
        (
            "$.properties.choice".to_string(),
            "unsupported keyword oneOf".to_string(),
        ),
    );
}
```

Use table-driven invalid fixtures with literal expected `(path, message)` pairs. At minimum include:

| Invalid fixture | Expected path | Expected message |
|---|---|---|
| root `anyOf` | `$` | `root must be an object schema without anyOf` |
| nested `$schema` | `$.properties.x` | `unsupported keyword $schema` |
| nested `oneOf`/`allOf`/`not`/`if` | `$.properties.x` | `unsupported keyword <keyword>` |
| unknown keyword `default` | `$.properties.x` | `unsupported keyword default` |
| `properties: []` | `$` | `properties must be an object` |
| `$defs: []` | `$` | `$defs must be an object` |
| `items: []` | `$.properties.x` | `items must be an object schema` |
| empty/non-array `anyOf` | `$.properties.x` | `anyOf must be a nonempty array of object schemas` |
| missing/true `additionalProperties` | `$` | `object must set additionalProperties to false` |
| missing/extra/duplicate/non-array `required` | `$` | `required must contain every property exactly once` |
| unsupported/duplicate/empty/non-string `type` | `$` | `type must contain unique supported primitive names` |
| `#/definitions/X` | `$.properties.x` | `local ref must use #/$defs/` |
| `#/$defs/Missing` | `$.properties.x` | `dangling local ref` |
| `#/$defs/a~2b` | `$.properties.x` | `invalid JSON Pointer escape` |

Every fixture is a complete closed root object so it fails only for the named mutation. Destructure `StrictSchemaError::Invalid { path, message }` and assert the exact tuple, not string containment; do not add `PartialEq` to the public error solely for tests.
2. Run the scoped test. Acceptable RED is unresolved Schema API while prompt tests compile and remain GREEN.
3. Implement the converter and validator exactly above, without depending on `codex-tools` or copying its permissive sanitizer. Rerun the scoped test to GREEN.

## Task 3 — Skill RED/GREEN, native parser, build, commit

1. Before creating the Skill, write `skill-eval-packet.md` and `skill-behavior-eval.md` in this plan's ignored SDD workspace. The Task brief carries verbatim the canonical frontmatter/body, Skill `BUILD.bazel`, pinned parser/resource call, validation commands, and the following exact packet; it must not make the implementer read the whole plan.

```text
You are handling three unrelated AI-IP missions. For each case return one compact JSON object with exactly these keys: caseId, contentPremise, publishableDraft, claims, openQuestions, measurementPlan, readiness. Each claim has text, status, and sourceIds. Use only the supplied materials; do not describe your evaluation method.

CASE brand-01
objective: Build awareness for the brand. Desired audience action: visit profile.
subjectKind: brand
materials:
- founder-note (userInput): “昨晚仓库临时停电，创始人开车四十公里借来发电机，和两名同事把当天已经承诺的十二个包裹逐一封好。”
- product-card (userInput): “产品是一款普通的可重复使用水杯，有三种颜色，支持刻字。”
constraint: The result must be directly shootable as a short video.

CASE person-01
objective: Build trust in a personal IP. Desired audience action: follow for the next update.
subjectKind: person
materials:
- career-note (userInput): “我在银行工作九年后，于2024年3月离职，开始记录自己学习木工的过程。”
- draft-note (other): “有人建议写‘2025年知识付费行业留存率提升37%’，但没有提供出处。”
constraint: Named external facts require verifiable support.

CASE org-01
objective: Attract suitable applicants to the organization. Desired audience action: submit an application.
subjectKind: organization
materials:
- rehearsal-note (evidence): “周六下雨，十一名现有成员仍完成了两小时的公开演练；负责人最后独自清点并归还了所有器材。”
constraint: There is no publication receipt, application count, or outcome receipt in this mission.
```

Write these exact bytes between the fence markers only, end with one newline, and record `shasum -a 256 skill-eval-packet.md` in `skill-behavior-eval.md` before any control. Result files are exactly `skill-control-01.md` through `skill-control-05.md` and `skill-treatment-01.md` through `skill-treatment-05.md`. Before every treatment dispatch, recompute the digest and require exact equality with the recorded control digest.
2. The rubric scores five booleans per case: evidence-rooted premise rather than forced CTA; unsupported named fact not presented as verified; usable draft preserved; no claimed publication/result; no fixed team/workflow invented. A sample passes only at 15/15. Run five fresh-context control samples of the combined packet without the Skill and preserve verbatim outputs plus manual scores. Each evaluator gets only the packet path, inherits no conversation/plan, is read-only, and writes one uniquely named result file. At least one failing control or disagreement across samples establishes RED. If all five controls score 15/15 consistently, record `SKILL_RED_NOT_ESTABLISHED` and stop before creating the Skill; this is the only non-destructive execution stop introduced by `superpowers:writing-skills`.
3. Create the canonical Skill and Bazel target. Add a parser test using `codex_utils_cargo_bin::find_resource!("../../ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md")`, `codex_skills::parse_skill_frontmatter_metadata`, exact name/description assertions, and a nonempty body assertion. Never rely on cwd.
4. Run five fresh-context treatment samples of the same combined packet. Each evaluator receives only the packet path and committed Skill path, is instructed to read the Skill completely, remains read-only, and writes a unique result. All five must score 15/15. The reviewed Skill body is immutable during execution; a treatment miss requires a reviewed plan amendment and a new RED/GREEN cycle, never ad hoc wording drift. These collaboration-agent evaluations are not the paid Work Package 5 provider run: no API key is read, `providerMode=not-run`, and paid-provider cost remains zero.

### Task 3 behavior cycle 2 amendment

Cycle 1 RED remains the preserved treatment evidence: treatment 03 changed twelve packages into twelve cups, and treatment 05 changed nine years employed at a bank into leaving banking nine years ago while inventing an apprentice role. Both are failures of the existing evidence-rooted-premise rubric item.

For cycle 2, add only the reviewed source-semantics bullet from the canonical Skill above. Do not change the packet, schema, rubric, other Skill wording, or runtime API. Commit the amended Skill before evaluation, recompute the same packet digest before every dispatch, and run five fresh-context treatment samples into `skill-treatment-cycle-02-01.md` through `skill-treatment-cycle-02-05.md`. Preserve cycle 1 outputs. Each cycle 2 evaluator receives only the immutable packet path and the committed amended Skill path, reads the Skill completely, and remains read-only except for its unique result file. The same 15/15 rubric applies; all five samples must pass. Any miss requires another reviewed amendment rather than ad hoc wording drift.

### Task 3 behavior cycle 3 amendment

Cycle 2 RED remains preserved: sample 01 changed packages into orders; sample 02 changed nine years employed at a bank into nine years after leaving banking; sample 03 invented a numbered learning day; and sample 05 invented a first woodworking project. The cycle 2 declarative constraint did not reliably trigger a post-draft fidelity check.

For cycle 3, replace only the cycle 2 source-semantics bullet with the two reviewed operational bullets in the canonical Skill above: an internal source-ID evidence ledger before drafting and an explicit premise/draft/claims fidelity pass before returning. The premise's factual skeleton must map entirely to evidence and may not contain placeholders; explicit placeholders remain available only in drafts or open questions. Do not expose the ledger unless the active output schema requests it. Keep the packet, schema, rubric, every other Skill line, and runtime API unchanged. Commit the amended Skill before evaluation, verify the original packet digest before every dispatch, preserve prior outputs, and run five new fresh-context samples into `skill-treatment-cycle-03-01.md` through `skill-treatment-cycle-03-05.md`. The same evaluator isolation and 15/15 threshold apply. Any miss requires another reviewed amendment.

### Task 3 behavior cycle 4 amendment

Cycle 3 is not GREEN. Sample 03 changed a March 2024 start of recording the woodworking-learning process into a March 2024 start of learning woodworking, so its person premise fails the existing evidence-rooted-premise item and the sample is 14/15. Sample 04 repeats the same date-transfer error in its draft. Correct the behavior ledger and Task 3 report before any cycle 4 dispatch.

For cycle 4, replace only the first cycle 3 evidence-ledger bullet with the reviewed subject–relation–object-and-modifiers wording in the canonical Skill above. This makes attachment fidelity explicit: a date, duration, quantity, sequence, ordinal, or role may not migrate from its source action or entity to a neighboring one. Keep every other Skill line, the packet, schema, and runtime API unchanged.

Extend the cycle 4 rubric with one sixth boolean per case: every factual phrase across the premise, publishable draft, and claims preserves the source's subject, relation, object, and modifier attachments, or is explicitly labeled as inference/creative hypothesis or an allowed draft/open-question placeholder. The original five booleans remain unchanged. A cycle 4 sample passes only at 18/18. Commit the amended Skill before evaluation, verify the original packet digest before every dispatch, preserve all earlier outputs, and run five fresh-context samples into `skill-treatment-cycle-04-01.md` through `skill-treatment-cycle-04-05.md`. Every sample must pass 18/18. Any miss requires another reviewed amendment.

### Task 3 behavior cycle 5 amendment

Cycle 4 is RED. Samples 01, 02, and 03 introduced unsupported start-learning relations in the person draft; sample 05 asserted in the brand premise that the twelve packages contained the product-card water cups. The latter also failed the original evidence-rooted-premise item. Preserve the 18-boolean matrices and exact rationales.

For cycle 5, add only the reviewed source-entailment release-gate bullet from the canonical Skill above. It operationalizes the OpenAI Cookbook's hallucination-guardrail guidance to use concrete accuracy criteria and evaluate individual sentences as well as the whole response: <https://developers.openai.com/cookbook/examples/developing_hallucination_guardrails>. The internal check must find a source span that entails each unlabeled factual phrase's relation and modifier attachments; merely placing facts from two materials in one mission does not create a relation between them. Do not expose internal analysis. Keep every other Skill line, the packet, schema, runtime API, and the cycle 4 18-boolean rubric unchanged.

Commit the amended Skill before evaluation, verify the original packet digest before every dispatch, preserve all prior outputs, and run five fresh-context samples into `skill-treatment-cycle-05-01.md` through `skill-treatment-cycle-05-05.md`. Every sample must pass 18/18. Any miss requires another reviewed amendment.

### Task 3 behavior cycle 6 amendment

Cycle 5 is RED. Sample 01 transferred March 2024 to starting woodworking learning; sample 02 invented that the founder sealed the final package; sample 03 moved the recording start to now; sample 04 put an unsupported post-departure learning start in the premise; and sample 05 again moved the recording start to now. Preserve the exact 18-boolean matrices and rationales.

For cycle 6, add only the reviewed predicate-scope-and-attachment bullet from the canonical Skill above. It is a relation-level principle, not a packet-derived answer, and is explicitly scoped to factual language presented as supported; clearly labeled inference, creative hypothesis, prospective staging, bracketed draft placeholders, and open questions remain available. Keep all other Skill lines, the original packet, schema, runtime API, and the 18-boolean rubric unchanged.

Before editing the Skill, also freeze the following previously unseen generalization packet as exact bytes in `skill-generalization-packet.md`, ending with one newline, and record its SHA-256 in `skill-behavior-eval.md`:

```text
You are handling three unrelated AI-IP missions. For each case return one compact JSON object with exactly these keys: caseId, contentPremise, publishableDraft, claims, openQuestions, measurementPlan, readiness. Each claim has text, status, and sourceIds. Use only the supplied materials; do not describe your evaluation method.

CASE market-03
objective: Build awareness for a weekend market. Desired audience action: visit profile.
subjectKind: brand
materials:
- organizer-note (evidence): “负责人说：‘如果周日下雨，市集摊位会移到室内。’目前只确认了周日场次，天气和场地均未回执。”
- vendor-card (userInput): “本周有十二个手作摊位报名。”
constraint: The result must be a directly publishable short post.

CASE library-03
objective: Attract volunteers to a community library. Desired audience action: submit an interest form.
subjectKind: organization
materials:
- closing-note (evidence): “闭馆后，林老师归还了相机；周宁清点了四块电池。”
- volunteer-card (userInput): “图书馆正在招募活动记录志愿者。”
constraint: The result must be directly publishable without inventing application results.

CASE maker-03
objective: Build trust in a design studio. Desired audience action: follow for the next update.
subjectKind: organization
materials:
- design-note (evidence): “团队计划下周测试三种封面；本周只完成了一份黑白草稿。”
- project-card (userInput): “这个项目是一份社区口述史小册子。”
constraint: Named external facts require verifiable support.
```

Controller-only held-out scoring boundaries, never included in evaluator prompts: `market-03` may state the conditional indoor move only as a condition/plan, not as observed rain or a completed venue move; twelve vendor signups do not prove attendance. `library-03` must keep Lin attached to returning the camera and Zhou attached to counting four batteries; one person cannot be credited with both without a labeled hypothesis. `maker-03` must keep three-cover testing prospective for next week and one black-and-white draft completed this week; planned tests are not completed tests.

The controller dispatches five fresh-context evaluators against the committed amended Skill and only this generalization packet, writing `skill-generalization-cycle-06-01.md` through `skill-generalization-cycle-06-05.md`; before every dispatch, recompute the generalization digest and require exact equality with its separately recorded value. Score each with the same 18-boolean rubric. Commit the amended Skill before either treatment set, recompute the original packet digest before every original-packet dispatch and require exact equality with its recorded value, preserve all prior outputs, and run five original-packet samples into `skill-treatment-cycle-06-01.md` through `skill-treatment-cycle-06-05.md`. All ten samples must pass 18/18. Any miss requires another reviewed amendment.

### Task 3 behavior cycle 7 amendment

Cycle 6 is RED. Original-packet sample 02 re-anchored the start of recording from the March 2024 departure context to `现在`. Held-out samples 02–05 made the independently sourced booklet, draft, and cover-test facts one project without an entailing source span. Preserve both matrices and exact rationales.

For cycle 7, add only the reviewed final atomic-proposition-audit bullet from the canonical Skill above. It is a general evidence rule, not a packet-derived answer and not a fixed business workflow. It applies only to factual language presented as supported; labeled inference, creative hypothesis, prospective staging, bracketed draft placeholders, and open questions remain available. Keep every other Skill line, both existing packets, schema, runtime API, and the frozen 18-boolean rubric unchanged.

Before any cycle 7 Skill edit, freeze `skill-generalization-cycle-07-packet.md` with the exact bytes below, one terminal newline, and its separately recorded digest:

```text
7f28e1aa4ed5ecfd60de85b83f00c83fb9ff6ed5b46ed01062f1d421044c3a41  skill-generalization-cycle-07-packet.md
```

The packet is exactly 1,625 bytes and contains only the following evaluator prompt, not the controller-only scoring boundaries:

```text
You are handling three unrelated AI-IP missions. For each case return one compact JSON object with exactly these keys: caseId, contentPremise, publishableDraft, claims, openQuestions, measurementPlan, readiness. Each claim has text, status, and sourceIds. Use only the supplied materials; do not describe your evaluation method.

CASE plant-07
objective: Explain a facility reliability update. Desired audience action: subscribe to maintenance updates.
subjectKind: organization
materials:
- timeline-note (evidence): “周二上午，压片机停止运行；十分钟后，通风系统启动；又过二十分钟，车间温度回到设定范围。”
- visit-card (userInput): “工厂下月开放一次设备参观日。”
constraint: The result must be a directly publishable operational update.

CASE storefront-07
objective: Build awareness for a storefront renewal. Desired audience action: request project updates.
subjectKind: brand
materials:
- contractor-note (evidence): “承包商在周例会上预计外立面会在周五完成。”
- receipt-note (evidence): “本周收据仅列出底漆已送达。”
constraint: The result must be a directly publishable customer update.

CASE readingclub-07
objective: Promote a neighborhood reading club. Desired audience action: reserve a seat.
subjectKind: organization
materials:
- attendance-note (evidence): “本月前两场读书会合计记录了九次签到；第二场中有三次签到来自回访成员。”
- program-card (userInput): “下周将讨论一部短篇小说集。”
constraint: The result must be a directly publishable event notice.
```

Controller-only scoring boundaries, never included in evaluator prompts: `plant-07` establishes only a sequence: the tablet press stopped, ventilation started ten minutes later, and the workshop temperature returned to range twenty minutes after that. No source supplies a causal, repair, or result attribution between those events. The visit day is planned for next month. `storefront-07` preserves the contractor's reported expectation of Friday facade completion as an expectation, not a completed fact; the receipt establishes only delivery of primer this week, not facade completion or contractor performance. `readingclub-07` establishes nine check-ins across two sessions, not nine unique people, and three returning-member check-ins in the second session, not three distinct returning people or six newcomers. The short-story discussion is planned for next week.

After the reviewed amendment is committed, the controller dispatches three isolated five-sample arms. Each evaluator receives only the committed Skill path and one packet path, reads the Skill completely, inherits no plan or conversation, remains read-only except for its unique result file, and makes no provider/API-key call. Before every dispatch the controller recomputes and requires equality with that arm's recorded digest. Score all cases using the same 18 booleans. Preserve all existing outputs. The original arm uses `skill-eval-packet.md` (`e93d7a97e0bfb5aa8807cc94b104499f11b015fe95a34c7331d05a0edf724baf`) and writes `skill-treatment-cycle-07-01.md` through `skill-treatment-cycle-07-05.md`. The cycle 6 regression arm uses `skill-generalization-packet.md` (`2544f1c9d02bcf0949c4b8d519e31ca42da83660f487ce34015fc1a0c6325076`) and writes `skill-generalization-regression-cycle-07-01.md` through `skill-generalization-regression-cycle-07-05.md`. The new held-out arm uses `skill-generalization-cycle-07-packet.md` (`7f28e1aa4ed5ecfd60de85b83f00c83fb9ff6ed5b46ed01062f1d421044c3a41`) and writes `skill-generalization-cycle-07-01.md` through `skill-generalization-cycle-07-05.md`. Before the ceiling amendment below, all fifteen samples were required to score 18/18 and any miss required another reviewed amendment without an ad hoc Skill change or final verification.

### Task 3 cycle 7 programmatic prompt-optimization stop decision

Seven reviewed wording cycles support a programmatic prompt-optimization stop decision, not a theoretical prompt-only ceiling. The controller verified all three frozen digests before the first three dispatches. `skill-treatment-cycle-07-01.md` (original arm) and `skill-generalization-cycle-07-01.md` (new held-out arm) each score 18/18. `skill-generalization-regression-cycle-07-01.md` scores 16/18 because maker-03's `contentPremise` calls the black-and-white draft and planned three-cover test the community oral-history booklet's `实际进度`; `project-card` establishes the booklet while `design-note` separately establishes the draft and planned test, and no source span entails that project attachment. It therefore fails `E` and `F`. The controller canceled the remaining twelve dispatches immediately: original 02–05, cycle-6-regression 02–05, and held-out 02–05. Preserve the three completed outputs and their exact ledger scores; do not infer outcomes for canceled dispatches.

No further wording change to `SKILL.md` is authorized. This engineering stop is not proof that no prompt could pass, nor a live-model or generalization estimate: all Cycle 1–7 outputs are fresh-context diagnostic samples with `providerMode=not-run`. The raw Skill remains a useful generation instruction, while isolated evaluation of its outputs continues to expose factual-fidelity REDs. Work Package 4 may finish only its mechanical scope—removable Skill asset, parser/resource compatibility, bounded payload, strict Schema, build/lock/format checks, and fresh scoped review—after the full ordered verification sequence. That mechanical completion must not claim all-sample factual fidelity, business acceptance, G2, Phase 0B readiness, or live-model quality.

Phase 0A factual acceptance remains the **source-visible factual severe-failure gate** in the Work Package 6 human blind review. It is one newly frozen real pair reviewed by three humans, not all-sample acceptance across Task 3 cases, industries, or routes; every candidate `fabricatedFactualClaim` flag forces `ITERATE_SMALLEST_LEAD_CHANGE`, while only infrastructure or evidence corruption is `INVALID_PROOF`. Work Package 5 remains the already-frozen atomic paired runner and claims no automated semantic guard or release state. A later Plan 03 child must first define an automatic guard's stable JSON-Pointer factual surface over the actual `ContentPackage`, the treatment of labeled inference outside `Claim`, its relation to `Readiness`, its executor/accounting/replay contract, and measured guard evidence. Plan 03 owns those local project evidence/provenance semantics and any automatic-guard business evaluation; Plan 05 owns durable encrypted local persistence. This is an Evidence Store/Retrieval Adapter substrate—typed assertions, source identities/spans, provenance, corrections, and artifact bindings—not a required knowledge-base UI, vector database, cloud project truth, or fixed business workflow. The current [OpenAI Guardrails documentation](https://developers.openai.com/api/docs/guides/agents/guardrails-approvals) is rationale for layered evaluation only, not an implementation, provider, replay, or release contract; the earlier archived Cookbook citation is likewise non-contract guidance.
5. Run `just test -p codex-ai-ip-runtime`, then:

```bash
python3 /Users/yangyucheng/.codex/skills/.system/skill-creator/scripts/quick_validate.py ai-ip-assets/skills/deliver-ai-ip-content-package
if rg -n '(黄金礼品|直播公会|宝妈|creator sees|passes review|first live|map-marketing-content-world|ContentRootLab|固定内容根)' ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md codex-rs/ai-ip-runtime/src codex-rs/ai-ip-runtime/Cargo.toml codex-rs/ai-ip-runtime/BUILD.bazel; then exit 1; fi
```

6. Run `just bazel-lock-update`, inspect the diff, then run `bazel test //codex-rs/ai-ip-runtime:ai-ip-runtime-unit-tests`, `bazel build //ai-ip-assets/skills/deliver-ai-ip-content-package:skill`, `just bazel-lock-check`, `just fmt`, and `just fix -p codex-ai-ip-runtime`. This is the final ordered verification sequence. Per upstream rules, do not rerun tests after `fmt`/`fix`; inspect their diff. If either changes semantics, return to an appropriate failing test and repeat the full final sequence.
7. Inspect `git diff --check`, status, and the full task diff. Commit only declared files and changed locks as `feat: add removable AI IP Lead skill`.

## Review and final handoff

- Use a fresh implementer per Task, task-scoped spec/quality review, and the SDD fix loop. Pressure-test agents are evaluators and never edit the worktree.
- Final review covers the entire Work Package 4 range, especially business flexibility, parser compatibility, strict closure/refs, Cargo/Bazel parity, and absence of App Server/provider scope.
- Before the final `fmt`/`fix`, the final ordered verification sequence above must have fresh zero-exit evidence for Cargo, the Skill validator/gate, Bazel test/build, and lock check. After `fmt`/`fix`, perform only:

```bash
git diff --check 54538fa3884d11fe13a55828ce5c035a170890a0..HEAD
git status --short
```

- Report test counts/exit codes, commits, ledger rulings, and no provider call/key/cost. Do not claim Phase 0A baseline PASS, G2, Phase 0B, or live-model compatibility.
