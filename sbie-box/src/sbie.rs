//! Thin wrappers around the shipped Sandboxie tools (Start.exe / SbieIni.exe).
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
fn hide_window(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW)
}

#[cfg(not(windows))]
fn hide_window(cmd: &mut Command) -> &mut Command {
    cmd
}

pub struct CmdOutput {
    pub ok: bool,
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

fn run(exe: &Path, args: &[String]) -> Result<CmdOutput, String> {
    if !exe.is_file() {
        return Err(format!("tool not found: {}", exe.display()));
    }
    let mut cmd = Command::new(exe);
    cmd.args(args);
    hide_window(&mut cmd);
    let out = cmd.output().map_err(|e| format!("cannot run {}: {}", exe.display(), e))?;
    Ok(CmdOutput {
        ok: out.status.success(),
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}

/// PIDs of processes running inside a sandbox.
pub fn list_pids(start_exe: &Path, box_name: &str) -> Result<Vec<u32>, String> {
    let out = run(start_exe, &[format!("/box:{}", box_name), "/listpids".to_string()])?;
    let mut pids = Vec::new();
    for token in out.stdout.split(|c: char| c.is_whitespace() || c == ',') {
        // Start.exe prints a bare 0 when the box has no processes; a pid 0
        // does not exist and must not be reported (or terminate() would poll
        // it forever).
        if let Ok(pid) = token.trim().parse::<u32>() {
            if pid != 0 {
                pids.push(pid);
            }
        }
    }
    Ok(pids)
}

/// Terminate every process in the sandbox and wait for the box to go idle.
pub fn terminate(start_exe: &Path, box_name: &str, wait_ms: u64) -> Result<(), String> {
    let _ = run(start_exe, &[format!("/box:{}", box_name), "/terminate".to_string()])?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
    loop {
        let pids = list_pids(start_exe, box_name).unwrap_or_default();
        if pids.is_empty() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(format!("sandbox {} still has processes: {:?}", box_name, pids));
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

/// Launch a program inside the sandbox.
pub fn run_in_box(start_exe: &Path, box_name: &str, program: &Path, args: &[String]) -> Result<CmdOutput, String> {
    let mut full = vec![format!("/box:{}", box_name), "/silent".to_string(), program.display().to_string()];
    full.extend(args.iter().cloned());
    run(start_exe, &full)
}

/// Resolve a program path that was installed *inside* the box to its physical
/// location in the materialized box tree.
pub fn resolve_boxed_program(box_dir: &Path, program: &str) -> PathBuf {
    let p = PathBuf::from(program);
    if p.is_file() {
        return p;
    }
    let trimmed = program.trim_start_matches('\\');
    // "C:\Program Files\X\X.exe" inside the box lives at <box>\drive\C\Program Files\X\X.exe
    if trimmed.len() > 2 && trimmed.as_bytes()[1] == b':' {
        let rest = &trimmed[2..];
        let rest = rest.trim_start_matches('\\');
        return box_dir.join("drive").join(&trimmed[..1]).join(rest);
    }
    // "drive\C\..." style relative path
    box_dir.join(trimmed)
}

/// One `SbieIni.exe` update. Going through SbieIni.exe matters: the client
/// hands the request to the SbieIniServer service, which runs as SYSTEM and is
/// therefore able to rewrite C:\Windows\Sandboxie.ini and reload the driver
/// configuration (a plain user process cannot do either).
pub fn ini_update(
    sbie_ini_exe: &Path,
    op: &str,
    section: &str,
    setting: &str,
    value: Option<&str>,
) -> Result<(), String> {
    let mut args: Vec<String> = vec![op.to_string(), section.to_string(), setting.to_string()];
    if let Some(v) = value {
        args.push(v.to_string());
    }
    let out = run(sbie_ini_exe, &args)?;
    if !out.ok {
        return Err(format!(
            "SbieIni {} {}/{} failed with exit {}: {}{}",
            op,
            section,
            setting,
            out.code,
            out.stdout.trim(),
            out.stderr.trim()
        ));
    }
    Ok(())
}

/// 8.3 short path of an existing file, or `None` when the volume has no short
/// names (or the query fails).
///
/// Needed because `SbieIni.exe` parses its own command line: it cannot carry a
/// literal double quote into `Sandboxie.ini`, so a quoted `RunCommand` is not
/// writable from here. A short path has no spaces, so it needs no quotes.
pub fn short_path(path: &Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    let ps = PathBuf::from(format!(
        "{}\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
        sysroot
    ));
    if !ps.is_file() {
        return None;
    }
    let script = format!(
        "(New-Object -ComObject Scripting.FileSystemObject).GetFile('{}').ShortPath",
        path.display()
    );
    let out = run(
        &ps,
        &[
            "-NoProfile".to_string(),
            "-NonInteractive".to_string(),
            "-Command".to_string(),
            script,
        ],
    )
    .ok()?;
    let text = out.stdout.trim().to_string();
    if text.is_empty() || text.contains(' ') || !text.contains('\\') {
        None
    } else {
        Some(text)
    }
}
