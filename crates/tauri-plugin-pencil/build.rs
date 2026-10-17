// The commands the web view may call. iOS implements them in Swift
// (`ios/Sources/PencilPlugin.swift`); there is no Rust side to them.
const COMMANDS: &[&str] = &[
    "open",
    "set_frame",
    "set_tool",
    "set_enabled",
    "undo",
    "redo",
    "clear",
    "export",
    "close",
    "register_listener",
    "remove_listener",
];

fn main() {
    tauri_plugin::Builder::new(COMMANDS).ios_path("ios").build();
}
