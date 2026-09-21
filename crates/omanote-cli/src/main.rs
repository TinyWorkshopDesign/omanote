//! `omanote-cli`: Omanote from the terminal, for scripts and AI agents.
//!
//! It works on the same local database as the app (the app picks changes up
//! within ~2 s and syncs them to Joplin Server). `omanote-cli mcp` exposes the
//! same operations as a Model Context Protocol server over stdio.

mod mcp;
mod ops;

use std::io::Read;

use ops::Ctx;

const HELP: &str = "\
omanote-cli — Omanote notes from the terminal

USAGE
  omanote-cli <command> [options]

COMMANDS
  folders                          List folders of the working notebook
  list [--folder F] [--limit N]    List notes, newest first
  show <note>                      Print a note
  search <text>                    Find notes containing text
  new [--folder F] [text|-]        Create a note (text from stdin with -)
  append <note> [text|-]           Append text to a note
  edit <note> [text|-]             Replace the text of a note
  move <note> <folder>             Move a note to another folder
  trash <note>                     Move a note to the Joplin trash
  sync                             Sync with Joplin Server now
  mcp                              Run as an MCP server on stdio

  <note> is an id or a unique id prefix; <folder> an id or a folder name.
  Add --json to get machine-readable output.

ENVIRONMENT
  OMANOTE_DATA_DIR   Data directory (default: the app's)
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(args) {
        eprintln!("omanote-cli: {e}");
        std::process::exit(1);
    }
}

fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(i) = args.iter().position(|a| a == name) {
        args.remove(i);
        true
    } else {
        false
    }
}

fn take_opt(args: &mut Vec<String>, name: &str) -> Option<String> {
    let i = args.iter().position(|a| a == name)?;
    args.remove(i);
    (i < args.len()).then(|| args.remove(i))
}

/// Text from the remaining args, or stdin when it is "-" or missing.
fn text_arg(rest: &[String]) -> Result<String, String> {
    if rest.is_empty() || rest == ["-"] {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map_err(|e| e.to_string())?;
        Ok(s.trim_end_matches('\n').to_string())
    } else {
        Ok(rest.join(" "))
    }
}

fn run(mut args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || take_flag(&mut args, "--help") || take_flag(&mut args, "-h") {
        print!("{HELP}");
        return Ok(());
    }
    let json = take_flag(&mut args, "--json");
    let cmd = args.remove(0);

    if cmd == "mcp" {
        return mcp::serve();
    }

    let ctx = Ctx::open()?;
    let print = |v: serde_json::Value, human: String| {
        if json {
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        } else if !human.is_empty() {
            println!("{human}");
        }
    };

    match cmd.as_str() {
        "folders" => {
            let folders = ctx.folders()?;
            let human = folders
                .iter()
                .map(|f| format!("{}  {}{} ({})", &f.id[..8], "  ".repeat(f.depth), f.title, f.note_count))
                .collect::<Vec<_>>()
                .join("\n");
            print(serde_json::to_value(&folders).unwrap(), human);
        }
        "list" => {
            let folder = take_opt(&mut args, "--folder");
            let limit = take_opt(&mut args, "--limit").and_then(|l| l.parse().ok()).unwrap_or(50);
            let notes = ctx.list(folder.as_deref(), limit)?;
            let human = notes
                .iter()
                .map(|n| format!("{}  {}", &n.id[..8], if n.title.is_empty() { "(untitled)" } else { &n.title }))
                .collect::<Vec<_>>()
                .join("\n");
            print(serde_json::to_value(&notes).unwrap(), human);
        }
        "search" => {
            let q = args.join(" ");
            let notes = ctx.search(&q)?;
            let human = notes.iter().map(|n| format!("{}  {}", &n.id[..8], n.title)).collect::<Vec<_>>().join("\n");
            print(serde_json::to_value(&notes).unwrap(), human);
        }
        "show" => {
            let note = ctx.read(args.first().ok_or("missing note id")?)?;
            print(serde_json::to_value(&note).unwrap(), note.text.clone());
        }
        "new" => {
            let folder = take_opt(&mut args, "--folder");
            let note = ctx.create(folder.as_deref(), &text_arg(&args)?)?;
            print(serde_json::to_value(&note).unwrap(), note.id.clone());
        }
        "append" => {
            let id = args.first().ok_or("missing note id")?.clone();
            let note = ctx.append(&id, &text_arg(&args[1..])?)?;
            print(serde_json::to_value(&note).unwrap(), String::new());
        }
        "edit" => {
            let id = args.first().ok_or("missing note id")?.clone();
            let note = ctx.update(&id, &text_arg(&args[1..])?)?;
            print(serde_json::to_value(&note).unwrap(), String::new());
        }
        "move" => {
            let (id, folder) = (args.first().ok_or("missing note id")?, args.get(1).ok_or("missing folder")?);
            ctx.move_note(id, folder)?;
        }
        "trash" => ctx.trash(args.first().ok_or("missing note id")?)?,
        "sync" => {
            let report = ctx.sync()?;
            print(
                serde_json::to_value(&report).unwrap(),
                format!("↑ {}  ↓ {}  conflicts {}", report.uploaded, report.downloaded, report.conflicts),
            );
        }
        other => return Err(format!("unknown command '{other}' (see --help)")),
    }
    Ok(())
}
