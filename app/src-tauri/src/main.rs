// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `omanote mcp`, `omanote list`, …: the terminal / AI-agent commands, without
    // starting the window (on macOS the app binary is the only one installed).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| omanote_cli::COMMANDS.contains(&a.as_str())) {
        std::process::exit(omanote_cli::main_with(args, "omanote"));
    }
    omanote_lib::run()
}
