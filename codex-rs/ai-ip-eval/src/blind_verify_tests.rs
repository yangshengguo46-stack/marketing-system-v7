use std::fs;
use std::path::Path;
use std::path::PathBuf;

use pretty_assertions::assert_eq;

use super::*;
use crate::blind::FrozenInputToken;
use crate::blind_verify::ExecutionContext;
use crate::blind_verify::PairVerification;
use crate::blind_verify::parse_pair_evidence;

fn replay_blind_args(private_root: &Path) -> crate::BlindPackArgs {
    crate::BlindPackArgs {
        reviewer_root: PathBuf::from("reviewer"),
        mapping_dir: PathBuf::from("coordinator/mappings"),
        seed_dir: None,
        replay_seeds: ["one", "two", "three"].map(str::to_string).to_vec(),
        frozen_run_context: private_root.join("frozen-run-context.json"),
    }
}

fn native_blind_args(private_root: &Path) -> crate::BlindPackArgs {
    let private_root = private_root.canonicalize().unwrap();
    crate::BlindPackArgs {
        reviewer_root: PathBuf::from("reviewer"),
        mapping_dir: PathBuf::from("coordinator/mappings"),
        seed_dir: Some(PathBuf::from("coordinator/blind-seeds")),
        replay_seeds: Vec::new(),
        frozen_run_context: private_root.join("frozen-run-context.json"),
    }
}

fn assert_no_blind_outputs(private_root: &Path) {
    for relative in [
        "reviewer",
        "coordinator/mappings",
        "coordinator/blind-seeds",
        "reviews",
        "coordinator/blind-pack-receipt.json",
    ] {
        let error = fs::symlink_metadata(private_root.join(relative)).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound, "{relative}");
    }
}

fn assert_current_stage(args: crate::BlindPackArgs, private_root: &Path) {
    let error = crate::blind::run_blind_pack(args).unwrap_err();
    assert_eq!(error.to_string(), "PairEvidenceCoreStageNotInstalled");
    assert_no_blind_outputs(private_root);
}

#[test]
fn blind_pair_parser_retains_exact_real_replay_and_native_envelopes() {
    let replay = run_frozen_replay_pair().unwrap();
    let replay_args = replay_blind_args(&replay.private_root);
    let snapshot = crate::blind::read_context_snapshot(&replay_args.frozen_run_context).unwrap();
    let parsed = parse_pair_evidence(&snapshot).unwrap();
    match &parsed.inputs {
        FrozenInputToken::Replay(inputs) => inputs.reverify_all().unwrap(),
        FrozenInputToken::Native { .. } => panic!("Replay pair returned Native inputs"),
    }
    let replay_coordinator = replay.private_root.join("replay-coordinator");
    let execution_bytes = fs::read(replay_coordinator.join("execution-context.json")).unwrap();
    let execution: ExecutionContext = serde_json::from_slice(&execution_bytes).unwrap();
    assert_eq!(
        (
            &parsed.execution_context.typed,
            &parsed.execution_context.raw_bytes
        ),
        (&execution, &execution_bytes)
    );
    assert_eq!(
        parsed.arms.each_ref().map(|arm| &arm.typed),
        [&replay.manifests[0], &replay.manifests[1]]
    );
    let pair_bytes = fs::read(replay_coordinator.join("replay-pair-verification.json")).unwrap();
    let pair: PairVerification = serde_json::from_slice(&pair_bytes).unwrap();
    assert_eq!(
        (
            &parsed.pair_verification.typed,
            &parsed.pair_verification.raw_bytes
        ),
        (&pair, &pair_bytes)
    );
    parsed.inventory.reverify_unchanged().unwrap();
    assert_current_stage(replay_args, &replay.private_root);

    let native = run_native_mock_pair_with_marker(None, 10);
    native.result.as_ref().unwrap();
    let native_args = native_blind_args(&native.live_root);
    let snapshot = crate::blind::read_context_snapshot(&native_args.frozen_run_context).unwrap();
    let parsed = parse_pair_evidence(&snapshot).unwrap();
    match &parsed.inputs {
        FrozenInputToken::Native { frozen, content } => {
            frozen.reverify_all().unwrap();
            content.reverify_unchanged().unwrap();
        }
        FrozenInputToken::Replay(_) => panic!("Native pair returned Replay inputs"),
    }
    let coordinator = native.live_root.join("coordinator");
    for (document, path) in [
        (
            &parsed.execution_context.raw_bytes,
            coordinator.join("execution-context.json"),
        ),
        (
            &parsed.pair_verification.raw_bytes,
            coordinator.join("pair-verification.json"),
        ),
    ] {
        assert_eq!(document, &fs::read(path).unwrap());
    }
    assert_current_stage(native_args, &native.live_root);
}
