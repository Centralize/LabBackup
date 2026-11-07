//! The backup engine: makes dated zip archives from folders, lists them,
//! restores files from them and verifies them. Runs on a worker thread and
//! reports progress over a channel.

use crate::config::BackupJob;
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use zip::write::SimpleFileOptions;
use zip::{AesMode, CompressionMethod, ZipArchive, ZipWriter};

pub const MANIFEST_NAME: &str = "labbackup-manifest.json";
const DATA_PREFIX: &str = "data/";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceInfo {
    pub index: usize,
    pub path: PathBuf,
    pub folder_name: String,
    pub is_file: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub app: String,
    pub format: u32,
    pub job_id: u64,
    pub job_name: String,
    pub created: DateTime<Local>,
    pub host: String,
    pub sources: Vec<SourceInfo>,
    pub file_count: u64,
    pub total_bytes: u64,
    pub encrypted: bool,
    pub skipped: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ArchiveInfo {
    pub path: PathBuf,
    pub manifest: Manifest,
    pub size_on_disk: u64,
}

#[derive(Clone, Debug)]
pub struct ArchiveEntry {
    pub zip_index: usize,
    pub source: usize,
    pub rel: String,
    pub size: u64,
    pub is_dir: bool,
    pub modified: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    Overwrite,
    Skip,
    KeepBoth,
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub phase: String,
    pub done_files: u64,
    pub total_files: u64,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub current: String,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        if self.total_bytes > 0 {
            (self.done_bytes as f64 / self.total_bytes as f64) as f32
        } else if self.total_files > 0 {
            self.done_files as f32 / self.total_files as f32
        } else {
            0.0
        }
    }
}

pub enum WorkerMsg {
    Progress(Progress),
    Done(Result<String, String>),
}

pub struct Reporter {
    tx: Sender<WorkerMsg>,
    cancel: Arc<AtomicBool>,
    repaint: Box<dyn Fn() + Send>,
    last: std::time::Instant,
}

impl Reporter {
    pub fn new(tx: Sender<WorkerMsg>, cancel: Arc<AtomicBool>, repaint: Box<dyn Fn() + Send>) -> Self {
        Self { tx, cancel, repaint, last: std::time::Instant::now() }
    }

    fn progress(&mut self, p: &Progress, force: bool) {
        if force || self.last.elapsed().as_millis() > 60 {
            self.last = std::time::Instant::now();
            let _ = self.tx.send(WorkerMsg::Progress(p.clone()));
            (self.repaint)();
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn done(self, result: Result<String, String>) {
        let _ = self.tx.send(WorkerMsg::Done(result));
        (self.repaint)();
    }
}

// ---------------------------------------------------------------------------
// Helpers

pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

/// Case-insensitive wildcard match supporting `*` and `?`.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn is_excluded(name: &str, excludes: &[String]) -> bool {
    excludes.iter().map(|e| e.trim()).filter(|e| !e.is_empty()).any(|e| wildcard_match(e, name))
}

fn safe_name(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() { "backup".into() } else { trimmed }
}

fn to_zip_time(t: std::time::SystemTime) -> zip::DateTime {
    let dt: DateTime<Local> = t.into();
    zip::DateTime::from_date_and_time(
        dt.year().clamp(1980, 2107) as u16,
        dt.month() as u8,
        dt.day() as u8,
        dt.hour() as u8,
        dt.minute() as u8,
        dt.second() as u8,
    )
    .unwrap_or_default()
}

fn from_zip_time(z: &zip::DateTime) -> Option<DateTime<Local>> {
    let naive = NaiveDate::from_ymd_opt(z.year() as i32, z.month() as u32, z.day() as u32)?
        .and_hms_opt(z.hour() as u32, z.minute() as u32, z.second() as u32)?;
    Local.from_local_datetime(&naive).earliest()
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "this computer".into())
}

// ---------------------------------------------------------------------------
// Backup

struct PlannedFile {
    source: usize,
    abs: PathBuf,
    rel: String,
    size: u64,
    modified: Option<std::time::SystemTime>,
    is_dir: bool,
}

pub fn run_backup(job: &BackupJob, password: Option<&str>, rep: &mut Reporter) -> Result<String, String> {
    if job.sources.is_empty() {
        return Err("Add at least one folder to back up.".into());
    }
    if job.destination.as_os_str().is_empty() {
        return Err("Choose where the backups should be saved.".into());
    }
    let mut progress = Progress { phase: "Scanning files".into(), ..Default::default() };
    rep.progress(&progress, true);

    // 1. Plan: walk every source and collect the files to store.
    let dest_canon = fs::canonicalize(&job.destination).ok();
    let mut sources = Vec::new();
    let mut planned = Vec::new();
    let mut skipped = Vec::new();
    for (i, src) in job.sources.iter().enumerate() {
        let meta = fs::metadata(src).map_err(|e| format!("Can't open {}: {e}", src.display()))?;
        let is_file = meta.is_file();
        let folder_name = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("source-{}", i + 1));
        sources.push(SourceInfo { index: i, path: src.clone(), folder_name, is_file });
        if is_file {
            planned.push(PlannedFile {
                source: i,
                abs: src.clone(),
                rel: src.file_name().unwrap_or_default().to_string_lossy().to_string(),
                size: meta.len(),
                modified: meta.modified().ok(),
                is_dir: false,
            });
            continue;
        }
        let walker = walkdir::WalkDir::new(src).follow_links(false).into_iter().filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            if is_excluded(&e.file_name().to_string_lossy(), &job.excludes) {
                return false;
            }
            // Never back up the backup folder into itself.
            match (&dest_canon, e.file_type().is_dir()) {
                (Some(d), true) => fs::canonicalize(e.path()).map(|p| &p != d).unwrap_or(true),
                _ => true,
            }
        });
        for entry in walker {
            if rep.cancelled() {
                return Err("Cancelled.".into());
            }
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    skipped.push(format!("{e}"));
                    continue;
                }
            };
            if entry.depth() == 0 || entry.file_type().is_symlink() {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(src)
                .unwrap_or(entry.path())
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            let meta = entry.metadata().ok();
            let is_dir = entry.file_type().is_dir();
            planned.push(PlannedFile {
                source: i,
                abs: entry.path().to_path_buf(),
                rel,
                size: if is_dir { 0 } else { meta.as_ref().map(|m| m.len()).unwrap_or(0) },
                modified: meta.and_then(|m| m.modified().ok()),
                is_dir,
            });
            progress.total_files += 1;
            if progress.total_files % 200 == 0 {
                progress.current = entry.path().display().to_string();
                rep.progress(&progress, false);
            }
        }
    }
    progress.total_files = planned.iter().filter(|p| !p.is_dir).count() as u64;
    progress.total_bytes = planned.iter().map(|p| p.size).sum();
    progress.phase = "Backing up".into();

    // 2. Write the archive to a temporary name, then rename when complete.
    fs::create_dir_all(&job.destination)
        .map_err(|e| format!("Can't create {}: {e}", job.destination.display()))?;
    let created = Local::now();
    let base = format!("{}_{}", safe_name(&job.name), created.format("%Y-%m-%d_%H-%M-%S"));
    let final_path = job.destination.join(format!("{base}.zip"));
    let partial_path = job.destination.join(format!("{base}.zip.partial"));

    let result = write_archive(job, password, &planned, &sources, created, &partial_path, &mut progress, &mut skipped, rep);
    match result {
        Ok(()) => {
            fs::rename(&partial_path, &final_path).map_err(|e| e.to_string())?;
        }
        Err(e) => {
            let _ = fs::remove_file(&partial_path);
            return Err(e);
        }
    }

    // 3. Retention: drop the oldest backups of this set beyond `keep_last`.
    let mut removed = 0;
    if job.keep_last > 0 {
        let mut mine: Vec<ArchiveInfo> =
            list_archives(&job.destination).into_iter().filter(|a| a.manifest.job_id == job.id).collect();
        mine.sort_by(|a, b| b.manifest.created.cmp(&a.manifest.created));
        for old in mine.iter().skip(job.keep_last as usize) {
            if fs::remove_file(&old.path).is_ok() {
                removed += 1;
            }
        }
    }

    let size = fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);
    let mut msg = format!(
        "Backed up {} files ({}) into {} ({}).",
        progress.done_files,
        human_bytes(progress.total_bytes),
        final_path.file_name().unwrap_or_default().to_string_lossy(),
        human_bytes(size)
    );
    if !skipped.is_empty() {
        msg.push_str(&format!(" {} item(s) couldn't be read and were skipped.", skipped.len()));
    }
    if removed > 0 {
        msg.push_str(&format!(" Removed {removed} old backup(s)."));
    }
    Ok(msg)
}

#[allow(clippy::too_many_arguments)]
fn write_archive(
    job: &BackupJob,
    password: Option<&str>,
    planned: &[PlannedFile],
    sources: &[SourceInfo],
    created: DateTime<Local>,
    path: &Path,
    progress: &mut Progress,
    skipped: &mut Vec<String>,
    rep: &mut Reporter,
) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("Can't write backup file: {e}"))?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let method = if job.compress { CompressionMethod::Deflated } else { CompressionMethod::Stored };
    let mut buf = vec![0u8; 256 * 1024];

    for p in planned {
        if rep.cancelled() {
            return Err("Cancelled.".into());
        }
        let name = format!("{DATA_PREFIX}{}/{}", p.source, p.rel);
        let mut opts = SimpleFileOptions::default()
            .compression_method(method)
            .large_file(p.size >= u32::MAX as u64 / 2);
        if let Some(t) = p.modified {
            opts = opts.last_modified_time(to_zip_time(t));
        }
        if p.is_dir {
            zip.add_directory(name, opts).map_err(|e| e.to_string())?;
            continue;
        }
        let opts = match password {
            Some(pw) => opts.with_aes_encryption(AesMode::Aes256, pw),
            None => opts,
        };
        progress.current = p.abs.display().to_string();
        let src = match File::open(&p.abs) {
            Ok(f) => f,
            Err(e) => {
                skipped.push(format!("{}: {e}", p.abs.display()));
                progress.done_bytes += p.size;
                continue;
            }
        };
        zip.start_file(name, opts).map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(src);
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    skipped.push(format!("{} (partly read): {e}", p.abs.display()));
                    break;
                }
            };
            zip.write_all(&buf[..n]).map_err(|e| format!("Writing backup failed: {e}"))?;
            progress.done_bytes += n as u64;
            rep.progress(progress, false);
            if rep.cancelled() {
                return Err("Cancelled.".into());
            }
        }
        progress.done_files += 1;
        rep.progress(progress, false);
    }

    let manifest = Manifest {
        app: "LabBackup".into(),
        format: 1,
        job_id: job.id,
        job_name: job.name.clone(),
        created,
        host: hostname(),
        sources: sources.to_vec(),
        file_count: progress.done_files,
        total_bytes: progress.total_bytes,
        encrypted: password.is_some(),
        skipped: skipped.iter().take(200).cloned().collect(),
    };
    zip.start_file(MANIFEST_NAME, SimpleFileOptions::default().compression_method(CompressionMethod::Deflated))
        .map_err(|e| e.to_string())?;
    zip.write_all(serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut inner = zip.finish().map_err(|e| format!("Finishing backup failed: {e}"))?;
    inner.flush().map_err(|e| e.to_string())?;
    progress.phase = "Finished".into();
    rep.progress(progress, true);
    Ok(())
}

// ---------------------------------------------------------------------------
// Listing

pub fn read_manifest(path: &Path) -> Result<ArchiveInfo, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let size_on_disk = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|e| format!("Not a readable backup: {e}"))?;
    let mut text = String::new();
    zip.by_name(MANIFEST_NAME)
        .map_err(|_| "This zip file wasn't made by LabBackup.".to_string())?
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    let manifest: Manifest = serde_json::from_str(&text).map_err(|e| format!("Backup details unreadable: {e}"))?;
    Ok(ArchiveInfo { path: path.to_path_buf(), manifest, size_on_disk })
}

/// All LabBackup archives in a folder, newest first.
pub fn list_archives(dir: &Path) -> Vec<ArchiveInfo> {
    let mut out: Vec<ArchiveInfo> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("zip")).unwrap_or(false))
        .filter_map(|p| read_manifest(&p).ok())
        .collect();
    out.sort_by(|a, b| b.manifest.created.cmp(&a.manifest.created));
    out
}

pub fn read_entries(path: &Path) -> Result<Vec<ArchiveEntry>, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let f = zip.by_index_raw(i).map_err(|e| e.to_string())?;
        let name = f.name().map_err(|e| e.to_string())?.to_string();
        let Some(rest) = name.strip_prefix(DATA_PREFIX) else { continue };
        let Some((idx, rel)) = rest.split_once('/') else { continue };
        let Ok(source) = idx.parse::<usize>() else { continue };
        let rel = rel.trim_end_matches('/').to_string();
        if rel.is_empty() {
            continue;
        }
        out.push(ArchiveEntry {
            zip_index: i,
            source,
            rel,
            size: f.size(),
            is_dir: f.is_dir(),
            modified: f.last_modified().and_then(|d| from_zip_time(&d)).map(|d| d.format("%Y-%m-%d %H:%M").to_string()),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Restore and verify

fn restore_target(info: &ArchiveInfo, entry: &ArchiveEntry, into: Option<&Path>) -> Option<PathBuf> {
    let src = info.manifest.sources.iter().find(|s| s.index == entry.source)?;
    // Reject any path that tries to escape the restore folder.
    let rel = Path::new(&entry.rel);
    if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return None;
    }
    Some(match (into, src.is_file) {
        (Some(dir), true) => dir.join(rel),
        (Some(dir), false) => dir.join(&src.folder_name).join(rel),
        (None, true) => src.path.parent().unwrap_or(Path::new(".")).join(rel),
        (None, false) => src.path.join(rel),
    })
}

fn keep_both_name(path: &Path) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for n in 1.. {
        let candidate = path.with_file_name(format!("{stem} (restored {n}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn open_entry<'a, R: Read + io::Seek>(
    zip: &'a mut ZipArchive<R>,
    i: usize,
    password: Option<&str>,
) -> Result<zip::read::ZipFile<'a, R>, String> {
    let r = match password {
        Some(pw) => zip.by_index_decrypt(i, pw.as_bytes()),
        None => zip.by_index(i),
    };
    r.map_err(|e| match e {
        zip::result::ZipError::InvalidPassword => "The password is wrong.".to_string(),
        zip::result::ZipError::UnsupportedArchive(m) if m.to_lowercase().contains("password") => {
            "This backup is password protected. Enter its password.".to_string()
        }
        other => other.to_string(),
    })
}

pub fn run_restore(
    info: &ArchiveInfo,
    entries: &[ArchiveEntry],
    into: Option<&Path>,
    conflict: Conflict,
    password: Option<&str>,
    rep: &mut Reporter,
) -> Result<String, String> {
    let file = File::open(&info.path).map_err(|e| e.to_string())?;
    let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    let mut progress = Progress {
        phase: "Restoring".into(),
        total_files: entries.iter().filter(|e| !e.is_dir).count() as u64,
        total_bytes: entries.iter().map(|e| e.size).sum(),
        ..Default::default()
    };
    let (mut written, mut skipped_existing, mut failed) = (0u64, 0u64, Vec::new());
    let mut buf = vec![0u8; 256 * 1024];

    for entry in entries {
        if rep.cancelled() {
            return Err(format!("Cancelled after restoring {written} files."));
        }
        let Some(mut target) = restore_target(info, entry, into) else {
            failed.push(format!("{}: unsafe path", entry.rel));
            continue;
        };
        if entry.is_dir {
            let _ = fs::create_dir_all(&target);
            continue;
        }
        progress.current = target.display().to_string();
        if target.exists() {
            match conflict {
                Conflict::Skip => {
                    skipped_existing += 1;
                    progress.done_files += 1;
                    progress.done_bytes += entry.size;
                    rep.progress(&progress, false);
                    continue;
                }
                Conflict::KeepBoth => target = keep_both_name(&target),
                Conflict::Overwrite => {}
            }
        }
        if let Some(parent) = target.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                failed.push(format!("{}: {e}", parent.display()));
                continue;
            }
        }
        let mut zf = match open_entry(&mut zip, entry.zip_index, password) {
            Ok(f) => f,
            Err(e) if e.contains("password") => return Err(e),
            Err(e) => {
                failed.push(format!("{}: {e}", entry.rel));
                continue;
            }
        };
        let modified = zf.last_modified().and_then(|d| from_zip_time(&d));
        // Write to a temporary file first so a failed restore never leaves half a file.
        let tmp = target.with_file_name(format!(
            ".{}.labbackup-tmp",
            target.file_name().unwrap_or_default().to_string_lossy()
        ));
        let res = (|| -> Result<(), String> {
            let mut out = BufWriter::new(File::create(&tmp).map_err(|e| e.to_string())?);
            loop {
                let n = zf.read(&mut buf).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                progress.done_bytes += n as u64;
                rep.progress(&progress, false);
            }
            out.flush().map_err(|e| e.to_string())?;
            drop(out);
            if target.exists() {
                fs::remove_file(&target).map_err(|e| e.to_string())?;
            }
            fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
            Ok(())
        })();
        match res {
            Ok(()) => {
                if let Some(m) = modified {
                    let ft = filetime::FileTime::from_unix_time(m.timestamp(), 0);
                    let _ = filetime::set_file_mtime(&target, ft);
                }
                written += 1;
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                if e.to_lowercase().contains("password") {
                    return Err("The password is wrong.".into());
                }
                failed.push(format!("{}: {e}", entry.rel));
            }
        }
        progress.done_files += 1;
        rep.progress(&progress, false);
    }
    progress.phase = "Finished".into();
    rep.progress(&progress, true);

    let where_to = match into {
        Some(d) => d.display().to_string(),
        None => "their original locations".into(),
    };
    let mut msg = format!("Restored {written} files to {where_to}.");
    if skipped_existing > 0 {
        msg.push_str(&format!(" Skipped {skipped_existing} that already existed."));
    }
    if !failed.is_empty() {
        msg.push_str(&format!(" {} failed (first: {}).", failed.len(), failed[0]));
        return Err(msg);
    }
    Ok(msg)
}

/// Reads every file in the archive and checks its checksum.
pub fn run_verify(info: &ArchiveInfo, password: Option<&str>, rep: &mut Reporter) -> Result<String, String> {
    let entries = read_entries(&info.path)?;
    let file = File::open(&info.path).map_err(|e| e.to_string())?;
    let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|e| e.to_string())?;
    let mut progress = Progress {
        phase: "Verifying".into(),
        total_files: entries.iter().filter(|e| !e.is_dir).count() as u64,
        total_bytes: entries.iter().map(|e| e.size).sum(),
        ..Default::default()
    };
    let mut buf = vec![0u8; 256 * 1024];
    let mut bad = Vec::new();
    for e in entries.iter().filter(|e| !e.is_dir) {
        if rep.cancelled() {
            return Err("Cancelled.".into());
        }
        progress.current = e.rel.clone();
        let mut zf = open_entry(&mut zip, e.zip_index, password)?;
        loop {
            match zf.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    progress.done_bytes += n as u64;
                    rep.progress(&progress, false);
                }
                Err(err) => {
                    bad.push(format!("{}: {err}", e.rel));
                    break;
                }
            }
        }
        progress.done_files += 1;
    }
    progress.phase = "Finished".into();
    rep.progress(&progress, true);
    if bad.is_empty() {
        Ok(format!("All {} files in this backup are intact.", progress.done_files))
    } else {
        Err(format!("{} damaged file(s), first: {}", bad.len(), bad[0]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn reporter() -> Reporter {
        let (tx, _rx) = channel();
        std::mem::forget(_rx);
        Reporter::new(tx, Arc::new(AtomicBool::new(false)), Box::new(|| {}))
    }

    fn round_trip(password: Option<&str>, tag: &str) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join(format!("test-{tag}"));
        let _ = fs::remove_dir_all(&root);
        let src = root.join("Data");
        fs::create_dir_all(src.join("sub/deeper")).unwrap();
        fs::create_dir_all(src.join("empty")).unwrap();
        fs::create_dir_all(src.join(".git")).unwrap();
        fs::write(src.join("a.txt"), "hello").unwrap();
        fs::write(src.join("skip.tmp"), "junk").unwrap();
        fs::write(src.join(".git/HEAD"), "ref").unwrap();
        fs::write(src.join("sub/deeper/b.bin"), vec![7u8; 300_000]).unwrap();
        let job = BackupJob {
            id: 42,
            name: "Test set".into(),
            sources: vec![src.clone()],
            destination: root.join("backups"),
            excludes: vec!["*.tmp".into(), ".git".into()],
            keep_last: 2,
            encrypt: password.is_some(),
            ..Default::default()
        };
        let msg = run_backup(&job, password, &mut reporter()).unwrap();
        assert!(msg.contains("2 files"), "{msg}");
        let list = list_archives(&job.destination);
        assert_eq!(list.len(), 1);
        let info = &list[0];
        assert_eq!(info.manifest.encrypted, password.is_some());
        let entries = read_entries(&info.path).unwrap();
        assert!(entries.iter().any(|e| e.rel == "empty" && e.is_dir));
        assert!(!entries.iter().any(|e| e.rel.contains(".git") || e.rel.ends_with(".tmp")));
        let ok = run_verify(info, password, &mut reporter()).unwrap();
        assert!(ok.contains("intact"), "{ok}");
        if password.is_some() {
            assert!(run_verify(info, Some("wrong"), &mut reporter()).is_err());
        }

        let out = root.join("restored");
        run_restore(info, &entries, Some(&out), Conflict::Overwrite, password, &mut reporter()).unwrap();
        assert_eq!(fs::read_to_string(out.join("Data/a.txt")).unwrap(), "hello");
        assert_eq!(fs::read(out.join("Data/sub/deeper/b.bin")).unwrap().len(), 300_000);
        assert!(out.join("Data/empty").is_dir());
        // Second restore with "keep both" makes renamed copies.
        run_restore(info, &entries, Some(&out), Conflict::KeepBoth, password, &mut reporter()).unwrap();
        assert!(out.join("Data/a (restored 1).txt").exists());
        // Restore to the original place after deleting a file.
        fs::remove_file(src.join("a.txt")).unwrap();
        let r = run_restore(info, &entries, None, Conflict::Skip, password, &mut reporter()).unwrap();
        assert!(r.contains("Restored 1 files"), "{r}");
        assert_eq!(fs::read_to_string(src.join("a.txt")).unwrap(), "hello");

        // Retention keeps only the newest two.
        for _ in 0..2 {
            std::thread::sleep(std::time::Duration::from_millis(1100));
            run_backup(&job, password, &mut reporter()).unwrap();
        }
        assert_eq!(list_archives(&job.destination).len(), 2);
    }

    #[test]
    fn plain_round_trip() {
        round_trip(None, "plain");
    }

    #[test]
    fn encrypted_round_trip() {
        round_trip(Some("s3cret pass"), "aes");
    }

    #[test]
    fn wildcards() {
        assert!(wildcard_match("*.tmp", "file.TMP"));
        assert!(wildcard_match("~$*", "~$doc.docx"));
        assert!(!wildcard_match("*.tmp", "file.txt"));
        assert!(wildcard_match("node_modules", "node_modules"));
    }
}
