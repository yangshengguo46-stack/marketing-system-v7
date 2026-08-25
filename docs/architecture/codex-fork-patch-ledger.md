# Codex fork patch ledger

This append-only ledger explains why AI IP 1.1 diverges from the locked OpenAI Codex ancestor. A patch is not accepted merely because it compiles: it must identify the upstream seam, business reason, regression proof, sync risk, and removal or rollback path.

## Entry contract

Each later entry contains:

- stable entry ID and owning implementation-plan task;
- upstream base and exact fork commit;
- classification: **preserve**, **directly modify**, **replace**, or **add**;
- touched files and externally visible contracts;
- business result that requires the change;
- focused and integration tests with evidence locations;
- upstream-sync conflict surface;
- deletion or rollback procedure.

No entry may contain credentials, private cases, prompt or response bodies, reviewer mappings, or customer content.

## F-0001 — Establish source provenance

- Owner: Phase 0A.1 Codex Fork Provenance.
- Upstream base: `4ef1d4b89bd419c976b04fefa0fd36844e898340`.
- Product origin status: `unconfigured`.
- Classification: **preserve** upstream Git ancestry, `LICENSE`, root `AGENTS.md`, and upstream NOTICE; **modify** `.gitattributes` and `NOTICE`; **add** source-lock verification and fork governance.
- Touched seams: repository metadata and documentation only; `codex-rs/` is unchanged.
- Business reason: permit a deep business-first Codex fork without losing reproducible source attribution or silently falling back to DeerFlow.
- Regression proof: `scripts/ai_ip/foundation/test_verify_upstream_lock.py` and a clean-tree run of `scripts/ai_ip/foundation/verify_upstream_lock.py`.
- Upstream-sync risk: later upstream changes to `LICENSE`, `NOTICE`, root `AGENTS.md`, or `.gitattributes` require an explicit lock update and ledger entry; the verifier fails closed on silent drift.
- Rollback: remove only the AI IP provenance commit after first returning to the source-import merge commit; never rewrite or discard either imported ancestor.
