//! Omanote from the terminal, for scripts and AI agents: the commands of
//! `omanote-cli`, also reachable as `omanote <command>` from the app binary (so
//! macOS needs nothing installed beyond Omanote.app).
//!
//! It works on the same local database as the app (the app picks changes up
//! within ~2 s and syncs them to Joplin Server). The `mcp` command exposes the
//! same operations as a Model Context Protocol server over stdio.

pub mod mcp;
pub mod ops;
pub mod sketch;

use std::io::Read;

use ops::Ctx;

const HELP: &str = "\
PROG — Omanote notes from the terminal

USAGE
  PROG <command> [options]

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
  resource <id>                    Print the local file of an attachment (downloads it if
                                   needed); --json adds the strokes of a sketch
  sketch [--replace <id>] [--background #rrggbb]
                                   Read {\"strokes\": [...]} as one JSON line on stdin and save
                                   it as an editable sketch PNG; prints the note's <img> tag
  sync                             Sync with Joplin Server now
  mcp                              Run as an MCP server on stdio

  <note> is an id or a unique id prefix; <folder> an id or a folder name.
  Add --json to get machine-readable output.

ENVIRONMENT
  OMANOTE_DATA_DIR   Data directory (default: the app's)
";

/// Commands understood by [`main_with`]; the app binary hands these over.
pub const COMMANDS: &[&str] =
    &["folders", "list", "show", "search", "new", "append", "edit", "move", "trash", "resource", "sketch", "sync", "mcp", "help"];

/// Runs one command and returns the process exit code. `prog` names the binary in messages.
pub fn main_with(args: Vec<String>, prog: &str) -> i32 {
    match run(args, prog) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{prog}: {e}");
            1
        }
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

fn run(mut args: Vec<String>, prog: &str) -> Result<(), String> {
    if args.is_empty() || args[0] == "help" || take_flag(&mut args, "--help") || take_flag(&mut args, "-h") {
        print!("{}", HELP.replace("PROG", prog));
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
        "resource" => {
            let id = args.first().ok_or("missing attachment id")?;
            let path = ctx.resource(id)?.ok_or("attachment not found")?;
            let sketch = if json { std::fs::read(&path).ok().and_then(|b| sketch::read_sketch_data(&b)) } else { None };
            print(serde_json::json!({ "id": id, "path": path, "sketch": sketch }), path.display().to_string());
        }
        "sketch" => {
            let replace = take_opt(&mut args, "--replace");
            let background = take_opt(&mut args, "--background").unwrap_or_else(|| "#ffffff".into());
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).map_err(|e| e.to_string())?;
            #[derive(serde::Deserialize)]
            struct Input {
                strokes: Vec<sketch::StrokeData>,
            }
            let input: Input = serde_json::from_str(&line).map_err(|e| format!("bad strokes: {e}"))?;
            let r = sketch::render(&input.strokes, &background)?.ok_or("nothing drawn")?;
            let (id, name) = match replace {
                Some(id) => {
                    ctx.replace_sketch(&id, &r.png)?;
                    (id, String::new())
                }
                None => {
                    let name = format!("sketch-{}.png", sketch_stamp());
                    (ctx.add_sketch(&r.png, &name)?, name)
                }
            };
            let tag = format!("<img src=\":/{id}\" alt=\"{name}\" width=\"{}\">", r.width);
            print(serde_json::json!({ "id": id, "name": name, "width": r.width, "tag": tag }), tag);
        }
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

/// `2026-11-03-07-30-00`, like the app's sketch names (UTC, as `toISOString`).
fn sketch_stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}-{:02}-{:02}-{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}
