use anyhow::Context;
use anyhow::bail;
use std::path::PathBuf;

pub(crate) fn locate() -> anyhow::Result<PathBuf> {
    let target = "ai-ip-native-app-server-fixture";
    let path = if codex_utils_cargo_bin::runfiles_available() {
        let resource = format!("{target}{}", std::env::consts::EXE_SUFFIX);
        codex_utils_cargo_bin::find_resource!(resource)
            .with_context(|| format!("locate {target} in Bazel runfiles"))?
    } else {
        codex_utils_cargo_bin::cargo_bin(target)
            .with_context(|| format!("locate Cargo target {target}"))?
    };
    let path = path
        .canonicalize()
        .with_context(|| format!("canonicalize native App Server fixture at {}", path.display()))?;
    if !path.is_file() {
        bail!(
            "native App Server fixture target {target} resolved to a non-file path: {}",
            path.display()
        );
    }
    Ok(path)
}
