//! sbie-box - local CLI for Sandboxie-Plus boxes and snapshots.
use sbiebox::{autorun, autostart, boot, box_status, create_box, delete_box, delete_snapshot, import_program, list_boxes, list_snapshots, restore, run_program, take_snapshot, terminate, Env};
use serde_json::Value;

const USAGE: &str = r#"sbie-box - local Sandboxie-Plus box manager

USAGE:
  sbie-box list                         list sandboxes
  sbie-box status <box>                 box state, pids, current snapshot
  sbie-box snapshots <box>              list snapshots
  sbie-box restore <box> <id>           materialize snapshot <id> onto the box
  sbie-box take <box> [--name NAME]     create a snapshot of the current state
  sbie-box snap-delete <box> <id>       delete a snapshot
  sbie-box boot <box> [--snapshot ID] --exe PATH [--args "..." ...]
                                        restore a snapshot, then start a program
  sbie-box run <box> --exe PATH [--args "..."]   start a program in a running box
  sbie-box stop <box>                   terminate everything in the box
  sbie-box create <box> [--type standard|app]    create a sandbox
  sbie-box delete <box> [--purge]       delete a sandbox definition
  sbie-box import <box> <folder> [--exe NAME.exe] [--name LABEL] [--no-menu]
                                        copy a portable ("green") program into the
                                        box and add it to the box's Run Menu

COMMON OPTIONS:
  --wait-ms N        how long to wait for the box to go idle (default 30000)
  --wait-window T    after boot, wait until a window title contains T
  --json             machine readable output (default)
  -h, --help         this help
"#;

struct Args {
    positional: Vec<String>,
    exe: Option<String>,
    prog_args: Vec<String>,
    name: Option<String>,
    snapshot: Option<String>,
    box_type: Option<String>,
    wait_ms: u64,
    wait_window: Option<String>,
    purge: bool,
    no_menu: bool,
}

fn parse(argv: &[String]) -> Result<Args, String> {
    let mut a = Args {
        positional: Vec::new(),
        exe: None,
        prog_args: Vec::new(),
        name: None,
        snapshot: None,
        box_type: None,
        wait_ms: 30_000,
        wait_window: None,
        purge: false,
        no_menu: false,
    };
    let mut i = 0;
    while i < argv.len() {
        let arg = &argv[i];
        match arg.as_str() {
            "--exe" => {
                i += 1;
                a.exe = Some(argv.get(i).ok_or("--exe needs a path")?.clone());
            }
            "--args" => {
                i += 1;
                let rest = argv.get(i).ok_or("--args needs a value")?;
                a.prog_args.extend(split_quoted(rest));
            }
            "--name" => {
                i += 1;
                a.name = Some(argv.get(i).ok_or("--name needs a value")?.clone());
            }
            "--snapshot" => {
                i += 1;
                a.snapshot = Some(argv.get(i).ok_or("--snapshot needs an id")?.clone());
            }
            "--no-menu" => {
                a.no_menu = true;
            }
            "--type" => {
                i += 1;
                a.box_type = Some(argv.get(i).ok_or("--type needs a value")?.clone());
            }
            "--wait-ms" => {
                i += 1;
                a.wait_ms = argv.get(i).ok_or("--wait-ms needs a number")?.parse().map_err(|_| "--wait-ms must be a number")?;
            }
            "--wait-window" => {
                i += 1;
                a.wait_window = Some(argv.get(i).ok_or("--wait-window needs a value")?.clone());
            }
            "--purge" => a.purge = true,
            other => a.positional.push(other.to_string()),
        }
        i += 1;
    }
    Ok(a)
}

fn split_quoted(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in raw.chars() {
        match ch {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn print(value: &Value) {
    println!("{}", serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string()));
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() || argv[0] == "-h" || argv[0] == "--help" {
        println!("{}", USAGE);
        return;
    }
    if let Err(err) = run(&argv) {
        eprintln!("error: {}", err);
        std::process::exit(1);
    }
}

fn run(argv: &[String]) -> Result<(), String> {
    let cmd = argv[0].as_str();
    if cmd == "list" {
        let env = Env::load()?;
        print(&Value::Array(list_boxes(&env)));
        return Ok(());
    }
    let args = parse(&argv[1..])?;
    let mut env = Env::load()?;
    let box_name = args.positional.first().cloned().ok_or("missing <box> argument")?;

    let result = match cmd {
        "status" => box_status(&env, &box_name)?,
        "snapshots" | "snap-list" => list_snapshots(&env, &box_name)?,
        "restore" => {
            let id = args.positional.get(1).cloned().or(args.snapshot.clone()).ok_or("missing <id> argument")?;
            restore(&env, &box_name, &id, args.wait_ms)?
        }
        "take" => take_snapshot(&env, &box_name, args.name.as_deref().unwrap_or(""), args.wait_ms)?,
        "snap-delete" => {
            let id = args.positional.get(1).cloned().ok_or("missing <id> argument")?;
            delete_snapshot(&env, &box_name, &id)?
        }
        "boot" => {
            let exe = args.exe.clone().ok_or("boot needs --exe <path>")?;
            boot(&env, &box_name, args.snapshot.as_deref(), &exe, &args.prog_args, args.wait_ms, args.wait_window.as_deref())?
        }
        "run" => {
            let exe = args.exe.clone().ok_or("run needs --exe <path>")?;
            run_program(&env, &box_name, &exe, &args.prog_args)?
        }
        "stop" | "terminate" => terminate(&env, &box_name, args.wait_ms)?,
        "create" => create_box(&mut env, &box_name, args.box_type.as_deref().unwrap_or("standard"))?,
        "delete" | "remove" => delete_box(&mut env, &box_name, args.purge, args.wait_ms)?,
        "import" => {
            let source = args.positional.get(1).cloned().ok_or("import needs a <folder> argument")?;
            import_program(&mut env, &box_name, &source, args.exe.as_deref(), args.name.as_deref(), !args.no_menu)?
        }
        "autorun" => autorun(&env, &box_name)?,
        "autostart" => {
            let arg = args.positional.get(1).map(|v| v.to_ascii_lowercase());
            let enable = match arg.as_deref() {
                None => None,
                Some("on") | Some("yes") | Some("1") => Some(true),
                Some("off") | Some("no") | Some("0") => Some(false),
                Some(other) => return Err(format!("autostart takes on|off, got '{}'", other)),
            };
            autostart(&mut env, &box_name, enable)?
        }
        other => return Err(format!("unknown command '{}' (try --help)", other)),
    };
    print(&result);
    Ok(())
}
