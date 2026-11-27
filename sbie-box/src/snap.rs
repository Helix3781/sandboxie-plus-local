//! Snapshot listing, materialization (the "restore" fix) and snapshot creation.
//!
//! SandMan's own SelectSnapshot only clears drive/user/share and copies the
//! registry hive back, relying on the driver to present lower layers read-only.
//! That breaks for boxes where a parent layer holds installed programs, so we
//! materialize the whole parent chain onto disk instead.
use crate::ini::{decode_utf16, RawIni};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const FILE_DELETED_FLAG: u32 = 0x0001;
pub const FILE_RELOCATION_FLAG: u32 = 0x0002;
pub const SUB_FOLDERS: [&str; 3] = ["drive", "user", "share"];

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub date: Option<String>,
}

fn snapshots_ini(box_dir: &Path) -> PathBuf {
    box_dir.join("Snapshots.ini")
}

pub fn list(box_dir: &Path) -> Vec<Snapshot> {
    let ini = RawIni::load_or_empty(&snapshots_ini(box_dir));
    let mut out = Vec::new();
    for section in ini.sections() {
        if let Some(id) = section.strip_prefix("Snapshot_") {
            let name = ini.get(&section, "Name").unwrap_or_default();
            let parent = ini
                .get(&section, "Parent")
                .filter(|p| !p.is_empty() && p != "0")
                .or_else(|| ini.get(&section, "Parrent").filter(|p| !p.is_empty() && p != "0"));
            // SandMan writes "SnapshotDate" as seconds since epoch (SandBox.cpp:484).
            // Older/foreign tools may use "DateTime" or "Date"; accept all three.
            let date = ini
                .get(&section, "SnapshotDate")
                .map(|secs| format_epoch_secs(&secs))
                .or_else(|| ini.get(&section, "DateTime"))
                .or_else(|| ini.get(&section, "Date"));
            out.push(Snapshot { id: id.to_string(), name, parent, date });
        }
    }
    out.sort_by(|a, b| natural_key(&a.id).cmp(&natural_key(&b.id)));
    out
}

fn natural_key(s: &str) -> (u64, String) {
    match s.parse::<u64>() {
        Ok(n) => (n, String::new()),
        Err(_) => (u64::MAX, s.to_string()),
    }
}

/// Format a seconds-since-epoch stamp as "yyyy-MM-dd HH:mm:ss" (UTC+8, same
/// offset `now_stamp` uses, so written and read stamps line up).
fn format_epoch_secs(raw: &str) -> String {
    match raw.trim().parse::<i64>() {
        Ok(secs) => {
            let secs = secs + 8 * 3600;
            let days = secs.div_euclid(86_400);
            let rem = secs.rem_euclid(86_400);
            let (y, m, d) = civil_from_days(days);
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                y,
                m,
                d,
                rem / 3600,
                (rem % 3600) / 60,
                rem % 60
            )
        }
        Err(_) => raw.to_string(),
    }
}

pub fn current(box_dir: &Path) -> Option<String> {
    let ini = RawIni::load_or_empty(&snapshots_ini(box_dir));
    ini.get("Current", "Snapshot").filter(|v| !v.is_empty())
}

pub fn set_current(box_dir: &Path, id: &str) -> io::Result<()> {
    let mut ini = RawIni::load_or_empty(&snapshots_ini(box_dir));
    ini.set("Current", "Snapshot", id);
    ini.save()
}

/// root .. target, in copy order (parents first).
pub fn chain(box_dir: &Path, target: &str) -> Result<Vec<String>, String> {
    let snaps: BTreeMap<String, Snapshot> = list(box_dir).into_iter().map(|s| (s.id.clone(), s)).collect();
    if !snaps.contains_key(target) {
        return Err(format!("snapshot {} does not exist in {}", target, box_dir.display()));
    }
    let mut out = vec![target.to_string()];
    let mut cursor = snaps.get(target).and_then(|s| s.parent.clone());
    let mut guard = 0;
    while let Some(parent) = cursor {
        if !snaps.contains_key(&parent) {
            break;
        }
        out.push(parent.clone());
        cursor = snaps.get(&parent).and_then(|s| s.parent.clone());
        guard += 1;
        if guard > 512 {
            return Err("snapshot parent chain is cyclic".to_string());
        }
    }
    out.reverse();
    Ok(out)
}

/// Apply one layer's deletion/relocation records (FilePaths.dat) to the box tree.
fn apply_layer_deletions(box_dir: &Path, layer_dir: &Path, log: &mut Vec<String>) {
    let dat = layer_dir.join("FilePaths.dat");
    if !dat.exists() {
        return;
    }
    let bytes = match fs::read(&dat) {
        Ok(b) => b,
        Err(_) => return,
    };
    let text = if crate::ini::looks_utf16(&bytes) {
        decode_utf16(&bytes)
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    for line in text.split('\n') {
        let line = line.trim_end_matches(['\r', '\u{0}']);
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(3, '|');
        let raw_path = parts.next().unwrap_or("").trim();
        let flags = parts
            .next()
            .and_then(|f| u32::from_str_radix(f.trim(), 16).ok())
            .unwrap_or(0);
        let relocation = parts.next().map(|r| r.trim().to_string()).filter(|r| !r.is_empty());
        if raw_path.is_empty() {
            continue;
        }
        let target = boxed_path(box_dir, raw_path);
        if flags & FILE_DELETED_FLAG != 0 {
            let _ = remove_any(&target);
        }
        if flags & FILE_RELOCATION_FLAG != 0 {
            if let Some(rel) = relocation {
                let dest = boxed_path(box_dir, &rel);
                let _ = move_any(&target, &dest);
            } else {
                log.push(format!("layer {}: relocation without target for {}", layer_dir.display(), raw_path));
            }
        }
    }
}

fn boxed_path(box_dir: &Path, raw: &str) -> PathBuf {
    let trimmed = raw.trim_start_matches('\\');
    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() {
        candidate
    } else {
        box_dir.join(candidate)
    }
}

fn remove_any(path: &Path) -> io::Result<()> {
    match fs::metadata(path) {
        Ok(m) if m.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(_) => Ok(()),
    }
}

fn move_any(from: &Path, to: &Path) -> io::Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if to.exists() {
        let _ = remove_any(to);
    }
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_tree(from, to)?;
            remove_any(from)
        }
    }
}

/// Recursive copy that overwrites existing files.
pub fn copy_tree(from: &Path, to: &Path) -> io::Result<u64> {
    let mut count = 0u64;
    let meta = match fs::metadata(from) {
        Ok(m) => m,
        Err(_) => return Ok(0),
    };
    if meta.is_file() {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
        return Ok(1);
    }
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            count += copy_tree(&src, &dst)?;
        } else if ft.is_file() {
            fs::create_dir_all(dst.parent().unwrap())?;
            fs::copy(&src, &dst)?;
            count += 1;
        }
        // symlinks/junctions are skipped on purpose: they are not snapshot content
    }
    Ok(count)
}

pub struct MaterializeReport {
    pub chain: Vec<String>,
    pub files: u64,
}

/// Full materialization of `target` onto the box root.
pub fn materialize(box_dir: &Path, target: &str, log: &mut Vec<String>) -> Result<MaterializeReport, String> {
    if !box_dir.is_dir() {
        return Err(format!("box folder not found: {}", box_dir.display()));
    }
    let chain = chain(box_dir, target)?;
    log.push(format!("materializing chain: {}", chain.join(" -> ")));

    // 1. non-recursive data files come from the target layer, FilePaths.dat is incremental
    let data_files: [(&str, bool, bool); 3] = [
        ("RegHive", true, false),
        ("RegPaths.dat", false, false),
        ("FilePaths.dat", false, true),
    ];
    for (name, required, recursive) in data_files {
        let dst = box_dir.join(name);
        if dst.exists() {
            fs::remove_file(&dst).map_err(|e| format!("cannot remove {}: {}", dst.display(), e))?;
        }
        if recursive {
            continue;
        }
        let src = box_dir.join(format!("snapshot-{}", target)).join(name);
        if src.is_file() {
            fs::copy(&src, &dst).map_err(|e| format!("cannot copy {}: {}", src.display(), e))?;
        } else if required {
            return Err(format!("required data file missing in snapshot {}: {}", target, name));
        }
    }

    // 2. clear the sandboxed file trees
    for sub in SUB_FOLDERS {
        let dir = box_dir.join(sub);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| format!("cannot clear {}: {}", dir.display(), e))?;
        }
        fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {}", dir.display(), e))?;
    }

    // 3. replay every layer: deletions first, then its files
    let mut files = 0u64;
    for layer in &chain {
        let layer_dir = box_dir.join(format!("snapshot-{}", layer));
        if !layer_dir.is_dir() {
            log.push(format!("layer {} folder missing, skipped", layer));
            continue;
        }
        apply_layer_deletions(box_dir, &layer_dir, log);
        for sub in SUB_FOLDERS {
            let src = layer_dir.join(sub);
            if src.is_dir() {
                let dst = box_dir.join(sub);
                files += copy_tree(&src, &dst).map_err(|e| format!("cannot copy {}: {}", src.display(), e))?;
            }
        }
    }

    set_current(box_dir, target).map_err(|e| format!("cannot update Snapshots.ini: {}", e))?;
    log.push(format!("materialized {} files from {} layer(s)", files, chain.len()));
    Ok(MaterializeReport { chain, files })
}

pub struct TakeReport {
    pub id: String,
    pub files: u64,
    pub bytes: u64,
}

/// Create a new snapshot of the current box state (box must not be running).
pub fn take(box_dir: &Path, name: &str, log: &mut Vec<String>) -> Result<TakeReport, String> {
    let mut ini = RawIni::load_or_empty(&snapshots_ini(box_dir));
    let existing = list(box_dir);
    let next_id = existing
        .iter()
        .filter_map(|s| s.id.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let id = next_id.to_string();
    let parent = ini.get("Current", "Snapshot").unwrap_or_default();
    let layer = box_dir.join(format!("snapshot-{}", id));
    if layer.exists() {
        return Err(format!("layer folder already exists: {}", layer.display()));
    }
    fs::create_dir_all(&layer).map_err(|e| format!("cannot create {}: {}", layer.display(), e))?;

    let mut files = 0u64;
    let mut bytes = 0u64;
    for sub in SUB_FOLDERS {
        let src = box_dir.join(sub);
        if src.is_dir() {
            let (f, b) = copy_tree_sized(&src, &layer.join(sub))?;
            files += f;
            bytes += b;
        }
    }
    // data files move into the layer, mirroring SandMan's TakeSnapshot
    for (name, _, _) in [("RegHive", true, false), ("RegPaths.dat", false, false), ("FilePaths.dat", false, true)] {
        let src = box_dir.join(name);
        if src.is_file() {
            let dst = layer.join(name);
            if fs::rename(&src, &dst).is_err() {
                fs::copy(&src, &dst).map_err(|e| format!("cannot copy {}: {}", src.display(), e))?;
                fs::remove_file(&src).ok();
            }
            if name == "RegHive" {
                // the box root needs a fresh hive, SandMan recreates it on start
                fs::copy(&dst, &src).ok();
            }
        }
    }
    for sub in SUB_FOLDERS {
        let dir = box_dir.join(sub);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| format!("cannot clear {}: {}", dir.display(), e))?;
        }
        fs::create_dir_all(&dir).ok();
    }

    let section = format!("Snapshot_{}", id);
    ini.ensure_section(&section);
    let stamp = now_stamp();
    ini.set(&section, "Name", if name.is_empty() { &stamp } else { name });
    if !parent.is_empty() {
        ini.set(&section, "Parent", &parent);
    }
    // SandMan stores the snapshot date as seconds since epoch (SandBox.cpp:484);
    // keep a readable copy too, but the epoch key is the authoritative one.
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    ini.set(&section, "SnapshotDate", &epoch.to_string());
    ini.set(&section, "DateTime", &stamp);
    ini.set("Current", "Snapshot", &id);
    ini.save().map_err(|e| format!("cannot save Snapshots.ini: {}", e))?;
    log.push(format!("created snapshot {} ({} files, {} bytes)", id, files, bytes));
    Ok(TakeReport { id, files, bytes })
}

fn copy_tree_sized(from: &Path, to: &Path) -> Result<(u64, u64), String> {
    fs::create_dir_all(to).map_err(|e| format!("cannot create {}: {}", to.display(), e))?;
    let mut files = 0u64;
    let mut bytes = 0u64;
    let entries = fs::read_dir(from).map_err(|e| format!("cannot read {}: {}", from.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let ft = entry.file_type().map_err(|e| e.to_string())?;
        if ft.is_dir() {
            let (f, b) = copy_tree_sized(&src, &dst)?;
            files += f;
            bytes += b;
        } else if ft.is_file() {
            fs::copy(&src, &dst).map_err(|e| format!("cannot copy {}: {}", src.display(), e))?;
            files += 1;
            bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    Ok((files, bytes))
}

pub fn delete(box_dir: &Path, id: &str) -> Result<(), String> {
    let ini = RawIni::load_or_empty(&snapshots_ini(box_dir));
    if ini.get(&format!("Snapshot_{}", id), "Name").is_none()
        && !box_dir.join(format!("snapshot-{}", id)).exists()
    {
        return Err(format!("snapshot {} not found", id));
    }
    let layer = box_dir.join(format!("snapshot-{}", id));
    if layer.exists() {
        fs::remove_dir_all(&layer).map_err(|e| format!("cannot remove {}: {}", layer.display(), e))?;
    }
    let mut ini = ini;
    ini.remove_section(&format!("Snapshot_{}", id));
    ini.save().map_err(|e| e.to_string())
}

pub fn now_stamp() -> String {
    // Local time without extra crates: derive from the system clock in UTC+8.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        + 8 * 3600;
    let days = secs / 86400;
    let rem = secs % 86400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
