fn main() {
    let target_vendor = std::env::var("CARGO_CFG_TARGET_VENDOR").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let flags: &[&str] = if target_vendor == "apple" {
        &["-Wl,-S", "-Wl,-x"]
    } else if matches!(target_env.as_str(), "gnu" | "musl") {
        &["-Wl,--strip-all"]
    } else {
        &[]
    };
    for flag in flags {
        println!("cargo::rustc-link-arg-bin=ai-ip-native-app-server-fixture={flag}");
    }
}
