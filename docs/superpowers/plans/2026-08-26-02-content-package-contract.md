# Phase 0A Work Package 3 — Pure ContentPackage Contract

> **Execution:** Use `superpowers:test-driven-development`, `superpowers:subagent-driven-development`, and `superpowers:verification-before-completion`.

**Goal:** Land the first business-owned vertical foundation: a provider-neutral, runtime-independent Rust domain crate that validates an evidence-aware, directly usable `ContentPackage` for one Mission.

**Source authorities:**

- `docs/superpowers/specs/2026-08-24-ip-agent-saas-design.md`
- `docs/superpowers/specs/2026-08-25-phase-0a-codex-business-proof-build-spec.md`, Work Package 3
- `docs/superpowers/specs/2026-08-25-domestic-model-adaptation-design.md`
- `docs/architecture/2026-08-25-legacy-six-version-decision-memory.md`
- `docs/architecture/codex-fork-patch-ledger.md`

**Business-priority exception:** The macOS native runtime captures for protocol, transport, state, build, metadata, licenses, dependencies, and assets passed, but the current public-evidence verifier cannot accept raw cargo-nextest selection JSON. The preserved Task 3 evidence proves two tooling defects: legal non-selected leaves include `filter-match.reason`, and raw selection metadata contains local Rust sysroot paths plus unrelated test names that trip the public forbidden scan. This plan does not call that baseline `PASS`, does not write F-0003, and does not authorize Phase 0B. Per the user's explicit business-first and speed direction, the isolated pure-domain child may proceed while the evidence-tool repair remains a separate follow-up. No existing Codex runtime behavior is changed by this child.

**Base:** `ec655cd87fb1bfbf9dc68163aab610a6d5e327c9`

**Provider boundary:** `providerMode=not-run`; `paidProviderCost=0`; do not locate or read an API key.

## Scope

Create:

- `codex-rs/ai-ip-domain/Cargo.toml`
- `codex-rs/ai-ip-domain/BUILD.bazel`
- `codex-rs/ai-ip-domain/src/lib.rs`
- `codex-rs/ai-ip-domain/src/mission.rs`
- `codex-rs/ai-ip-domain/src/content_package.rs`
- `codex-rs/ai-ip-domain/src/domain_tests.rs`

Modify only as mechanically required:

- `codex-rs/Cargo.toml`
- `codex-rs/Cargo.lock`
- `MODULE.bazel.lock`

Do not modify App Server, Codex core, provider code, prompts, Skills, the command matrix, evidence tooling, or any preserved evidence/quarantine/bundle/ref.

## Domain contract

### Mission input

- `SubjectKind`: `Person`, `Brand`, `Product`, `Organization`, `Hybrid`.
- `MaterialKind`: `UserInput`, `Evidence`, `ActualResultReceipt`, `Other`.
- `MissionMaterial`: stable `material_id`, relative `relative_path`, lowercase SHA-256, and `material_kind`.
- `HeldOutMissionCase`: stable `case_id`, nonempty `objective`, `subject_kind`, bounded `constraints`, and bounded `materials`.
- The exact public input shape is:

```rust
pub enum SubjectKind { Person, Brand, Product, Organization, Hybrid }
pub enum MaterialKind { UserInput, Evidence, ActualResultReceipt, Other }

pub struct MissionMaterial {
    pub material_id: String,
    pub relative_path: String,
    pub sha256: String,
    pub material_kind: MaterialKind,
}

pub struct HeldOutMissionCase {
    pub case_id: String,
    pub objective: String,
    pub subject_kind: SubjectKind,
    pub constraints: Vec<String>,
    pub materials: Vec<MissionMaterial>,
}
```
- Centralize these exact input bounds in `mission.rs`: `MAX_CASE_ID_BYTES = 128`, `MAX_MATERIAL_ID_BYTES = 128`, `MAX_MATERIALS = 128`, `MAX_CONSTRAINTS = 64`, `MAX_OBJECTIVE_BYTES = 8 * 1024`, `MAX_CONSTRAINT_BYTES = 2 * 1024`, and `MAX_MATERIAL_PATH_BYTES = 1024`.
- IDs use the internal grammar `[A-Za-z0-9][A-Za-z0-9._-]*`, must fit their byte limit, and may not start with the neutral reserved namespace `__ai_ip_dev__`. This namespace never contains or derives from a frozen case name, industry, answer, or fixture body.
- Reject empty/oversized strings, duplicate material IDs, SHA-256 values other than exactly 64 lowercase hexadecimal bytes, absolute paths, platform prefixes/root components, `.`/`..`, and reserved development IDs. Validation is pure and never reads files.

### Output

- The exact public structs are:

```rust
pub struct InfluenceRelation {
    pub subject: String,
    pub subject_kind: SubjectKind,
    pub audience: String,
    pub desired_action: String,
}

pub struct ActionFunnelStep {
    pub audience_state: String,
    pub intended_change: String,
    pub next_action: String,
}

pub struct PublishableContent {
    pub format: String,
    pub title: Option<String>,
    pub body: String,
    pub production_notes: Vec<String>,
}

pub struct Claim {
    pub text: String,
    pub status: ClaimStatus,
    pub source_refs: Vec<String>,
    pub result_receipt_ref: Option<String>,
}

pub struct MeasurementPlan {
    pub success_signal: String,
    pub collection_method: String,
    pub observation_window: String,
}

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
```

- `ContentPackage` validates every required string as nonempty, requires one or more action-funnel steps, and requires a nonempty publishable body. `InfluenceRelation` explicitly represents subject, `SubjectKind`, audience, and desired action. Purchase is not a default or required action.
- `Readiness` has exactly `Draft`, `ReadyForHumanReview`, and `BlockedByMissingEvidence`; there is no `Published` variant or field that claims this turn performed publication.
- `ClaimStatus` has exactly `UserFact`, `ExternalEvidence`, `ModelInterpretation`, `CreativeHypothesis`, `Unknown`, and `ActualResult`.
- The exact enums are `pub enum Readiness { Draft, ReadyForHumanReview, BlockedByMissingEvidence }` and `pub enum ClaimStatus { UserFact, ExternalEvidence, ModelInterpretation, CreativeHypothesis, Unknown, ActualResult }`.
- `Claim` has text, `source_refs`, and nullable `result_receipt_ref`. `UserFact`, `ExternalEvidence`, and `ActualResult` each require at least one nonempty source ref; `ActualResult` additionally requires a nonempty receipt ref. `result_receipt_ref` must be absent for every other status. Model interpretation, creative hypothesis, and unknown may be unsourced but never become evidence by readiness alone.
- `validate_against(&HeldOutMissionCase)` requires matching subject kind and same-case material IDs. `UserFact` refs resolve only to `UserInput`; `ExternalEvidence` refs resolve only to `Evidence`; `ActualResult.result_receipt_ref` resolves to `ActualResultReceipt`; cross-case-looking or unknown refs fail. File existence/digest recomputation belongs to the later runner, not this crate.
- Every DTO uses `#[serde(deny_unknown_fields, rename_all = "camelCase")]`; every enum serializes camelCase; public DTOs derive `Serialize`, `Deserialize`, `JsonSchema`, `Clone`, `Debug`, `PartialEq`, and `Eq` where valid.
- Readiness has only these minimal semantic checks: `ReadyForHumanReview` is invalid while `open_questions` is nonempty or any claim is `Unknown`; `BlockedByMissingEvidence` is invalid unless at least one open question or `Unknown` claim exists; `Draft` permits either state. This is not a workflow gate and does not require a fixed funnel, evidence count, CTA, or provider.
- Both `HeldOutMissionCase::validate()` and `ContentPackage::validate()` return `Result<(), ValidationErrors>`. `ContentPackage::validate_against(&HeldOutMissionCase)` first performs both intrinsic validations, then the same-case kind/ref checks. `ValidationErrors` owns `Vec<String>`, exposes `errors(&self) -> &[String]`, implements `Display`/`Error`, and appends messages in field/declaration order so multiple failures are deterministic. Keep API surface small and modules below the upstream size guidance.

### Complete crate/build skeleton

`codex-rs/ai-ip-domain/Cargo.toml`:

```toml
[package]
edition.workspace = true
license.workspace = true
name = "codex-ai-ip-domain"
version.workspace = true

[lib]
doctest = false
name = "codex_ai_ip_domain"
path = "src/lib.rs"

[lints]
workspace = true

[dependencies]
schemars = { workspace = true }
serde = { workspace = true, features = ["derive"] }

[dev-dependencies]
pretty_assertions = { workspace = true }
serde_json = { workspace = true }
```

`codex-rs/ai-ip-domain/BUILD.bazel`:

```starlark
load("//:defs.bzl", "codex_rust_crate")

codex_rust_crate(
    name = "ai-ip-domain",
    crate_name = "codex_ai_ip_domain",
)
```

`lib.rs` privately declares `mission` and `content_package`, explicitly re-exports only the DTOs, enums, constants, and `ValidationErrors`, and declares `#[cfg(test)] #[path = "domain_tests.rs"] mod tests;`.

## Task 1 — RED tests and crate skeleton

1. Read the five source authorities and root `AGENTS.md`; write `.superpowers/sdd/2026-08-26-02-content-package-contract/progress.md` with a short reuse/prohibition note. Preserve one Lead, six evidence states, direct action relation, bounded artifacts, and provider neutrality; prohibit fixed agent rosters, giant semantic maps, named-case answers, fixed funnels, and provider/model branches.
2. Add the complete Cargo/Bazel skeleton above, add `"ai-ip-domain"` to `codex-rs/Cargo.toml` workspace members, create empty implementation modules, and add `lib.rs` module/test declarations. Do not update locks yet.
3. In `domain_tests.rs`, add `valid_case()` and `valid_package(SubjectKind)` fixtures containing only neutral synthetic values such as `case-1`, `material-1`, `evidence/a.txt`, and `报名参加志愿活动`; do not use any frozen evaluation case or expected answer.
4. Add the six exact build-spec tests: `organization_subject_can_target_a_membership_action`, `external_evidence_without_source_is_rejected`, `actual_result_without_receipt_is_rejected`, `claim_reference_must_resolve_inside_the_same_case`, `actual_result_receipt_must_have_the_declared_receipt_kind`, and `held_out_material_paths_cannot_escape_case_root`.
5. Add table-driven tests named `subject_and_evidence_enums_use_camel_case_wire_names`, `unknown_dto_fields_are_rejected`, `mission_bounds_accept_limit_and_reject_one_byte_over`, `ids_and_material_digests_are_strict`, `material_paths_are_relative_normal_paths`, and `duplicate_material_ids_are_rejected`. The boundary table covers every exact constant above at limit and one byte over; path cases cover Unix absolute, Windows prefix, root, `.`, and `..` components.
6. Add tests named `content_package_requires_every_business_section`, `user_fact_requires_user_input_source`, `external_evidence_requires_evidence_source`, `actual_result_requires_declared_source_and_receipt`, `non_actual_claim_rejects_result_receipt`, `subject_kind_must_match_case`, `unknown_and_cross_case_refs_are_rejected`, `ready_for_review_rejects_missing_evidence_markers`, `blocked_readiness_requires_a_missing_evidence_marker`, and `validation_aggregates_errors_in_field_order`. Assert the complete `ValidationErrors.errors()` vector where practical.
7. Run from `codex-rs`: `just test -p codex-ai-ip-domain`. Expected RED is a compile failure for the deliberately unimplemented public domain symbols/methods used by these tests. Record command, exit, and representative missing symbol; do not accept a missing tool/workspace member/dependency failure as RED.

## Task 2 — Minimum implementation and GREEN

1. Implement the exact mission DTOs/constants/validators first. Run `just test -p codex-ai-ip-domain mission -- --nocapture` and make only mission tests GREEN.
2. Implement the exact output DTOs, `ValidationErrors`, intrinsic validation, readiness rules, and `validate_against`. Run `just test -p codex-ai-ip-domain` to GREEN. No I/O, async, database, Codex runtime, provider, model, prompt, hidden case answer, or workflow engine.
3. From the repository root run `just bazel-lock-update` and review that only `MODULE.bazel.lock` changes beyond Cargo files. Then run `bazel test //codex-rs/ai-ip-domain:ai-ip-domain-unit-tests`, followed by `just bazel-lock-check`, in that order.
4. From `codex-rs` run `just fmt`, review its diff, then `just fix -p codex-ai-ip-domain` as required by upstream. Do not rerun the same passing tests after mechanical fmt/fix unless fix changes semantics.
5. Run `git diff --check`; verify touched paths are exactly the nine authorized paths; confirm the failed-baseline quarantine digest/path, transfer refs, old/new bundles, and provider disposition are unchanged.
6. Write `.superpowers/sdd/2026-08-26-02-content-package-contract/task-1-report.md` with RED/GREEN commands, counts, locks, fmt/fix, exact touched paths, reuse/prohibition note, and zero-provider claims.
7. Commit exactly the authorized paths with `git commit -m "feat: add AI IP content package contracts"`.

## Task 3 — Independent review

- Review correctness, business usefulness, provider neutrality, serde/schema shape, path/reference validation, error aggregation, Bazel/Cargo integration, scope, and regression evidence.
- Critical or Important findings block completion and return to the original implementer for up to three focused fix rounds.
- On PASS, record exact commit, focused/Bazel results, touched paths, and `providerMode=not-run` in the ignored SDD report. Do not claim a published result, business-quality win, baseline PASS, G2, or Phase 0B authorization.

## Acceptance

- A caller can construct and validate one evidence-aware `ContentPackage` without Codex core or a provider.
- Organization subjects can target membership or recruitment actions without being coerced into purchase.
- Facts, evidence, interpretations, creative hypotheses, unknowns, and actual results remain distinct.
- Missing evidence yields a usable draft/blocking status instead of fabricating facts or publication.
- `just test -p codex-ai-ip-domain`, Bazel unit tests, lock checks, formatting, and scoped lint pass on the committed implementation.
