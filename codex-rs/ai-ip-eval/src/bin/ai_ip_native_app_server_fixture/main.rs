mod server;
mod wire;

const EXPECTED_ARGS: [&str; 4] = ["app-server", "--listen", "stdio://", "--strict-config"];

fn main() -> anyhow::Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    anyhow::ensure!(
        args == EXPECTED_ARGS,
        "expected argv {:?}, received {args:?}",
        EXPECTED_ARGS
    );
    server::run()
}
