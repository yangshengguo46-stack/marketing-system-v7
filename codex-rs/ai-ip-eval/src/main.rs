#[ctor::ctor]
fn pre_main() {
    codex_process_hardening::pre_main_hardening();
}

fn main() -> anyhow::Result<()> {
    codex_ai_ip_eval::run_main()
}
