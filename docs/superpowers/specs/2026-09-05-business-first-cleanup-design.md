# Business-first cleanup — approved scope

The user approved this retirement in chat: “删吧，旧的不去，新的不来”. This records that approved scope, not a new product proposal.

## Outcome

Remove the heavy evaluation implementation and its prerequisites from active development. Keep Codex's native execution substrate and the small existing business primitives. Do not build a replacement evaluator, security system, or new business feature in this change.

## Retire

- `codex-rs/ai-ip-eval/`, `scripts/ai_ip/eval_lab/`, and `ai-ip-evals/`, including implementation-only tests, schemas, Promptfoo integration, and manifests.
- The seven foundation evaluation files: `capture_command.py`, `required_command_matrix.json`, `run_frozen_eval.py`, `test_capture_command.py`, `test_run_frozen_eval.py`, `test_verify_evidence.py`, and `verify_evidence.py`. Keep source provenance and its existing tests.
- Evaluator-only extensions to `codex-rs/responses-api-proxy/`: restore this crate to locked upstream `4ef1d4b89bd419c976b04fefa0fd36844e898340`, preserving its original CLI and credential handling, rather than deleting the native proxy.
- Cargo/Bazel dependency references and current execution instructions that require the retired implementations.
- Repeated ledger/entailment/atomic-proposition audit paragraphs in the Lead Skill. Preserve its existing business judgment, evidence/inference distinction, useful drafting, and no-fabricated-results guidance. This is removal of requirements, not a claim that marketing quality has improved.

## Preserve

- Native Codex core/state/model/login/sandbox behavior; existing useful App Server descendant notifications and business-output integration tests.
- `codex-rs/ai-ip-domain/`, `codex-rs/ai-ip-runtime/`, their remaining behavioral tests, and `ai-ip-assets/skills/deliver-ai-ip-content-package/`.
- Move the golden-gift blueprint, legacy source-import manifest, golden-gift rubric, and content-package business rubric to `docs/architecture/business-reference/` unchanged. They remain historical/manual calibration references, not production prompt material or certified evaluator authority. No synthetic answers are represented as real business evidence.
- Six-version decision memory, main product architecture and domestic-model direction, source lock, licenses, patch history, and original external case/material locations.
- All untracked/ignored user files, credentials, media, external caches, other worktrees, and prior SDD workspaces.

## Development authority

Retire the mandatory Phase 0A/06A/06B/07A/07B proof-completion dependency before internal business development. Mark historical evaluator plans/specs clearly retired; update the master roadmap and plan index so they cannot silently reactivate those tasks. Preserve records of previous results. Retirement means `RETIRED_NOT_PASSED`, never business PASS, model qualification, or release authorization. Real spending, publication, customer-data and customer-release responsibilities are not waived. No replacement mandatory gate is added.

## Recovery and acceptance

Recovery branch: `codex/pre-business-cleanup-20260905` at `04b3e8bee6c2d0d23b152654eeeadc7011f0a66c`. Cleanup executes on `codex/business-first-cleanup` in the existing linked worktree. Do not merge, push, prune worktrees, or delete caches in this task.

Acceptance: retired implementations are absent from tracked active code/build targets; preserved reference bytes match the recovery commit; native proxy matches locked upstream; remaining business crates still pass available focused tests; lockfiles are regenerated using repository tooling; final review reports concrete remaining validation limits. Existing broken host/build infrastructure is reported separately, not repaired by inventing another subsystem.
