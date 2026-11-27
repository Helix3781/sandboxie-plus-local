//! Local Sandboxie-Plus box manager: shared logic for the `sbie-box` CLI and the
//! `sbie-mcp` MCP server. Everything runs on this machine; no network at all.
pub mod ini;
pub mod paths;
pub mod sbie;
pub mod snap;

use ini::RawIni;
use serde_json::{json, Value};
use std::path::PathBuf;

pub struct Env {
    pub ini_path: PathBuf,
    pub start_exe: PathBuf,
    pub sbie_ini_exe: PathBuf,
    /// True when the ini was redirected (SBIE_INI / SBIE_DIRECT_INI), i.e. a
    /// portable or test configuration that no SYSTEM service will write for us.
    pub direct_ini: bool,
    pub ini: RawIni,
}

impl Env {
    pub fn load() -> Result<Env, String> {
        let ini_path = paths::ini_path();
        let ini = RawIni::load(&ini_path).map_err(|e| format!("cannot read {}: {}", ini_path.display(), e))?;
        let direct_ini = std::env::var("SBIE_INI").map(|v| !v.trim().is_empty()).unwrap_or(false)
            || std::env::var("SBIE_DIRECT_INI").is_ok();
        Ok(Env {
            ini_path,
            start_exe: paths::tool("Start.exe"),
            sbie_ini_exe: paths::tool("SbieIni.exe"),
            direct_ini,
            ini,
        })
    }

    pub fn reload(&mut self) -> Result<(), String> {
        self.ini = RawIni::load(&self.ini_path).map_err(|e| format!("cannot read {}: {}", self.ini_path.display(), e))?;
        Ok(())
    }

    pub fn root(&self, box_name: &str) -> Result<PathBuf, String> {
        paths::box_root(&self.ini, box_name)
    }

    fn require_box(&self, box_name: &str) -> Result<PathBuf, String> {
        self.root(box_name)
    }
}

pub fn list_boxes(env: &Env) -> Vec<Value> {
    let mut out = Vec::new();
    for name in paths::list_boxes(&env.ini) {
        let root = paths::box_root(&env.ini, &name).ok();
        let current = root.as_ref().and_then(|r| snap::current(r));
        out.push(json!({
            "name": name,
            "root": root.map(|r| r.display().to_string()),
            "currentSnapshot": current,
        }));
    }
    out
}

pub fn box_status(env: &Env, box_name: &str) -> Result<Value, String> {
    let root = env.require_box(box_name)?;
    let pids = sbie::list_pids(&env.start_exe, box_name).unwrap_or_default();
    let snaps = snap::list(&root);
    Ok(json!({
        "box": box_name,
        "root": root.display().to_string(),
        "exists": root.is_dir(),
        "running": !pids.is_empty(),
        "pids": pids,
        "currentSnapshot": snap::current(&root),
        "snapshotCount": snaps.len(),
        "applicationCompartment": env.ini.get(box_name, "NoSecurityIsolation").map(|v| v.eq_ignore_ascii_case("y") || v == "1").unwrap_or(false),
    }))
}

pub fn list_snapshots(env: &Env, box_name: &str) -> Result<Value, String> {
    let root = env.require_box(box_name)?;
    let snaps = snap::list(&root);
    Ok(json!({
        "box": box_name,
        "current": snap::current(&root),
        "snapshots": snaps.iter().map(|s| json!({
            "id": s.id, "name": s.name, "parent": s.parent, "date": s.date
        })).collect::<Vec<_>>(),
    }))
}

pub fn restore(env: &Env, box_name: &str, id: &str, wait_ms: u64) -> Result<Value, String> {
    let root = env.require_box(box_name)?;
    sbie::terminate(&env.start_exe, box_name, wait_ms)?;
    let mut log = Vec::new();
    let report = snap::materialize(&root, id, &mut log)?;
    Ok(json!({
        "box": box_name,
        "snapshot": id,
        "restored": true,
        "chain": report.chain,
        "files": report.files,
        "log": log,
    }))
}

pub fn take_snapshot(env: &Env, box_name: &str, name: &str, wait_ms: u64) -> Result<Value, String> {
    let root = env.require_box(box_name)?;
    sbie::terminate(&env.start_exe, box_name, wait_ms)?;
    let mut log = Vec::new();
    let report = snap::take(&root, name, &mut log)?;
    Ok(json!({
        "box": box_name,
        "created": report.id,
        "files": report.files,
        "bytes": report.bytes,
        "log": log,
    }))
}

pub fn delete_snapshot(env: &Env, box_name: &str, id: &str) -> Result<Value, String> {
    let root = env.require_box(box_name)?;
    snap::delete(&root, id)?;
    Ok(json!({"box": box_name, "deleted": id}))
}

pub fn boot(
    env: &Env,
    box_name: &str,
    snapshot: Option<&str>,
    program: &str,
    args: &[String],
    wait_ms: u64,
    wait_for_window: Option<&str>,
) -> Result<Value, String> {
    let root = env.require_box(box_name)?;
    let mut log = Vec::new();
    let target = match snapshot {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => snap::current(&root).ok_or_else(|| format!("box {} has no current snapshot", box_name))?,
    };
    sbie::terminate(&env.start_exe, box_name, wait_ms)?;
    let report = snap::materialize(&root, &target, &mut log)?;

    let resolved = sbie::resolve_boxed_program(&root, program);
    if !resolved.is_file() {
        return Err(format!("program not found after restore: {}", resolved.display()));
    }
    let started = sbie::run_in_box(&env.start_exe, box_name, &resolved, args)?;
    let mut result = json!({
        "box": box_name,
        "snapshot": target,
        "files": report.files,
        "program": resolved.display().to_string(),
        "launchExitCode": started.code,
        "launchStdout": started.stdout.trim(),
        "launchStderr": started.stderr.trim(),
        "log": log,
    });

    if let Some(title) = wait_for_window {
        let found = wait_window(title, wait_ms)?;
        result["windowFound"] = json!(found);
    }
    Ok(result)
}

fn wait_window(title: &str, wait_ms: u64) -> Result<bool, String> {
    // Use tasklist to look for a visible window title without extra dependencies.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
    loop {
        let out = std::process::Command::new("tasklist")
            .args(["/v", "/fo", "csv", "/nh"])
            .output();
        if let Ok(out) = out {
            let text = String::from_utf8_lossy(&out.stdout);
            if text.to_lowercase().contains(&title.to_lowercase()) {
                return Ok(true);
            }
        }
        if std::time::Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

pub fn run_program(env: &Env, box_name: &str, program: &str, args: &[String]) -> Result<Value, String> {
    let root = env.require_box(box_name)?;
    let resolved = sbie::resolve_boxed_program(&root, program);
    if !resolved.is_file() {
        return Err(format!("program not found: {}", resolved.display()));
    }
    let out = sbie::run_in_box(&env.start_exe, box_name, &resolved, args)?;
    Ok(json!({
        "box": box_name,
        "program": resolved.display().to_string(),
        "exitCode": out.code,
        "stdout": out.stdout.trim(),
        "stderr": out.stderr.trim(),
    }))
}

pub fn terminate(env: &Env, box_name: &str, wait_ms: u64) -> Result<Value, String> {
    env.require_box(box_name)?;
    sbie::terminate(&env.start_exe, box_name, wait_ms)?;
    Ok(json!({"box": box_name, "terminated": true}))
}

/// One write against the box section of Sandboxie.ini.
pub struct IniOp {
    pub op: &'static str, // "set" (replace) | "append" (add value) | "remove_section"
    pub setting: String,
    pub value: Option<String>,
}

impl IniOp {
    pub fn set(key: &str, value: &str) -> IniOp {
        IniOp { op: "set", setting: key.to_string(), value: Some(value.to_string()) }
    }
    pub fn append(key: &str, value: &str) -> IniOp {
        IniOp { op: "append", setting: key.to_string(), value: Some(value.to_string()) }
    }
    pub fn remove_section() -> IniOp {
        IniOp { op: "remove_section", setting: "*".to_string(), value: None }
    }
}

/// Apply a batch of section writes. In a normal install this goes through
/// SbieIni.exe (SYSTEM service, can write C:\\Windows\\Sandboxie.ini and reloads
/// the driver); in a redirected/portable configuration the file is written
/// directly instead.
fn apply_ops(env: &mut Env, box_name: &str, ops: &[IniOp]) -> Result<(), String> {
    if env.direct_ini {
        if !ops.is_empty() && ops[0].op != "remove_section" {
            env.ini.ensure_section(box_name);
        }
        for op in ops {
            match op.op {
                "set" => env.ini.set(box_name, &op.setting, op.value.as_deref().unwrap_or("")),
                "append" => env.ini.append(box_name, &op.setting, op.value.as_deref().unwrap_or("")),
                "remove_section" => env.ini.remove_section(box_name),
                other => return Err(format!("internal: unknown ini op '{}'", other)),
            }
        }
        env.ini
            .save()
            .map_err(|e| format!("cannot save {}: {}", env.ini_path.display(), e))?;
    } else {
        for op in ops {
            if op.op == "remove_section" {
                sbie::ini_update(&env.sbie_ini_exe, "set", box_name, "*", None)?;
            } else {
                sbie::ini_update(&env.sbie_ini_exe, op.op, box_name, &op.setting, op.value.as_deref())?;
            }
        }
        env.reload()?;
    }
    Ok(())
}

/// Driver rule (`Box_IsValidName`): 1..=37 characters from [0-9A-Za-z_].
pub fn is_valid_box_name(name: &str) -> bool {
    if name.is_empty() || name.chars().count() > 37 {
        return false;
    }
    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The default key set SandMan's `CSandBox` constructor would fill in, written
/// explicitly so the box is complete even when SandMan never opens it.
fn default_box_ops(is_app: bool) -> Vec<IniOp> {
    let mut ops = Vec::new();
    ops.push(IniOp::set("Enabled", "y"));
    ops.push(IniOp::set("BlockNetworkFiles", "y"));
    ops.push(IniOp::append("RecoverFolder", "%Desktop%"));
    ops.push(IniOp::append("RecoverFolder", "%Personal%"));
    ops.push(IniOp::append("RecoverFolder", "%{374DE290-123F-4565-9164-39C4925E467B}%"));
    ops.push(IniOp::set("BorderColor", if is_app { "#00FD00,ttl" } else { "#02F6F6,ttl" }));
    for t in [
        "AutoRecoverIgnore",
        "LingerPrograms",
        "BlockPorts",
        "qWave",
        "FileCopy",
        "SkipHook",
        "OpenBluetooth",
    ] {
        ops.push(IniOp::append("Template", t));
    }
    if is_app {
        // Application compartment box (eAppBox): NoSecurityIsolation plus the
        // RPC port template. No UsePrivacyMode - that is the "with data
        // protection" variant (eAppBoxPlus), which is not offered.
        ops.push(IniOp::set("NoSecurityIsolation", "y"));
        ops.push(IniOp::append("Template", "RpcPortBindingsExt"));
    }
    ops.push(IniOp::set("UseFileDeleteV2", "y"));
    ops.push(IniOp::set("UseRegDeleteV2", "y"));
    ops.push(IniOp::set("AutoRecover", "y"));
    ops.push(IniOp::set("ConfigLevel", "10"));
    ops
}

pub fn create_box(env: &mut Env, box_name: &str, kind: &str) -> Result<Value, String> {
    if !is_valid_box_name(box_name) {
        return Err(format!(
            "invalid box name '{}': use 1-37 characters from A-Z a-z 0-9 _",
            box_name
        ));
    }
    if env.ini.sections().iter().any(|s| s.eq_ignore_ascii_case(box_name)) {
        return Err(format!("sandbox '{}' already exists", box_name));
    }
    let normalized = kind.to_ascii_lowercase();
    let is_app = match normalized.as_str() {
        "standard" | "std" | "default" => false,
        "app" | "application" | "appbox" | "compartment" => true,
        other => return Err(format!("unknown box type '{}' (use standard|app)", other)),
    };

    let mut ops = default_box_ops(is_app);
    // Keep new boxes alongside the existing ones (the driver ignores the global
    // BoxRootFolder unless the box itself says where it lives).
    if let Some(base) = env
        .ini
        .get("GlobalSettings", "BoxRootFolder")
        .map(|v| v.trim().trim_end_matches('\\').to_string())
        .filter(|v| !v.is_empty())
    {
        ops.insert(1, IniOp::set("FileRootPath", &format!("{}\\%SANDBOX%", base)));
    }
    apply_ops(env, box_name, &ops)?;

    let root = env.root(box_name)?;
    std::fs::create_dir_all(&root).map_err(|e| format!("cannot create {}: {}", root.display(), e))?;
    Ok(json!({
        "box": box_name,
        "type": if is_app { "app" } else { "standard" },
        "root": root.display().to_string(),
    }))
}

/// A deleted box must also leave `GlobalSettings/BoxGrouping`, or SandMan keeps
/// listing a sandbox that no longer exists.
fn forget_in_box_grouping(env: &mut Env, box_name: &str) {
    let grouping = match env.ini.get("GlobalSettings", "BoxGrouping") {
        Some(v) => v.to_string(),
        None => return,
    };
    let kept: Vec<&str> = grouping
        .split(',')
        .filter(|item| {
            let name = item.trim().trim_start_matches(':');
            !name.eq_ignore_ascii_case(box_name)
        })
        .collect();
    let joined = kept.join(",");
    if joined != grouping {
        let _ = apply_ops(env, "GlobalSettings", &[IniOp::set("BoxGrouping", &joined)]);
    }
}

pub fn delete_box(env: &mut Env, box_name: &str, purge: bool, wait_ms: u64) -> Result<Value, String> {
    let root = env.require_box(box_name)?;
    let _ = sbie::terminate(&env.start_exe, box_name, wait_ms);
    apply_ops(env, box_name, &[IniOp::remove_section()])?;
    forget_in_box_grouping(env, box_name);
    if purge && root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| format!("cannot remove {}: {}", root.display(), e))?;
    }
    Ok(json!({"box": box_name, "deleted": true, "purged": purge}))
}

/// Copy a portable ("green") program folder into a box and register it in the
/// box's Run Menu, so the program permanently lives inside the sandbox.
///
/// The copy lands at `<box root>\drive\<L>\<same path as on the host>`, because
/// inside a box the host's `C:\` is redirected to `drive\C\`. Keeping the
/// original absolute path means the program still finds its own DLLs and config,
/// while every write it makes stays inside the box.
pub fn import_program(
    env: &mut Env,
    box_name: &str,
    source: &str,
    exe: Option<&str>,
    label: Option<&str>,
    add_menu: bool,
) -> Result<Value, String> {
    use std::path::{Component, Path};

    let root = env.require_box(box_name)?;
    let src_raw = PathBuf::from(source);
    let src = std::fs::canonicalize(&src_raw)
        .map_err(|e| format!("cannot read source folder {}: {}", src_raw.display(), e))?;
    if !src.is_dir() {
        return Err(format!("source is not a folder: {}", src.display()));
    }
    if src.starts_with(&root) {
        return Err(format!("{} is already inside the box root {}", src.display(), root.display()));
    }

    let mut comps = src.components();
    let drive = match comps.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            std::path::Prefix::Disk(letter) | std::path::Prefix::VerbatimDisk(letter) => {
                Some((letter as char).to_ascii_uppercase())
            }
            _ => None,
        },
        _ => None,
    };
    let drive = drive.ok_or_else(|| {
        format!("source must be on a drive letter (got {}); import works with C:, D:, ...", src.display())
    })?;
    let parts: Vec<String> = comps
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        return Err("refusing to import a whole drive root".to_string());
    }

    let mut target = root.join("drive").join(drive.to_string());
    for part in &parts {
        target = target.join(part);
    }
    let existed = target.exists();
    std::fs::create_dir_all(&target).map_err(|e| format!("cannot create {}: {}", target.display(), e))?;
    let files = snap::copy_tree(&src, &target).map_err(|e| format!("copy into {} failed: {}", target.display(), e))?;

    // The path the program keeps inside the box: same drive letter, same folder.
    let boxed = format!("{}:\\{}", drive, parts.join("\\"));

    // Pick the executable: explicit --exe (relative to the folder), else a single
    // .exe sitting directly in the folder, else nothing.
    let exe_rel: Option<String> = match exe {
        Some(rel) => Some(rel.trim_start_matches('\\').replace('/', "\\")),
        None => {
            let mut found: Vec<String> = Vec::new();
            if let Ok(iter) = std::fs::read_dir(&src) {
                for entry in iter.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        if let Some(ext) = p.extension() {
                            if ext.eq_ignore_ascii_case("exe") {
                                found.push(entry.file_name().to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
            if found.len() == 1 {
                Some(found.remove(0))
            } else {
                None
            }
        }
    };

    let mut boxed_exe: Option<String> = None;
    let mut menu_note = String::from("Run Menu skipped (--no-menu)");
    if let Some(rel) = &exe_rel {
        let host_exe = src.join(rel);
        if !host_exe.is_file() {
            return Err(format!("executable not found in source folder: {}", rel));
        }
        let mut full = format!("{}\\{}", boxed, rel);
        boxed_exe = Some(full.clone());
        // SbieIni.exe parses its own command line and cannot carry a literal
        // double quote into the ini, so a quoted command line is impossible from
        // here. When the path contains a space, fall back to its 8.3 short name;
        // if the volume has no short names, skip the entry instead of writing one
        // that CreateProcess would misparse.
        let mut can_menu = add_menu;
        if add_menu && full.contains(' ') {
            match sbie::short_path(&target.join(rel)) {
                Some(short) => {
                    let marker = format!("\\drive\\{}\\", drive);
                    match short.to_lowercase().find(&marker.to_lowercase()) {
                        Some(idx) => {
                            full = format!("{}:\\{}", drive, &short[idx + marker.len()..]);
                        }
                        None => can_menu = false,
                    }
                }
                None => can_menu = false,
            }
        }
        if can_menu {
            let name = match label {
                Some(l) if !l.trim().is_empty() => l.trim().to_string(),
                _ => Path::new(rel)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| parts.last().cloned().unwrap_or_else(|| box_name.to_string())),
            };
            apply_ops(env, box_name, &[IniOp::append("RunCommand", &format!("{}|{}", name, full))])?;
            boxed_exe = Some(full);
            menu_note = format!("Run Menu: {}", name);
        } else if add_menu {
            menu_note = String::from(
                "Run Menu skipped: path contains a space and this volume has no 8.3 short name; add the entry in SandMan (box options > General > Run Menu)",
            );
        }
    } else if add_menu {
        menu_note = String::from("Run Menu skipped: no single .exe found (pass --exe NAME.exe)");
    }

    let src_disp = src.display().to_string();
    let src_disp = src_disp.strip_prefix("\\\\?\\").unwrap_or(&src_disp).to_string();

    Ok(json!({
        "box": box_name,
        "source": src_disp,
        "target": target.display().to_string(),
        "files": files,
        "overwritten": existed,
        "exe": boxed_exe,
        "run_menu": menu_note,
        "note": "start it from inside the box; the host copy is untouched"
    }))
}
