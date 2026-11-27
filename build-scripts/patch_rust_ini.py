import io, re, sys

def read(p):
    with io.open(p, 'r', encoding='utf-8') as f:
        return f.read()

def write(p, s):
    with io.open(p, 'w', encoding='utf-8', newline='') as f:
        f.write(s)

# ---------- 1. ini.rs : add append() ----------
p = 'src/ini.rs'
s = read(p)
if 'pub fn append(' in s:
    print('ini.rs: append already present')
else:
    anchor = '''    pub fn remove_section(&mut self, section: &str) {'''
    add = '''    /// Append one more `key=value` line for a multi-valued setting,
    /// keeping any existing lines that share the same key.
    pub fn append(&mut self, section: &str, key: &str, value: &str) {
        if let Some((start, end)) = self.section_range(section) {
            let mut last: Option<usize> = None;
            for i in start + 1..end {
                if let Some((k, _)) = split_kv(&self.lines[i]) {
                    if k.eq_ignore_ascii_case(key) {
                        last = Some(i);
                    }
                }
            }
            let at = match last {
                Some(i) => i + 1,
                None => end,
            };
            self.lines.insert(at, format!("{}={}", key, value));
            return;
        }
        if !self.lines.is_empty() && !self.lines.last().unwrap().trim().is_empty() {
            self.lines.push(String::new());
        }
        self.lines.push(format!("[{}]", section));
        self.lines.push(format!("{}={}", key, value));
    }

'''
    assert anchor in s, 'ini.rs anchor missing'
    s = s.replace(anchor, add + anchor, 1)
    write(p, s)
    print('ini.rs: append added')

# ---------- 2. sbie.rs : add ini_update() ----------
p = 'src/sbie.rs'
s = read(p)
if 'pub fn ini_update(' in s:
    print('sbie.rs: ini_update already present')
else:
    s = s.rstrip() + '''

/// One `SbieIni.exe` update. Going through SbieIni.exe matters: the client
/// hands the request to the SbieIniServer service, which runs as SYSTEM and is
/// therefore able to rewrite C:\\Windows\\Sandboxie.ini and reload the driver
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
'''
    write(p, s)
    print('sbie.rs: ini_update added')

# ---------- 3. lib.rs ----------
p = 'src/lib.rs'
s = read(p)

s = s.replace('''pub struct Env {
    pub ini_path: PathBuf,
    pub start_exe: PathBuf,
    pub ini: RawIni,
}''', '''pub struct Env {
    pub ini_path: PathBuf,
    pub start_exe: PathBuf,
    pub sbie_ini_exe: PathBuf,
    /// True when the ini was redirected (SBIE_INI / SBIE_DIRECT_INI), i.e. a
    /// portable or test configuration that no SYSTEM service will write for us.
    pub direct_ini: bool,
    pub ini: RawIni,
}''', 1)
assert 'pub sbie_ini_exe' in s

s = s.replace('''        let ini = RawIni::load(&ini_path).map_err(|e| format!("cannot read {}: {}", ini_path.display(), e))?;
        Ok(Env { ini_path, start_exe: paths::tool("Start.exe"), ini })''', '''        let ini = RawIni::load(&ini_path).map_err(|e| format!("cannot read {}: {}", ini_path.display(), e))?;
        let direct_ini = std::env::var("SBIE_INI").map(|v| !v.trim().is_empty()).unwrap_or(false)
            || std::env::var("SBIE_DIRECT_INI").is_ok();
        Ok(Env {
            ini_path,
            start_exe: paths::tool("Start.exe"),
            sbie_ini_exe: paths::tool("SbieIni.exe"),
            direct_ini,
            ini,
        })''', 1)
assert 'sbie_ini_exe: paths::tool' in s

# insert IniOp + apply_ops before create_box
ops_code = '''/// One write against the box section of Sandboxie.ini.
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
/// SbieIni.exe (SYSTEM service, can write C:\\\\Windows\\\\Sandboxie.ini and reloads
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
    ops.push(IniOp::set("BorderColor", if is_app { "#E8E803,ttl" } else { "#02F6F6,ttl" }));
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
        // Application compartment box with data protection (eAppBoxPlus):
        // NoSecurityIsolation + UsePrivacyMode, plus the RPC port template.
        ops.push(IniOp::set("NoSecurityIsolation", "y"));
        ops.push(IniOp::set("UsePrivacyMode", "y"));
        ops.push(IniOp::append("Template", "RpcPortBindingsExt"));
    }
    ops.push(IniOp::set("UseFileDeleteV2", "y"));
    ops.push(IniOp::set("UseRegDeleteV2", "y"));
    ops.push(IniOp::set("AutoRecover", "y"));
    ops.push(IniOp::set("ConfigLevel", "10"));
    ops
}

'''
anchor = 'pub fn create_box(env: &mut Env, box_name: &str, kind: &str) -> Result<Value, String> {'
assert anchor in s
s = s.replace(anchor, ops_code + anchor, 1)

# replace create_box body
start = s.index(anchor)
# find end of function: next "\npub fn delete_box"
end = s.index('pub fn delete_box(', start)
new_create = '''pub fn create_box(env: &mut Env, box_name: &str, kind: &str) -> Result<Value, String> {
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
        .map(|v| v.trim().trim_end_matches('\\\\').to_string())
        .filter(|v| !v.is_empty())
    {
        ops.insert(1, IniOp::set("FileRootPath", &format!("{}\\\\%SANDBOX%", base)));
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

'''
s = s[:start] + new_create + s[end:]

# replace delete_box body
dstart = s.index('pub fn delete_box(env: &mut Env, box_name: &str, purge: bool, wait_ms: u64) -> Result<Value, String> {')
# end: next "\n}\n" after ... find next "\npub fn "
try:
    dend = s.index('\npub fn ', dstart + 10)
except ValueError:
    dend = len(s)
new_del = '''pub fn delete_box(env: &mut Env, box_name: &str, purge: bool, wait_ms: u64) -> Result<Value, String> {
    let root = env.require_box(box_name)?;
    let _ = sbie::terminate(&env.start_exe, box_name, wait_ms);
    apply_ops(env, box_name, &[IniOp::remove_section()])?;
    if purge && root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| format!("cannot remove {}: {}", root.display(), e))?;
    }
    Ok(json!({"box": box_name, "deleted": true, "purged": purge}))
}
'''
s = s[:dstart] + new_del + s[dend:]

write(p, s)
print('lib.rs: patched')
