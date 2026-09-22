//! `omanote-cli`: Omanote from the terminal, for scripts and AI agents (see lib.rs).

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(omanote_cli::main_with(args, "omanote-cli"));
}
