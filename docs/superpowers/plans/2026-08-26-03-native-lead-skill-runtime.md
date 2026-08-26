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
- Before drafting, build an internal evidence ledger keyed by source ID. Preserve the source meaning of every entity, role, quantity, date, duration, sequence, and relationship; treat each as a constraint rather than a creative variable.
- Before returning, compare every factual phrase in the premise, draft, and claims against that ledger. Every factual element in the premise must map to supplied evidence; never use a placeholder there. If a draft or claim phrase does not map exactly, remove it, label it as an inference or creative hypothesis, or turn it into an explicit bracketed placeholder or open question.
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
