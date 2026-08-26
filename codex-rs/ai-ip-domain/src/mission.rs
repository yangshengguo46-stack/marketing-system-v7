use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

pub const MAX_CASE_ID_BYTES: usize = 128;
pub const MAX_MATERIAL_ID_BYTES: usize = 128;
pub const MAX_MATERIALS: usize = 128;
pub const MAX_CONSTRAINTS: usize = 64;
pub const MAX_OBJECTIVE_BYTES: usize = 8 * 1024;
pub const MAX_CONSTRAINT_BYTES: usize = 2 * 1024;
pub const MAX_MATERIAL_PATH_BYTES: usize = 1024;

const RESERVED_DEVELOPMENT_NAMESPACE: &str = "__ai_ip_dev__";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationErrors {
    pub(crate) errors: Vec<String>,
}

impl ValidationErrors {
    pub fn errors(&self) -> &[String] {
        &self.errors
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.errors.join("; "))
    }
}

impl Error for ValidationErrors {}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub enum SubjectKind {
    Person,
    Brand,
    Product,
    Organization,
    Hybrid,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub enum MaterialKind {
    UserInput,
    Evidence,
    ActualResultReceipt,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MissionMaterial {
    pub material_id: String,
    pub relative_path: String,
    pub sha256: String,
    pub material_kind: MaterialKind,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HeldOutMissionCase {
    pub case_id: String,
    pub objective: String,
    pub subject_kind: SubjectKind,
    pub constraints: Vec<String>,
    pub materials: Vec<MissionMaterial>,
}

impl HeldOutMissionCase {
    pub fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = Vec::new();

        if self.case_id.starts_with(RESERVED_DEVELOPMENT_NAMESPACE) {
            errors.push("caseId: reserved development namespace".into());
        } else if !is_internal_id(&self.case_id, MAX_CASE_ID_BYTES) {
            errors.push("caseId: invalid internal id".into());
        }

        if self.objective.is_empty() {
            errors.push("objective: must not be empty".into());
        } else if self.objective.len() > MAX_OBJECTIVE_BYTES {
            errors.push("objective: exceeds 8192 bytes".into());
        }

        if self.constraints.len() > MAX_CONSTRAINTS {
            errors.push("constraints: exceeds 64 items".into());
        }
        for (index, constraint) in self.constraints.iter().enumerate() {
            if constraint.is_empty() {
                errors.push(format!("constraints[{index}]: must not be empty"));
            } else if constraint.len() > MAX_CONSTRAINT_BYTES {
                errors.push(format!("constraints[{index}]: exceeds 2048 bytes"));
            }
        }

        if self.materials.len() > MAX_MATERIALS {
            errors.push("materials: exceeds 128 items".into());
        }
        let mut material_ids = HashSet::new();
        for (index, material) in self.materials.iter().enumerate() {
            let material_id_field = format!("materials[{index}].materialId");
            if !is_internal_id(&material.material_id, MAX_MATERIAL_ID_BYTES)
                || material
                    .material_id
                    .starts_with(RESERVED_DEVELOPMENT_NAMESPACE)
            {
                errors.push(format!("{material_id_field}: invalid internal id"));
            }
            if !material_ids.insert(&material.material_id) {
                errors.push(format!("{material_id_field}: duplicate"));
            }

            let path_field = format!("materials[{index}].relativePath");
            if material.relative_path.is_empty() {
                errors.push(format!("{path_field}: must not be empty"));
            } else if material.relative_path.len() > MAX_MATERIAL_PATH_BYTES {
                errors.push(format!("{path_field}: exceeds 1024 bytes"));
            } else if !is_normalized_relative_path(&material.relative_path) {
                errors.push(format!("{path_field}: must be a normalized relative path"));
            }

            if !is_lowercase_sha256(&material.sha256) {
                errors.push(format!(
                    "materials[{index}].sha256: must be 64 lowercase hexadecimal characters"
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ValidationErrors { errors })
        }
    }
}

fn is_internal_id(value: &str, maximum_bytes: usize) -> bool {
    if value.is_empty() || value.len() > maximum_bytes {
        return false;
    }

    let mut characters = value.bytes();
    match characters.next() {
        Some(character) if character.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    characters.all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, b'.' | b'_' | b'-')
    })
}

fn is_normalized_relative_path(value: &str) -> bool {
    if value.starts_with('/') || value.contains('\\') || is_windows_drive_path(value) {
        return false;
    }

    value
        .split('/')
        .all(|segment| !segment.is_empty() && !matches!(segment, "." | ".."))
}

fn is_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
