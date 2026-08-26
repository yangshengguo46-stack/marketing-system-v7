use std::fs;
use std::path::Path;

use codex_app_server_protocol::SkillsListResponse;
use pretty_assertions::assert_eq;
use serde_json::json;

use crate::CatalogRoots;
use crate::compare_catalogs;
use crate::normalize_catalog;
use crate::validate_stable_catalog;

fn skill(name: &str, path: &Path) -> serde_json::Value {
    json!({
        "name": name,
        "description": format!("{name} description"),
        "shortDescription": format!("{name} short"),
        "interface": null,
        "dependencies": {"tools": []},
        "path": path,
        "scope": "user",
        "enabled": true
    })
}

fn response(cwd: &Path, skills: Vec<serde_json::Value>) -> SkillsListResponse {
    serde_json::from_value(json!({
        "data": [{"cwd": cwd, "skills": skills, "errors": []}]
    }))
    .unwrap()
}

#[test]
fn candidate_catalog_diff_is_exactly_the_canonical_lead_skill() {
    let temp = tempfile::tempdir().unwrap();
    let host = temp.path().join("host");
    let generic_home = host.join("generic/.codex");
    let candidate_home = host.join("candidate/.codex");
    let case = temp.path().join("case");
    for path in [&generic_home, &candidate_home, &case] {
        fs::create_dir_all(path).unwrap();
    }
    let generic_common = generic_home.join("skills/common/SKILL.md");
    let candidate_common = candidate_home.join("skills/common/SKILL.md");
    let candidate_target = candidate_home.join("skills/deliver-ai-ip-content-package/SKILL.md");
    for path in [&generic_common, &candidate_common, &candidate_target] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
    }
    fs::write(&generic_common, "common").unwrap();
    fs::write(&candidate_common, "common").unwrap();
    let source_skill = codex_utils_cargo_bin::find_resource!(
        "../../ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md"
    )
    .unwrap();
    let source_skill_bytes = fs::read(source_skill).unwrap();
    fs::write(&candidate_target, &source_skill_bytes).unwrap();

    let generic_response = response(&case, vec![skill("common", &generic_common)]);
    let candidate_response = response(
        &case,
        vec![
            skill("deliver-ai-ip-content-package", &candidate_target),
            skill("common", &candidate_common),
        ],
    );
    let generic = normalize_catalog(
        &generic_response,
        &CatalogRoots {
            codex_home: generic_home,
            host_home: host.clone(),
            case_dir: case.clone(),
        },
    )
    .unwrap();
    let candidate = normalize_catalog(
        &candidate_response,
        &CatalogRoots {
            codex_home: candidate_home,
            host_home: host,
            case_dir: case,
        },
    )
    .unwrap();

    let parity =
        compare_catalogs(&generic, &candidate, &candidate_target, &source_skill_bytes).unwrap();
    assert_eq!(generic.target_count, 0);
    assert_eq!(candidate.target_count, 1);
    assert_eq!(parity.normalized_base_catalog_sha256, generic.sha256);
    assert_eq!(parity.candidate_skill_sha256.len(), 64);
    validate_stable_catalog(&candidate, &candidate).unwrap();
}

#[test]
fn catalog_normalization_rejects_errors_wrong_cwd_and_treatment_drift() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let case = temp.path().join("case");
    let wrong = temp.path().join("wrong");
    for path in [&home, &case, &wrong] {
        fs::create_dir_all(path).unwrap();
    }
    let roots = CatalogRoots {
        codex_home: home,
        host_home: temp.path().to_path_buf(),
        case_dir: case.clone(),
    };
    let wrong_response = response(&wrong, vec![]);
    assert!(
        normalize_catalog(&wrong_response, &roots)
            .unwrap_err()
            .to_string()
            .contains("cwd")
    );

    let error_response: SkillsListResponse = serde_json::from_value(json!({
        "data": [{
            "cwd": case,
            "skills": [],
            "errors": [{"path": "bad", "message": "synthetic"}]
        }]
    }))
    .unwrap();
    assert!(
        normalize_catalog(&error_response, &roots)
            .unwrap_err()
            .to_string()
            .contains("errors")
    );
}
