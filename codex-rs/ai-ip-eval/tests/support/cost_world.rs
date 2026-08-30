use std::collections::BTreeMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use codex_utils_cargo_bin::cargo_bin;
use pretty_assertions::assert_eq;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use tempfile::TempDir;

const COST_LEAVES: [&str; 4] = [
    "coordinator/cost/generic-receipt.json",
    "coordinator/cost/candidate-receipt.json",
    "coordinator/cost/generic-binding.json",
    "coordinator/cost/candidate-binding.json",
];

pub fn eval_binary() -> Result<PathBuf> {
    Ok(cargo_bin("codex-ai-ip-eval")?)
}

pub struct CostWorld {
    _temp: TempDir,
    binary: PathBuf,
    private_root: PathBuf,
    frozen_context: PathBuf,
}

impl CostWorld {
    pub fn replay() -> Result<Self> {
        let binary = eval_binary()?;
        let fixture_set =
            codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json")?;
        let fixture_root = fixture_set
            .parent()
            .context("replay fixture-set parent")?
            .canonicalize()?;
        let temp = private_temp()?;
        let private_root = owner_only_dir(&temp.path().join("replay-private"))?;
        let frozen_context = private_root.join("frozen-run-context.json");
        let freeze = Command::new(&binary)
            .args(["freeze-run-context", "replay", "--repo-root"])
            .arg(&fixture_root)
            .args(["--fork-sha", "synthetic-replay-fork", "--private-root"])
            .arg(&private_root)
            .arg("--codex-bin")
            .arg(&binary)
            .arg("--case")
            .arg(fixture_root.join("replay-case.json"))
            .arg("--transcript")
            .arg(fixture_root.join("replay-transcript.jsonl"))
            .arg("--fixture-set-manifest")
            .arg(&fixture_set)
            .arg("--output")
            .arg(&frozen_context)
            .output()?;
        assert_success("freeze-run-context replay", &freeze);
        let inputs = owner_only_dir(&private_root.join("inputs"))?;
        owner_only_dir(&inputs.join("supplier-statements"))?;
        let pair = Command::new(&binary)
            .args(["replay-pair", "--frozen-run-context"])
            .arg(&frozen_context)
            .output()?;
        assert_success("replay-pair", &pair);
        Ok(Self {
            _temp: temp,
            binary,
            private_root,
            frozen_context,
        })
    }

    pub fn native_mock() -> Result<Self> {
        let binary = eval_binary()?;
        let temp = private_temp()?;
        let repository = create_repository(temp.path())?;
        let fork_sha = git_output(&repository, &["rev-parse", "HEAD"])?;
        let private_root = owner_only_dir(&temp.path().join("native-private"))?;
        let inputs = owner_only_dir(&private_root.join("inputs"))?;
        let case = temp.path().join("case.json");
        let case_bytes = fs::read(codex_utils_cargo_bin::find_resource!(
            "tests/fixtures/replay-case.json"
        )?)?;
        fs::write(&case, &case_bytes)?;
        let materials = owner_only_dir(&temp.path().join("materials"))?;
        let cost_inputs = copy_cost_inputs(&inputs)?;
        let attestation = inputs.join("held-out-attestation.json");
        write_native_attestation(
            &attestation,
            &private_root,
            &fork_sha,
            &case_bytes,
            &cost_inputs,
        )?;
        let fixture_binary = temp.path().join(format!(
            "ai-ip-native-app-server-fixture{}",
            std::env::consts::EXE_SUFFIX
        ));
        fs::copy(cargo_bin("ai-ip-native-app-server-fixture")?, &fixture_binary)?;
        set_owner_file_mode(&fixture_binary, /*mode*/ 0o700)?;

        let upstream = tiny_http::Server::http("127.0.0.1:0")
            .map_err(|error| anyhow::anyhow!("bind loopback upstream: {error}"))?;
        let upstream_addr = upstream
            .server_addr()
            .to_ip()
            .context("loopback upstream address")?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = std::thread::spawn(move || -> Result<()> {
            let mut index = 0_u64;
            while !worker_stop.load(Ordering::SeqCst) {
                let Some(request) = upstream.recv_timeout(Duration::from_millis(100))? else {
                    continue;
                };
                let body = format!(
                    "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"cost-cli-response-{index}\",\"model\":\"mock-revision\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}}}}\n\n"
                );
                request.respond(tiny_http::Response::from_string(body).with_header(
                    tiny_http::Header::from_bytes("content-type", "text/event-stream")
                        .expect("static header is valid"),
                ))?;
                index += 1;
            }
            Ok(())
        });

        let frozen_context = private_root.join("frozen-run-context.json");
        let run = (|| -> Result<(Output, Option<Output>)> {
            let freeze = Command::new(&binary)
                .args(["freeze-run-context", "live", "--repo-root"])
                .arg(&repository)
                .arg("--evidence-repo-root")
                .arg(&repository)
                .arg("--fork-sha")
                .arg(&fork_sha)
                .arg("--private-root")
                .arg(&private_root)
                .arg("--codex-bin")
                .arg(&fixture_binary)
                .arg("--case")
                .arg(&case)
                .arg("--material-root")
                .arg(&materials)
                .arg("--attestation")
                .arg(&attestation)
                .arg("--provider-budget-evidence")
                .arg(&cost_inputs.budget)
                .arg("--rate-card")
                .arg(&cost_inputs.rate)
                .arg("--billing-policy")
                .arg(&cost_inputs.billing)
                .arg("--fx-policy")
                .arg(&cost_inputs.fx)
                .arg("--lead-skill")
                .arg(codex_utils_cargo_bin::find_resource!(
                    "tests/fixtures/replay-lead-skill.md"
                )?)
                .args(["--model-label", "local-mock"])
                .args(["--provider-label", "local-mock"])
                .args(["--provider-role", "approvedReference"])
                .arg("--provider-upstream-url")
                .arg(format!("http://{upstream_addr}/v1/responses"))
                .args(["--authorized-total-cost-fen", "0"])
                .args(["--authorized-per-run-cost-fen", "0"])
                .args(["--max-provider-request-attempts-per-run", "2"])
                .args(["--max-total-tokens-per-run", "10"])
                .args(["--max-elapsed-seconds-per-run", "180"])
                .args(["--max-output-tokens-per-request", "17"])
                .arg("--output")
                .arg(&frozen_context)
                .output()?;
            if !freeze.status.success() {
                return Ok((freeze, None));
            }
            let pair = Command::new(&binary)
                .args(["live-pair", "--frozen-run-context"])
                .arg(&frozen_context)
                .output()?;
            Ok((freeze, Some(pair)))
        })();
        stop.store(true, Ordering::SeqCst);
        let worker_result = worker
            .join()
            .map_err(|_| anyhow::anyhow!("loopback upstream worker panicked"))?;
        worker_result?;
        let (freeze, pair) = run?;
        assert_success("freeze-run-context live", &freeze);
        let pair = pair.context("live-pair did not run after a successful freeze")?;
        assert_success("live-pair", &pair);

        Ok(Self {
            _temp: temp,
            binary,
            private_root,
            frozen_context,
        })
    }

    pub fn private_root(&self) -> &Path {
        &self.private_root
    }

    pub fn supplier_statement(&self, condition: &str) -> PathBuf {
        self.private_root
            .join("inputs/supplier-statements")
            .join(format!("{condition}.json"))
    }

    pub fn add_pending_candidate_supplier(&self) -> Result<()> {
        let path = self.supplier_statement("candidate");
        fs::write(
            &path,
            fs::read(codex_utils_cargo_bin::find_resource!(
                "tests/fixtures/contracts/06b1/supplier-statement.canonical.json"
            )?)?,
        )?;
        set_owner_file_mode(&path, /*mode*/ 0o600)
    }

    pub fn assert_refusal(
        &self,
        condition: &str,
        supplier_statement: Option<&Path>,
        expected_stderr: &[u8],
    ) -> Result<()> {
        let inventory_path = self
            .private_root
            .join("coordinator/private-inventory.jsonl");
        let inventory_before = fs::read(&inventory_path)?;
        let tree_before = tree_snapshot(&self.private_root)?;
        let mut command = Command::new(&self.binary);
        command
            .args(["make-cost-receipt", "--condition", condition])
            .arg("--frozen-run-context")
            .arg(&self.frozen_context);
        if let Some(path) = supplier_statement {
            command.arg("--supplier-statement").arg(path);
        }
        let output = command.output()?;

        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, expected_stderr);
        self.assert_cost_leaves_absent();
        assert_eq!(fs::read(inventory_path)?, inventory_before);
        assert_eq!(tree_snapshot(&self.private_root)?, tree_before);
        Ok(())
    }

    fn assert_cost_leaves_absent(&self) {
        for relative in COST_LEAVES {
            assert_path_absent(&self.private_root, relative);
        }
    }
}

struct CostInputs {
    budget: PathBuf,
    rate: PathBuf,
    billing: PathBuf,
    fx: PathBuf,
}

fn private_temp() -> Result<TempDir> {
    let temp = TempDir::new()?;
    #[cfg(unix)]
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(/*mode*/ 0o700))?;
    Ok(temp)
}

fn owner_only_dir(path: &Path) -> Result<PathBuf> {
    fs::create_dir(path)?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(/*mode*/ 0o700))?;
    Ok(path.canonicalize()?)
}

fn set_owner_file_mode(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

fn create_repository(root: &Path) -> Result<PathBuf> {
    let repository = root.join("repo");
    fs::create_dir(&repository)?;
    fs::write(repository.join("source"), b"committed source\n")?;
    fs::create_dir_all(repository.join("codex-rs/responses-api-proxy/src"))?;
    fs::write(
        repository.join("codex-rs/responses-api-proxy/src/broker.rs"),
        b"// committed synthetic broker\n",
    )?;
    git_output(&repository, &["init", "--quiet"])?;
    git_output(&repository, &["add", "."])?;
    git_output(
        &repository,
        &[
            "-c",
            "user.name=Synthetic",
            "-c",
            "user.email=synthetic@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "cost CLI Native fixture",
        ],
    )?;
    Ok(repository.canonicalize()?)
}

fn git_output(repository: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "synthetic git command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn copy_cost_inputs(inputs: &Path) -> Result<CostInputs> {
    let copy = |leaf: &str, fixture: &str| -> Result<PathBuf> {
        let resource = format!("tests/fixtures/contracts/06b1/{fixture}");
        let destination = inputs.join(leaf);
        fs::copy(codex_utils_cargo_bin::find_resource!(resource)?, &destination)?;
        set_owner_file_mode(&destination, /*mode*/ 0o600)?;
        Ok(destination)
    };
    Ok(CostInputs {
        budget: copy(
            "provider-budget-evidence.json",
            "provider-budget-evidence.canonical.json",
        )?,
        rate: copy("rate-card.json", "provider-rate-card.canonical.json")?,
        billing: copy("billing-policy.json", "billing-policy.canonical.json")?,
        fx: copy("fx-policy.json", "fx-policy.canonical.json")?,
    })
}

fn write_native_attestation(
    path: &Path,
    private_root: &Path,
    fork_sha: &str,
    case_bytes: &[u8],
    cost_inputs: &CostInputs,
) -> Result<()> {
    let source = codex_utils_cargo_bin::find_resource!(
        "tests/fixtures/contracts/06a/canonical-native-attestation.json"
    )?;
    let mut attestation: Value = serde_json::from_slice(&fs::read(source)?)?;
    attestation["candidateSha"] = Value::String(fork_sha.to_string());
    attestation["caseSha256"] = Value::String(sha256(case_bytes));
    attestation["sourceMaterialsSha256"] = Value::String(sha256(b"[]"));
    attestation["privateRoot"] = serde_json::json!(private_root);
    attestation["providerRole"] = Value::String("approvedReference".to_string());
    attestation["approvedTotalFen"] = serde_json::json!(0);
    attestation["approvedPerRunFen"] = serde_json::json!(0);
    attestation["maxProviderRequestAttemptsPerRun"] = serde_json::json!(2);
    attestation["maxTotalTokensPerRun"] = serde_json::json!(10);
    attestation["maxElapsedSecondsPerRun"] = serde_json::json!(180);
    attestation["maxOutputTokensPerRequest"] = serde_json::json!(17);
    attestation["retentionDeadline"] = Value::String("2099-09-04T07:30:00Z".to_string());
    for (field, input) in [
        ("providerBudgetEvidenceSha256", &cost_inputs.budget),
        ("rateCardSha256", &cost_inputs.rate),
        ("billingPolicyCommitment", &cost_inputs.billing),
        ("fxPolicySha256", &cost_inputs.fx),
    ] {
        attestation[field] = Value::String(sha256(&fs::read(input)?));
    }
    fs::write(path, serde_json::to_vec_pretty(&attestation)?)?;
    set_owner_file_mode(path, /*mode*/ 0o600)
}

fn tree_snapshot(root: &Path) -> Result<BTreeMap<PathBuf, Option<Vec<u8>>>> {
    fn visit(
        root: &Path,
        current: &Path,
        snapshot: &mut BTreeMap<PathBuf, Option<Vec<u8>>>,
    ) -> Result<()> {
        let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root)?.to_path_buf();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                snapshot.insert(relative, None);
                visit(root, &path, snapshot)?;
            } else {
                assert!(kind.is_file(), "unexpected tree entry: {}", path.display());
                snapshot.insert(relative, Some(fs::read(path)?));
            }
        }
        Ok(())
    }

    let mut snapshot = BTreeMap::new();
    visit(root, root, &mut snapshot)?;
    Ok(snapshot)
}

fn assert_success(stage: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{stage} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_path_absent(private_root: &Path, relative: &str) {
    match fs::symlink_metadata(private_root.join(relative)) {
        Err(error) => assert_eq!(
            error.kind(),
            std::io::ErrorKind::NotFound,
            "found {relative}"
        ),
        Ok(metadata) => panic!("found {relative} with type {:?}", metadata.file_type()),
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
