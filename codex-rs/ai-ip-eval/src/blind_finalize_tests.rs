use std::fs;
use std::path::Path;
use std::path::PathBuf;

use pretty_assertions::assert_eq;

use super::*;

fn replay_args(private_root: &Path) -> crate::BlindPackArgs {
    crate::BlindPackArgs {
        reviewer_root: PathBuf::from("reviewer"),
        mapping_dir: PathBuf::from("coordinator/mappings"),
        seed_dir: None,
        replay_seeds: ["one", "two", "three"].map(str::to_string).to_vec(),
        frozen_run_context: private_root.join("frozen-run-context.json"),
    }
}

fn native_args(private_root: &Path) -> crate::BlindPackArgs {
    let private_root = private_root.canonicalize().unwrap();
    crate::BlindPackArgs {
        reviewer_root: PathBuf::from("reviewer"),
        mapping_dir: PathBuf::from("coordinator/mappings"),
        seed_dir: Some(PathBuf::from("coordinator/blind-seeds")),
        replay_seeds: Vec::new(),
        frozen_run_context: private_root.join("frozen-run-context.json"),
    }
}

fn assert_no_outputs(private_root: &Path) {
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

fn replay_core() -> (
    tempfile::TempDir,
    ReplayPairTestRun,
    crate::blind_verify::PairEvidenceCore,
) {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let snapshot =
        crate::blind::read_context_snapshot(&replay.private_root.join("frozen-run-context.json"))
            .unwrap();
    let core = crate::blind_verify::verify_pair_evidence_core(&snapshot).unwrap();
    (fixture, replay, core)
}

fn native_core() -> (NativeMockPairTestRun, crate::blind_verify::PairEvidenceCore) {
    let max_total_tokens_per_run = 10;
    let native = run_native_mock_pair_with_marker(/*marker*/ None, max_total_tokens_per_run);
    native.result.as_ref().unwrap();
    let live_root = native.live_root.canonicalize().unwrap();
    let snapshot =
        crate::blind::read_context_snapshot(&live_root.join("frozen-run-context.json")).unwrap();
    let core = crate::blind_verify::verify_pair_evidence_core(&snapshot).unwrap();
    (native, core)
}

#[test]
fn treatment_marker_scan_is_recursive_case_insensitive_and_committed() {
    let mut package: codex_ai_ip_domain::ContentPackage =
        serde_json::from_str(&package_json()).unwrap();
    let mut markers = crate::blind_finalize::embedded_treatment_markers().unwrap();
    markers.extend([
        "candidate".to_string(),
        "generic".to_string(),
        codex_ai_ip_runtime::LEAD_SKILL_NAME.to_string(),
    ]);
    for marker in markers {
        package.claims[0].text = format!("deep {} value", marker.to_uppercase());
        let error =
            crate::blind_finalize::verify_decoded_package_strings(&package, &[marker]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "sealed package contains a forbidden treatment marker"
        );
    }
    package.claims[0].text = "Treatment-free nested claim".to_string();
    package.publishable_content.production_notes = vec!["Safe nested note".to_string()];
    crate::blind_finalize::verify_decoded_package_strings(&package, &["not-present".to_string()])
        .unwrap();
}

#[test]
fn real_replay_pair_reaches_bundle_stage_without_outputs() {
    let fixture = replay_fixture_with_material();
    let replay = run_frozen_replay_pair_from(fixture.path()).unwrap();
    let error = crate::blind::run_blind_pack(replay_args(&replay.private_root)).unwrap_err();
    assert_eq!(error.to_string(), "BlindBundleStageNotInstalled");
    assert_no_outputs(&replay.private_root);
}

#[test]
fn real_native_pair_reaches_bundle_stage_without_outputs() {
    let (native, core) = native_core();
    drop(core);
    let error = crate::blind::run_blind_pack(native_args(&native.live_root)).unwrap_err();
    assert_eq!(error.to_string(), "BlindBundleStageNotInstalled");
    assert_no_outputs(&native.live_root);
}

#[test]
fn finalizer_rejects_marker_and_skill_commitment_drift() {
    let (_fixture, replay, mut core) = replay_core();
    let candidate = core
        .arms
        .iter()
        .position(|arm| arm.condition == crate::EvaluationCondition::Candidate)
        .unwrap();
    for marker in [
        "hidden CANDIDATE marker".to_string(),
        "hidden GeNeRiC marker".to_string(),
        replay.private_root.to_string_lossy().into_owned(),
        replay
            .private_root
            .join("replay-generic-home")
            .to_string_lossy()
            .into_owned(),
        replay
            .private_root
            .join("replay-candidate-home/.codex/skills")
            .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
            .join("SKILL.md")
            .to_string_lossy()
            .into_owned(),
    ] {
        core.arms[candidate].content_package.claims[0].text = marker;
        let error = crate::blind_finalize::verify_treatment_and_skill(&core).unwrap_err();
        assert_eq!(
            error.to_string(),
            "sealed package contains a forbidden treatment marker"
        );
        assert_no_outputs(&replay.private_root);
    }

    core.arms[candidate].content_package.claims[0].text = "Treatment-free claim".to_string();
    let expected_candidate = core.arms[candidate].skill_use.clone();
    core.arms[candidate].skill_use.evidence_sha256 = Some("f".repeat(64));
    let error = crate::blind_finalize::verify_treatment_and_skill(&core).unwrap_err();
    assert_eq!(
        error.to_string(),
        "sealed Skill-use evidence does not bind the canonical complete Skill"
    );
    assert_no_outputs(&replay.private_root);

    core.arms[candidate].skill_use = expected_candidate;
    let generic = core
        .arms
        .iter()
        .position(|arm| arm.condition == crate::EvaluationCondition::Generic)
        .unwrap();
    let candidate_skill_use = core.arms[candidate].skill_use.clone();
    core.arms[generic].skill_use = candidate_skill_use;
    let error = crate::blind_finalize::verify_treatment_and_skill(&core).unwrap_err();
    assert_eq!(
        error.to_string(),
        "sealed Skill-use evidence does not bind the canonical complete Skill"
    );
    assert_no_outputs(&replay.private_root);
}

#[test]
fn finalizer_reverifies_replay_material_and_inventory() {
    let (fixture, replay, core) = replay_core();
    fs::write(fixture.path().join("notes/evidence.txt"), b"late drift\n").unwrap();
    let error = crate::blind_finalize::finalize_blind_pair(core).unwrap_err();
    assert_eq!(
        error.to_string(),
        "reverify Replay material replay-evidence"
    );
    assert_no_outputs(&replay.private_root);

    let (_fixture, replay, core) = replay_core();
    fs::write(replay.private_root.join("late-evidence.txt"), b"late\n").unwrap();
    let error = crate::blind_finalize::finalize_blind_pair(core).unwrap_err();
    assert_eq!(
        error.to_string(),
        "private tree differs from its inventory records"
    );
    assert_no_outputs(&replay.private_root);
}

#[test]
fn finalizer_reverifies_native_material() {
    let (native, core) = native_core();
    let material = core.materials.first().unwrap();
    let path = core
        .private_root
        .join("inputs/case")
        .join(&material.relative_path);
    fs::write(path, b"late native drift\n").unwrap();
    let error = crate::blind_finalize::finalize_blind_pair(core).unwrap_err();
    assert_eq!(error.to_string(), "re-read material:evidence-1 artifact");
    assert_no_outputs(&native.live_root);
}
