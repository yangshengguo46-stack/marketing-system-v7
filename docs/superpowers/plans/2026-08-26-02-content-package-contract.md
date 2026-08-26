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
- Centralize input bounds in `mission.rs`: `MAX_MATERIALS = 128`, objective 8 KiB, one constraint 2 KiB, one path 1 KiB, plus explicit finite bounds for IDs and list counts.
- Reject empty/oversized strings, duplicate material IDs, invalid SHA-256, absolute paths, path prefixes/root components, `..`, and reserved development-fixture IDs. Validation is pure and never reads files.

### Output

- `ContentPackage` contains nonempty `mission_summary`, `influence_relation`, one or more `action_funnel` steps, `strategic_judgment`, directly usable `publishable_content`, `claims`, `open_questions`, `measurement_plan`, and `readiness`.
- `InfluenceRelation` explicitly represents subject, `SubjectKind`, audience, and desired action. Purchase is not a default or required action.
- `Readiness` has exactly `Draft`, `ReadyForHumanReview`, and `BlockedByMissingEvidence`; there is no `Published` variant or field that claims this turn performed publication.
- `ClaimStatus` has exactly `UserFact`, `ExternalEvidence`, `ModelInterpretation`, `CreativeHypothesis`, `Unknown`, and `ActualResult`.
- `Claim` has text, `source_refs`, and nullable `result_receipt_ref`. `ExternalEvidence` requires a source; `ActualResult` requires a source and nonempty receipt ref. Model interpretation and creative hypothesis may be unsourced but never become evidence by readiness alone.
- `validate_against(&HeldOutMissionCase)` requires matching subject kind and same-case material IDs. `UserFact` refs resolve only to `UserInput`; `ExternalEvidence` refs resolve only to `Evidence`; `ActualResult.result_receipt_ref` resolves to `ActualResultReceipt`; cross-case-looking or unknown refs fail. File existence/digest recomputation belongs to the later runner, not this crate.
- Every DTO uses `#[serde(deny_unknown_fields, rename_all = "camelCase")]`; every enum serializes camelCase; public DTOs derive `Serialize`, `Deserialize`, `JsonSchema`, `Clone`, `Debug`, `PartialEq`, and `Eq` where valid.
- `validate()` aggregates deterministic field errors instead of returning only the first error. Keep API surface small and modules below the upstream size guidance.

## Task 1 — RED tests and crate skeleton

1. Read the five source authorities and root `AGENTS.md`; record a concise reuse/prohibition note in the ignored SDD report. In particular: preserve one Lead, evidence states, direct action relation, bounded artifacts, and provider neutrality; prohibit fixed agent rosters, giant semantic maps, named-case answers, fixed funnels, and provider/model branches.
2. Create the crate/test skeleton with tests that reference the required API before implementation. Add the workspace member and only `serde`, `schemars`, and test-only `pretty_assertions` dependencies.
3. Tests cover all enum wire names, unknown-field rejection, the six key examples from the build spec, every input bound/path/digest/duplicate rule, nonempty ContentPackage sections, no empty funnel, all claim source-kind rules, subject-kind mismatch, cross-case/unknown refs, and aggregation of multiple errors.
4. Run `just test -p codex-ai-ip-domain` through the repository's required entry point and record the expected compile/test failure. Do not manufacture an unrelated failure.

## Task 2 — Minimum implementation and GREEN

1. Implement only the pure DTOs and validation needed by Task 1. No I/O, async, database, Codex runtime, provider, model, prompt, hidden case answer, or workflow engine.
2. Run focused `just test -p codex-ai-ip-domain` to GREEN.
3. Update Cargo/Bazel locks with the repository commands; run the Bazel unit target and lock check.
4. Run `just fmt`, review its diff, then `just fix -p codex-ai-ip-domain` as required by upstream. Do not rerun the same passing tests after mechanical fmt/fix unless fix changes semantics.
5. Run `git diff --check`, verify touched paths are exactly the authorized set, and confirm no evidence/ref/bundle/quarantine/provider mutation.
6. Commit `feat: add AI IP content package contracts`.

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
