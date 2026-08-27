use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::bail;
use codex_ai_ip_runtime::LEAD_SKILL_NAME;
use codex_app_server_protocol::SkillMetadata;
use codex_app_server_protocol::SkillScope;
use codex_app_server_protocol::SkillsListResponse;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

pub struct CatalogRoots {
    pub codex_home: PathBuf,
    pub host_home: PathBuf,
    pub case_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct NormalizedSkill {
    name: String,
    description: String,
    short_description: Option<String>,
    interface: Option<serde_json::Value>,
    dependencies: Option<serde_json::Value>,
    path: String,
    scope: SkillScope,
    enabled: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CatalogSnapshot {
    pub canonical_json: Vec<u8>,
    pub sha256: String,
    pub target_count: usize,
    without_target_json: Vec<u8>,
    target_path: Option<String>,
    target_is_enabled_user: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CatalogParity {
    pub normalized_base_catalog_sha256: String,
    pub candidate_skill_sha256: String,
}

impl CatalogSnapshot {
    pub fn normalized_base_catalog_sha256(&self) -> String {
        sha256(&self.without_target_json)
    }
}

pub fn normalize_catalog(
    response: &SkillsListResponse,
    roots: &CatalogRoots,
) -> anyhow::Result<CatalogSnapshot> {
    if response.data.len() != 1 {
        bail!("skills/list must return exactly one cwd entry");
    }
    let entry = &response.data[0];
    if !entry.errors.is_empty() {
        bail!("skills/list returned errors");
    }
    let canonical_case = roots
        .case_dir
        .canonicalize()
        .context("canonicalize case cwd")?;
    let response_cwd = entry
        .cwd
        .canonicalize()
        .context("canonicalize skills/list cwd")?;
    if response_cwd != canonical_case {
        bail!("skills/list returned a non-canonical or unexpected cwd");
    }

    let canonical_codex_home = roots
        .codex_home
        .canonicalize()
        .context("canonicalize CODEX_HOME")?;
    let canonical_host_home = roots
        .host_home
        .canonicalize()
        .context("canonicalize host HOME")?;
    let mut normalized = entry
        .skills
        .iter()
        .map(|skill| {
            normalize_skill(
                skill,
                &canonical_codex_home,
                &canonical_host_home,
                &canonical_case,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    normalized.sort_by_key(skill_key);
    let targets = normalized
        .iter()
        .filter(|skill| skill.name == LEAD_SKILL_NAME)
        .collect::<Vec<_>>();
    let target_path = targets.first().map(|skill| skill.path.clone());
    let target_is_enabled_user = targets
        .first()
        .is_some_and(|skill| skill.enabled && skill.scope == SkillScope::User);
    let without_target = normalized
        .iter()
        .filter(|skill| skill.name != LEAD_SKILL_NAME)
        .cloned()
        .collect::<Vec<_>>();
    let canonical_json = serde_json::to_vec(&normalized)?;
    let without_target_json = serde_json::to_vec(&without_target)?;
    Ok(CatalogSnapshot {
        sha256: sha256(&canonical_json),
        canonical_json,
        target_count: targets.len(),
        without_target_json,
        target_path,
        target_is_enabled_user,
    })
}

pub fn compare_catalogs(
    generic: &CatalogSnapshot,
    candidate: &CatalogSnapshot,
    candidate_skill_path: &Path,
    source_skill_bytes: &[u8],
) -> anyhow::Result<CatalogParity> {
    if generic.target_count != 0 {
        bail!("generic catalog contains the target Skill");
    }
    if candidate.target_count != 1 {
        bail!("candidate catalog must contain exactly one target Skill");
    }
    if !candidate.target_is_enabled_user {
        bail!("candidate target Skill must be enabled with user scope");
    }
    let expected_target_path = format!("$CODEX_HOME/skills/{LEAD_SKILL_NAME}/SKILL.md");
    if candidate.target_path.as_deref() != Some(expected_target_path.as_str()) {
        bail!("candidate target Skill has the wrong canonical path");
    }
    if generic.canonical_json != candidate.without_target_json {
        bail!("catalogs differ after removing the candidate target Skill");
    }
    let candidate_bytes = std::fs::read(candidate_skill_path).context("read candidate SKILL.md")?;
    if candidate_bytes != source_skill_bytes {
        bail!("candidate SKILL.md differs from the source asset");
    }
    Ok(CatalogParity {
        normalized_base_catalog_sha256: sha256(&generic.canonical_json),
        candidate_skill_sha256: sha256(&candidate_bytes),
    })
}

pub fn validate_stable_catalog(
    before: &CatalogSnapshot,
    after: &CatalogSnapshot,
) -> anyhow::Result<()> {
    if before.canonical_json != after.canonical_json {
        bail!("skills catalog changed during the arm");
    }
    Ok(())
}

fn normalize_skill(
    skill: &SkillMetadata,
    codex_home: &Path,
    host_home: &Path,
    case_dir: &Path,
) -> anyhow::Result<NormalizedSkill> {
    let path = skill
        .path
        .as_path()
        .canonicalize()
        .context("canonicalize Skill path")?;
    let path = normalize_path(&path, codex_home, host_home, case_dir)?;
    let mut interface = skill
        .interface
        .as_ref()
        .map(serde_json::to_value)
        .transpose()?;
    if let Some(interface) = interface.as_mut() {
        normalize_json_paths(interface, codex_home, host_home, case_dir)?;
    }
    Ok(NormalizedSkill {
        name: skill.name.clone(),
        description: skill.description.clone(),
        short_description: skill.short_description.clone(),
        interface,
        dependencies: skill
            .dependencies
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?,
        path,
        scope: skill.scope,
        enabled: skill.enabled,
    })
}

fn normalize_json_paths(
    value: &mut serde_json::Value,
    codex_home: &Path,
    host_home: &Path,
    case_dir: &Path,
) -> anyhow::Result<()> {
    let Some(object) = value.as_object_mut() else {
        return Ok(());
    };
    for key in ["iconSmall", "iconLarge"] {
        if let Some(path_value) = object.get_mut(key)
            && let Some(raw) = path_value.as_str()
        {
            let canonical = Path::new(raw)
                .canonicalize()
                .context("canonicalize Skill interface path")?;
            *path_value = serde_json::Value::String(normalize_path(
                &canonical, codex_home, host_home, case_dir,
            )?);
        }
    }
    Ok(())
}

fn normalize_path(
    path: &Path,
    codex_home: &Path,
    host_home: &Path,
    case_dir: &Path,
) -> anyhow::Result<String> {
    for (root, token) in [
        (codex_home, "$CODEX_HOME"),
        (case_dir, "$CASE"),
        (host_home, "$HOST_HOME"),
    ] {
        if let Ok(relative) = path.strip_prefix(root) {
            let suffix = relative.to_string_lossy().replace('\\', "/");
            return Ok(if suffix.is_empty() {
                token.to_string()
            } else {
                format!("{token}/{suffix}")
            });
        }
    }
    bail!(
        "catalog path is outside the frozen roots: {}",
        path.display()
    )
}

fn skill_key(skill: &NormalizedSkill) -> (String, String, String) {
    let scope = serde_json::to_string(&skill.scope).unwrap_or_default();
    (scope, skill.name.clone(), skill.path.clone())
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
