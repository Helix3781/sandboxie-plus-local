//! Sandboxie configuration locations and path resolution.
use crate::ini::RawIni;
use std::env;
use std::path::PathBuf;

pub fn ini_path() -> PathBuf {
    if let Ok(p) = env::var("SBIE_INI") {
        return PathBuf::from(p);
    }
    let windir = env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
    PathBuf::from(windir).join("Sandboxie.ini")
}

pub fn sbie_home() -> PathBuf {
    if let Ok(p) = env::var("SBIE_HOME") {
        return PathBuf::from(p);
    }
    let pf = env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());
    PathBuf::from(pf).join("Sandboxie-Plus")
}

pub fn tool(name: &str) -> PathBuf {
    sbie_home().join(name)
}

pub fn expand_vars(raw: &str, box_name: &str) -> String {
    let user = env::var("USERNAME").unwrap_or_default();
    let mut out = String::with_capacity(raw.len());
    let bytes: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '%' {
            if let Some(rel_end) = bytes[i + 1..].iter().position(|c| *c == '%') {
                let end = i + 1 + rel_end;
                let name: String = bytes[i + 1..end].iter().collect();
                let value = match name.to_ascii_uppercase().as_str() {
                    "SANDBOX" => Some(box_name.to_string()),
                    "USER" => Some(user.clone()),
                    other => env::var(other).ok(),
                };
                if let Some(v) = value {
                    out.push_str(&v);
                    i = end + 1;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// Resolve the on-disk root folder of a sandbox.
pub fn box_root(ini: &RawIni, box_name: &str) -> Result<PathBuf, String> {
    if ini.get(box_name, "FileRootPath").is_none() && !ini.sections().iter().any(|s| s.eq_ignore_ascii_case(box_name)) {
        return Err(format!("sandbox '{}' not found in {}", box_name, ini.path.display()));
    }
    // 1) FileRootPath is the full box folder.
    if let Some(p) = ini.get(box_name, "FileRootPath") {
        return Ok(PathBuf::from(strip_nt_prefix(&expand_vars(&p, box_name))));
    }
    // 2) BoxRootFolder (box level, else GlobalSettings) is a PARENT folder; the driver
    //    appends "\\Sandbox\\<BoxName>" to it (Box_InitPaths / Driver_Sandbox).
    let template = ini
        .get(box_name, "BoxRootFolder")
        .or_else(|| ini.get("GlobalSettings", "BoxRootFolder"))
        .ok_or_else(|| "neither FileRootPath nor BoxRootFolder is configured".to_string())?;
    let base = strip_nt_prefix(&expand_vars(&template, box_name));
    let base = base.trim_end_matches('\\').to_string();
    Ok(PathBuf::from(base).join("Sandbox").join(box_name))
}

fn strip_nt_prefix(path: &str) -> String {
    let mut p = path.trim().to_string();
    if p.starts_with("\\??\\") {
        p = p[4..].to_string();
    } else if let Some(rest) = p.strip_prefix("\\\\?\\") {
        p = rest.to_string();
    }
    p
}

pub fn list_boxes(ini: &RawIni) -> Vec<String> {
    let mut out = Vec::new();
    for name in ini.sections() {
        let lower = name.to_ascii_lowercase();
        if lower.starts_with("global") || lower.starts_with("user") || lower.starts_with("template") || lower.starts_with("defaultbox") {
            continue;
        }
        out.push(name);
    }
    out
}
