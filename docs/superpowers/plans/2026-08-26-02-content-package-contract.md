> **HISTORICAL_COMPLETED — NOT EXECUTABLE (2026-09-05).** This completed provenance/business-primitive record is preserved as history. Its commands, follow-on authorizations and proof prerequisites are superseded by the approved [business-first cleanup](../specs/2026-09-05-business-first-cleanup-design.md) and current [roadmap](2026-08-25-00-codex-ai-ip-master-roadmap.md). Retained implementation remains; the old Phase 0A/06A/06B/07A/07B gates are `RETIRED_NOT_PASSED`. No historical child is executable and no business PASS or model/customer-release qualification is claimed.

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
- Paths use `/` as the only separator. Reject empty/oversized strings, duplicate material IDs, SHA-256 values other than exactly 64 lowercase hexadecimal bytes, a leading `/`, any `\\`, empty segments, and `.`/`..` segments. This also rejects Windows drive/UNC spellings consistently on every host. Validation is pure and never reads files.

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

The error carrier is exactly:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationErrors {
    errors: Vec<String>,
}

impl ValidationErrors {
    pub fn errors(&self) -> &[String] { &self.errors }
}
```

`Display` joins messages with `"; "`. Validators build one local `Vec<String>` and return `Ok(())` when empty or `Err(ValidationErrors { errors })` otherwise; no fail-fast `?` is used inside one intrinsic validation pass. Error text and order are frozen by this catalog:

1. Mission: `caseId: invalid internal id`, `caseId: reserved development namespace`, `objective: must not be empty`, `objective: exceeds 8192 bytes`, `constraints: exceeds 64 items`, then for each index `constraints[{i}]: must not be empty|exceeds 2048 bytes`, `materials: exceeds 128 items`, then for each material in input order `materials[{i}].materialId: invalid internal id|duplicate`, `.relativePath: must not be empty|exceeds 1024 bytes|must be a normalized relative path`, `.sha256: must be 64 lowercase hexadecimal characters`.
2. Package intrinsic: empty-field messages, in declaration order, are `missionSummary: must not be empty`, `influenceRelation.subject: must not be empty`, `.audience: must not be empty`, `.desiredAction: must not be empty`, `actionFunnel: must contain at least one step`, then `actionFunnel[{i}].audienceState|intendedChange|nextAction: must not be empty`, `strategicJudgment: must not be empty`, `publishableContent.format: must not be empty`, `publishableContent.title: must not be empty when present`, `publishableContent.body: must not be empty`, `publishableContent.productionNotes[{i}]: must not be empty`, `claims[{i}].text: must not be empty`, `claims[{i}].sourceRefs[{j}]: must not be empty`, `openQuestions[{i}]: must not be empty`, and `measurementPlan.successSignal|collectionMethod|observationWindow: must not be empty`. Claim semantic messages then append as `claims[{i}].sourceRefs: <status> requires at least one source`, `claims[{i}].resultReceiptRef: actualResult requires a nonempty receipt`, or `claims[{i}].resultReceiptRef: only actualResult may set a receipt`; readiness messages append last as `readiness: readyForHumanReview cannot contain missing-evidence markers` and `readiness: blockedByMissingEvidence requires a missing-evidence marker`.
3. Against-case messages append only after intrinsic case/package errors are absent: `influenceRelation.subjectKind: does not match mission case`, `claims[{i}].sourceRefs[{j}]: unknown material id`, `...: userFact requires userInput material`, `...: externalEvidence requires evidence material`, `claims[{i}].resultReceiptRef: unknown material id`, or `...: requires actualResultReceipt material`.

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
2. Add the complete Cargo/Bazel skeleton above, add `"ai-ip-domain"` to `codex-rs/Cargo.toml` workspace members, create `mission.rs`, and make `lib.rs` expose only the mission API plus `domain_tests.rs`. Do not declare the content module or reference content symbols yet. Do not update locks yet.
3. In `domain_tests.rs`, add `valid_case()` with only neutral synthetic values `case-1`, `material-1`, and `evidence/a.txt`. Add mission-only tests: all SubjectKind/MaterialKind camelCase wire values; HeldOutMissionCase unknown-field rejection through `serde_json`; every constant at limit and one byte over; empty objective/constraint/path; ID grammar and `__ai_ip_dev__` namespace; material count/duplicate IDs; exact lowercase digest; Unix absolute, Windows-prefix-like (`C:\\secret`), root, `.`, and `..` path components; and a multi-error assertion equal to the catalog order.
4. Leave the mission public types/constants/method bodies absent but keep the crate, member, and direct dependencies valid. Run from `codex-rs`: `just test -p codex-ai-ip-domain`. Expected RED is unresolved mission imports/methods in `domain_tests.rs`; missing tool/member/dependency is not an acceptable RED.
5. Implement the exact mission public API and mission validation. Run the entire currently declared crate test target `just test -p codex-ai-ip-domain`; all mission tests must GREEN before adding any content symbol/test.
6. Only after mission GREEN, declare `content_package`, export its exact API, add `valid_package(SubjectKind)`, and add the six exact build-spec tests: `organization_subject_can_target_a_membership_action`, `external_evidence_without_source_is_rejected`, `actual_result_without_receipt_is_rejected`, `claim_reference_must_resolve_inside_the_same_case`, `actual_result_receipt_must_have_the_declared_receipt_kind`, and `held_out_material_paths_cannot_escape_case_root` (the last reuses the already-GREEN mission API).
7. Add content tests: all ClaimStatus/Readiness camelCase wire values; ContentPackage unknown-field rejection; each required section/string empty in a table; empty funnel; `UserFact`/`ExternalEvidence`/`ActualResult` empty source requirements; wrong material kinds; missing/wrong receipt; receipt on non-actual claim; subject-kind mismatch; unknown refs including colon-bearing cross-case-looking refs; both readiness boundary directions; and the exact aggregate-error vector from the catalog.
8. Keep all content public types/methods absent for this run, then run `just test -p codex-ai-ip-domain`. Expected second RED is unresolved content imports/methods while all mission symbols compile; record a representative content symbol failure.

Reference test shapes (expand table rows mechanically, do not weaken assertions):

```rust
#[test]
fn organization_subject_can_target_a_membership_action() {
    let package = valid_package(SubjectKind::Organization);
    assert_eq!(
        package.influence_relation.desired_action,
        "报名参加志愿活动"
    );
    package.validate().unwrap();
}

#[test]
fn user_fact_requires_user_input_source() {
    let case = valid_case();
    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].status = ClaimStatus::UserFact;
    package.claims[0].source_refs.clear();
    assert_eq!(
        package.validate_against(&case).unwrap_err().errors(),
        &["claims[0].sourceRefs: userFact requires at least one source"]
    );
}

#[test]
fn readiness_boundaries_are_explicit() {
    let mut package = valid_package(SubjectKind::Brand);
    package.readiness = Readiness::ReadyForHumanReview;
    package.open_questions = vec!["还缺少哪项证明？".into()];
    assert_eq!(
        package.validate().unwrap_err().errors(),
        &["readiness: readyForHumanReview cannot contain missing-evidence markers"]
    );
    package.open_questions.clear();
    package.readiness = Readiness::BlockedByMissingEvidence;
    assert_eq!(
        package.validate().unwrap_err().errors(),
        &["readiness: blockedByMissingEvidence requires a missing-evidence marker"]
    );
}
```

## Task 2 — Minimum implementation and GREEN

1. Starting from the second RED, implement `content_package.rs` exactly: DTO derives/serde attributes, error carrier, field-order aggregation, claim/readiness semantics, then `validate_against`. Reuse small private validation helpers from `mission.rs` only if they have at least two call sites; do not create public helpers.
2. Run the entire `just test -p codex-ai-ip-domain` target to GREEN. No I/O, async, database, Codex runtime, provider, model, prompt, hidden case answer, or workflow engine.
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
