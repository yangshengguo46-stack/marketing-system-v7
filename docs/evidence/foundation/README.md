# Foundation evidence — historical record

> **RETIRED — RETIRED_NOT_PASSED (2026-09-05).** The frozen capture/evaluator matrix and its mandatory completion gates are retired by the approved business-first cleanup. No capture, wrapper, verifier or successor command below remains an execution instruction.

Existing public evidence files and their earlier PASS/BLOCKED/invalid-capture claims remain unchanged as history; retirement does not certify them, close their gaps or convert them into business PASS. The seven foundation evaluation tools and matrix are removed. `scripts/ai_ip/foundation/verify_upstream_lock.py` and `test_verify_upstream_lock.py` remain for source provenance.

Internal business development proceeds without rebuilding this proof system. Source lock, licenses and accurate reporting of retained tests still apply. Spending, publication, customer-data and customer-release responsibilities are unchanged. No live-model or business-quality claim is made.

## Historical capture contract — non-executable

The text below records the former contract; paths to removed tools are historical identifiers and must not be followed as current commands.

---

## Claims and non-claims

This directory records reproducible, public foundation evidence for the frozen candidate and the frozen tool tree. A `PASS` means the required command completed with its frozen expected exit status and its recorded bytes, platform, architecture, tested SHA, tools SHA, recorder SHA, matrix SHA, and lock hashes verify. `BLOCKED_BASELINE` means a required baseline command could not be completed and is recorded as such; it is not a pass and cannot be converted into one by later post evidence.

The evidence has no provider calls: `providerMode=not-run` and `paidProviderCost=0`. It makes no claim about business evaluation, model quality, provider reachability, user content, or credentials. Manifest host labels plus bootstrap hashes establish a reproducible provenance claim but cannot cryptographically prove that an untrusted remote machine is physical Windows.

## Directory layout

`macos-x86_64/` and `windows-11-x64/` contain platform-specific bootstrap records, command manifests, and their exact stdout/stderr logs. The command matrix has 30 entries: `fmt-check`, `app-server-protocol`, `app-server-transport`, `state`, `thread-store`, `app-server-process`, `thread-start`, `thread-resume`, `executor-skill`, `mcp-tool`, `process-exec`, `fs`, `apply-patch`, `build-cli-app-server`, `cargo-metadata`, `cargo-license-source`, `pnpm-dependencies`, `pnpm-licenses`, `source-assets`, `macos-sandbox`, `windows-sandbox-restricted`, `windows-sandbox-elevated`, `post-domain`, `post-runtime`, `post-eval`, `post-responses-proxy`, `post-descendant-raw`, `post-ai-ip-strict-output`, `post-bazel-rust`, and `post-bazel-assets`.

The canonical matrix is `scripts/ai_ip/foundation/required_command_matrix.json`; its digest is recorded with every manifest. The verifier recomputes the bytes of every public record rather than trusting filenames or prior status labels.

## Three-tree isolation

Capture uses three separate trees: a clean detached tested candidate tree, a clean detached tools tree, and an evidence tree that alone receives public evidence. The tested and tools Git SHAs are recorded independently; neither is inferred from the evidence tree HEAD. The frozen evaluator dispatch wrapper separately rechecks its detached evaluator worktree, exact candidate SHA, and selected binary digest before it directly executes that binary.

## Capture and create-new rules

Evidence is created in the evidence tree only. Bootstrap, manifests, and other generated records use the recorder's create-new and atomic publication rules; existing records are never overwritten in place. The only ignored files that may be staged are the exact generated `*.stdout.log` and `*.stderr.log` paths, and they are force-added only with an explicit `git add -f -- <exact-log-path>`.

The matrix command, expected exit code, platform, architecture, tested SHA, tools SHA, recorder SHA, matrix SHA, and lock hashes are bound into each command manifest. Capturing a command does not grant a caller authority to change any of those values through ambient Git configuration or a log pathname.

## Verification and status

Verification requires the fixed matrix and recalculates manifest and log digests. For each required baseline command it distinguishes a verified `PASS` from `BLOCKED_BASELINE`; a post result cannot erase a baseline block or mismatch. The public verifier does not accept private run roots, keys, attestations, provider tokens, ignored-failure lists, or provider parameters.

The frozen evaluator wrapper is only a dispatch guard. It validates replay versus live context fields, selected platform, clean detached evaluator worktree, and frozen binary hash, then appends one frozen-context argument to the child. It holds the verified descriptor, immediately rechecks descriptor and pathname identity before dispatch, and uses fd execution where the host standard library supports it; other hosts use direct pathname exec after that recheck. This narrows but cannot eliminate the final instruction-level pathname TOCTOU on platforms without fd execution. It produces no business manifest, business result, provider request, or cost record.

## Windows transfer and return

Windows evidence is transferred with two named, advertisable refs: one for the immutable tested candidate and one for the tools/evidence source needed to reproduce the capture contract. A bare SHA is not substituted for either transfer tip. The Windows machine creates a later return child ref containing only its evidence result; the receiving repository verifies that child against the frozen candidate and tools bindings before accepting it.

The Windows host label and bootstrap hashes bind what was reported and tested. They do not independently attest to the physical hardware or operating system of an untrusted remote host.

## Forbidden content

Do not commit credentials, bearer tokens, API keys, provider request or response bodies, private cases, prompts, reviewer mappings, user materials, customer content, dynamic broker ports, or private absolute paths here. Do not place business evaluation output in foundation evidence. Do not use this directory to claim provider execution: `providerMode=not-run` and `paidProviderCost=0` remain the only foundation provider statements.

## Retention

Public foundation records retain only the minimum byte-bound command evidence needed to verify the matrix. Private run materials, if a later business proof creates them, are outside this directory and follow that run's distinct retention deadline and closeout process. Foundation capture never imports private case data or provider secrets into public logs.

## Rollback

To roll back this boundary, revert the specific evidence/tool commit after preserving any already recorded evidence history required by policy. Do not rewrite imported ancestors, alter an existing manifest or log in place, broaden ignore rules, or remove unrelated evidence. A replacement capture is a new, separately verified record with a new commit.
