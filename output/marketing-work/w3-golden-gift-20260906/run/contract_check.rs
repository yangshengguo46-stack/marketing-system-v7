fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 1 {
        println!("{}", codex_ai_ip_runtime::content_package_schema().unwrap());
    } else {
        let bytes = std::fs::read(&args[1]).unwrap();
        let package: codex_ai_ip_domain::ContentPackage = serde_json::from_slice(&bytes).unwrap();
        package.validate().unwrap();
        println!("ContentPackage::validate PASS");
    }
}
