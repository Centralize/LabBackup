//! The window: backup sets on the left, and Back up / Restore / History
//! screens for the selected set on the right.

use crate::config::{BackupJob, Config, HistoryKind};
use crate::engine::{self, human_bytes, ArchiveEntry, ArchiveInfo, Conflict, Progress, Reporter, WorkerMsg};
use crate::theme::{self, ACCENT, ERR, OK, WARN};
use chrono::Local;
use eframe::egui::{self, Color32, RichText};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Backup,
    Restore,
    History,
}

struct RunningTask {
    rx: Receiver<WorkerMsg>,
    cancel: Arc<AtomicBool>,
    kind: HistoryKind,
    job_id: Option<u64>,
    job_name: String,
    started: Instant,
}

struct Screenshot {
    path: PathBuf,
    requested: bool,
    done: Arc<AtomicBool>,
}

#[derive(Default)]
struct RestoreState {
    /// Which folder the archive list was read from.
    listed_dir: Option<PathBuf>,
    archives: Vec<ArchiveInfo>,
    /// The chosen backup (from the list or opened from anywhere).
    chosen: Option<ArchiveInfo>,
    entries: Vec<ArchiveEntry>,
    checked: Vec<bool>,
    filter: String,
    error: Option<String>,
    to_other: bool,
    other_dir: String,
    conflict: Option<Conflict>,
    password: String,
    confirm_delete: bool,
}

pub struct LabBackupApp {
    cfg: Config,
    selected: usize,
    tab: Tab,
    new_source: String,
    excludes_buf: String,
    excludes_for: Option<u64>,
    passwords: HashMap<u64, (String, String)>,
    show_password: bool,
    task: Option<RunningTask>,
    progress: Progress,
    status: Option<(bool, String)>,
    restore: RestoreState,
    confirm_delete_job: bool,
    dirty: bool,
    last_save: Instant,
    last_auto_check: Instant,
    screenshot: Option<Screenshot>,
    frames: u64,
}

impl LabBackupApp {
    pub fn new(cc: &eframe::CreationContext<'_>, screenshot: Option<PathBuf>, tab: &str) -> Self {
        let cfg = Config::load();
        theme::apply(&cc.egui_ctx, cfg.dark_mode);
        let tab = match tab {
            "restore" => Tab::Restore,
            "history" => Tab::History,
            _ => Tab::Backup,
        };
        Self {
            cfg,
            selected: 0,
            tab,
            new_source: String::new(),
            excludes_buf: String::new(),
            excludes_for: None,
            passwords: HashMap::new(),
            show_password: false,
            task: None,
            progress: Progress::default(),
            status: None,
            restore: RestoreState::default(),
            confirm_delete_job: false,
            dirty: false,
            last_save: Instant::now(),
            last_auto_check: Instant::now(),
            screenshot: screenshot.map(|path| Screenshot { path, requested: false, done: Arc::new(AtomicBool::new(false)) }),
            frames: 0,
        }
    }

    fn job(&self) -> Option<&BackupJob> {
        self.cfg.jobs.get(self.selected)
    }

    fn busy(&self) -> bool {
        self.task.is_some()
    }

    fn session_password(&self, job: &BackupJob) -> Result<Option<String>, String> {
        if !job.encrypt {
            return Ok(None);
        }
        let (a, b) = self.passwords.get(&job.id).cloned().unwrap_or_default();
        if a.is_empty() {
            return Err("Enter a password for this backup set, or turn off password protection.".into());
        }
        if a != b {
            return Err("The two passwords don't match.".into());
        }
        Ok(Some(a))
    }

    fn spawn<F>(&mut self, ctx: &egui::Context, kind: HistoryKind, job_id: Option<u64>, job_name: String, work: F)
    where
        F: FnOnce(&mut Reporter) -> Result<String, String> + Send + 'static,
    {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let ctx2 = ctx.clone();
        let mut rep = Reporter::new(tx, cancel.clone(), Box::new(move || ctx2.request_repaint()));
        std::thread::spawn(move || {
            let result = work(&mut rep);
            rep.done(result);
        });
        self.progress = Progress { phase: "Starting".into(), ..Default::default() };
        self.status = None;
        self.task = Some(RunningTask { rx, cancel, kind, job_id, job_name, started: Instant::now() });
    }

    fn start_backup(&mut self, ctx: &egui::Context, idx: usize) {
        if self.busy() {
            return;
        }
        let Some(job) = self.cfg.jobs.get(idx).cloned() else { return };
        let password = match self.session_password(&job) {
            Ok(p) => p,
            Err(e) => {
                self.status = Some((false, e));
                return;
            }
        };
        let name = job.name.clone();
        self.spawn(ctx, HistoryKind::Backup, Some(job.id), name, move |rep| {
            engine::run_backup(&job, password.as_deref(), rep)
        });
    }

    fn poll_task(&mut self) {
        let Some(task) = &self.task else { return };
        let mut finished = None;
        while let Ok(msg) = task.rx.try_recv() {
            match msg {
                WorkerMsg::Progress(p) => self.progress = p,
                WorkerMsg::Done(r) => finished = Some(r),
            }
        }
        if let Some(result) = finished {
            let task = self.task.take().unwrap();
            let ok = result.is_ok();
            let msg = match result {
                Ok(m) | Err(m) => m,
            };
            self.cfg.log(task.kind, &task.job_name, ok, msg.clone());
            if task.kind == HistoryKind::Backup {
                if let Some(job) = self.cfg.jobs.iter_mut().find(|j| Some(j.id) == task.job_id) {
                    if ok {
                        job.last_run = Some(Local::now());
                    }
                }
                self.restore.listed_dir = None; // refresh the list of backups
            }
            self.status = Some((ok, msg));
            self.dirty = true;
        }
    }

    fn auto_backups(&mut self, ctx: &egui::Context) {
        if self.screenshot.is_some() || self.busy() || self.last_auto_check.elapsed() < Duration::from_secs(20) {
            return;
        }
        self.last_auto_check = Instant::now();
        let now = Local::now();
        let due = self.cfg.jobs.iter().position(|j| {
            j.auto_enabled
                && !j.sources.is_empty()
                && (!j.encrypt || self.session_password(j).is_ok())
                && j.last_run.map(|t| (now - t).num_minutes() >= j.auto_every_hours.max(1) as i64 * 60).unwrap_or(true)
        });
        if let Some(i) = due {
            self.start_backup(ctx, i);
        }
    }

    fn save_if_needed(&mut self, force: bool) {
        if self.screenshot.is_some() {
            return;
        }
        if self.dirty && (force || self.last_save.elapsed() > Duration::from_secs(1)) {
            if let Err(e) = self.cfg.save() {
                self.status = Some((false, format!("Couldn't save settings: {e}")));
            }
            self.dirty = false;
            self.last_save = Instant::now();
        }
    }

    // -----------------------------------------------------------------------
    // Layout pieces

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let pal = theme::palette(self.cfg.dark_mode);
        egui::Panel::top("top")
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::symmetric(16, 10)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), egui::Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(rect, 8.0, theme::ACCENT_DARK);
                    let c = rect.center();
                    p.circle_stroke(c, 8.0, egui::Stroke::new(2.5, Color32::WHITE));
                    p.line_segment([c, c + egui::vec2(0.0, -5.5)], egui::Stroke::new(2.5, Color32::WHITE));
                    p.line_segment([c, c + egui::vec2(4.0, 2.0)], egui::Stroke::new(2.5, Color32::WHITE));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(RichText::new("LabBackup").strong().size(17.0));
                        ui.label(RichText::new("Back up and restore your folders").size(12.0).color(pal.muted));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if self.cfg.dark_mode { "☀ Light" } else { "🌙 Dark" };
                        if ui.button(label).clicked() {
                            self.cfg.dark_mode = !self.cfg.dark_mode;
                            theme::apply(ui.ctx(), self.cfg.dark_mode);
                            self.dirty = true;
                        }
                        let total: usize = self.cfg.jobs.len();
                        ui.label(RichText::new(format!("{total} backup set(s)")).color(pal.muted));
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let pal = theme::palette(self.cfg.dark_mode);
        egui::Panel::bottom("status")
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::symmetric(16, 8)))
            .show(ui, |ui| {
                if let Some(task) = &self.task {
                    let p = &self.progress;
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label(RichText::new(format!("{} — {}", p.phase, task.job_name)).strong());
                        ui.label(
                            RichText::new(format!(
                                "{} of {} files · {} of {} · {}s",
                                p.done_files,
                                p.total_files,
                                human_bytes(p.done_bytes),
                                human_bytes(p.total_bytes),
                                task.started.elapsed().as_secs()
                            ))
                            .color(pal.muted),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Cancel").clicked() {
                                task.cancel.store(true, Ordering::Relaxed);
                            }
                        });
                    });
                    ui.add(egui::ProgressBar::new(p.fraction()).show_percentage().fill(ACCENT_BAR));
                    ui.add(egui::Label::new(RichText::new(&p.current).size(12.0).color(pal.muted)).truncate());
                } else if let Some((ok, msg)) = &self.status {
                    ui.horizontal(|ui| {
                        let (icon, color) = if *ok { ("✔", OK) } else { ("⚠", ERR) };
                        ui.label(RichText::new(icon).color(color).strong());
                        ui.add(egui::Label::new(RichText::new(msg).color(color)).wrap());
                    });
                } else {
                    ui.label(RichText::new("Ready.").color(pal.muted));
                }
            });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let pal = theme::palette(self.cfg.dark_mode);
        let dark = self.cfg.dark_mode;
        egui::Panel::left("sets")
            .resizable(false)
            .exact_size(250.0)
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(egui::Margin::same(12)))
            .show(ui, |ui| {
                ui.label(RichText::new("BACKUP SETS").size(12.0).strong().color(pal.muted));
                ui.add_space(2.0);
                let mut select = None;
                egui::ScrollArea::vertical().id_salt("sets_scroll").max_height(ui.available_height() - 44.0).show(ui, |ui| {
                    for (i, job) in self.cfg.jobs.iter().enumerate() {
                        let selected = i == self.selected;
                        let fill = if selected { if dark { Color32::from_rgb(38, 56, 78) } else { Color32::from_rgb(214, 230, 248) } } else { pal.card };
                        let stroke = if selected { egui::Stroke::new(1.5, ACCENT) } else { egui::Stroke::new(1.0, pal.border) };
                        let resp = egui::Frame::new()
                            .fill(fill)
                            .stroke(stroke)
                            .corner_radius(8)
                            .inner_margin(egui::Margin::symmetric(12, 9))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("📁").size(16.0));
                                    ui.add(egui::Label::new(RichText::new(&job.name).strong()).truncate());
                                    if job.encrypt {
                                        ui.label(RichText::new("🔒").size(12.0));
                                    }
                                });
                                let last = match job.last_run {
                                    Some(t) => format!("Last backup {}", relative_time(t)),
                                    None => "Never backed up".to_string(),
                                };
                                let col = if job.last_run.is_some() { pal.muted } else { WARN };
                                ui.label(RichText::new(last).size(12.0).color(col));
                                let detail = format!(
                                    "{} source(s){}",
                                    job.sources.len(),
                                    if job.auto_enabled { format!(" · auto every {}h", job.auto_every_hours) } else { String::new() }
                                );
                                ui.label(RichText::new(detail).size(12.0).color(pal.muted));
                            })
                            .response
                            .interact(egui::Sense::click());
                        if resp.clicked() {
                            select = Some(i);
                        }
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                    }
                });
                if let Some(i) = select {
                    self.select_job(i);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    let btn = egui::Button::new(RichText::new("+  New backup set").strong()).min_size(egui::vec2(ui.available_width(), 32.0));
                    if ui.add(btn).clicked() {
                        let job = BackupJob { id: self.cfg.next_id, name: format!("Backup set {}", self.cfg.next_id), ..Default::default() };
                        self.cfg.next_id += 1;
                        self.cfg.jobs.push(job);
                        let last = self.cfg.jobs.len() - 1;
                        self.select_job(last);
                        self.tab = Tab::Backup;
                        self.dirty = true;
                    }
                });
            });
    }

    fn select_job(&mut self, i: usize) {
        if i != self.selected {
            self.selected = i;
            self.restore = RestoreState::default();
            self.confirm_delete_job = false;
        }
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        let pal = theme::palette(self.cfg.dark_mode);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg).inner_margin(egui::Margin::symmetric(20, 14)))
            .show(ui, |ui| {
                if self.cfg.jobs.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(120.0);
                        ui.heading("No backup sets yet");
                        ui.label("Click “New backup set” on the left to choose folders to protect.");
                    });
                    return;
                }
                self.selected = self.selected.min(self.cfg.jobs.len() - 1);
                // Header with the set's name, the main action and the tabs.
                let busy = self.busy();
                let mut start = false;
                ui.horizontal(|ui| {
                    let job = &mut self.cfg.jobs[self.selected];
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut job.name)
                            .font(egui::FontId::proportional(20.0))
                            .desired_width(320.0)
                            .frame(egui::Frame::NONE),
                    );
                    if resp.changed() {
                        self.dirty = true;
                    }
                    resp.on_hover_text("Click to rename this backup set");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ready = !job.sources.is_empty() && !busy;
                        if ui.add_enabled(ready, theme::primary_button("💾  Back up now")).clicked() {
                            start = true;
                        }
                        let info = match job.last_run {
                            Some(t) => format!("Last backup {}", relative_time(t)),
                            None => "Not backed up yet".into(),
                        };
                        ui.label(RichText::new(info).color(pal.muted));
                    });
                });
                if start {
                    self.start_backup(ui.ctx(), self.selected);
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for (tab, label) in [(Tab::Backup, "💾  Back up"), (Tab::Restore, "⟲  Restore"), (Tab::History, "🕘  History")] {
                        let sel = self.tab == tab;
                        let text = RichText::new(label).size(15.0);
                        let text = if sel { text.strong().color(ACCENT) } else { text.color(pal.muted) };
                        let resp = ui.add(egui::Button::new(text).frame(false).min_size(egui::vec2(110.0, 30.0)));
                        if sel {
                            let r = resp.rect;
                            ui.painter().line_segment(
                                [r.left_bottom() + egui::vec2(8.0, 0.0), r.right_bottom() - egui::vec2(8.0, 0.0)],
                                egui::Stroke::new(2.5, ACCENT),
                            );
                        }
                        if resp.clicked() {
                            self.tab = tab;
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().id_salt("main_scroll").auto_shrink([false, false]).show(ui, |ui| {
                    match self.tab {
                        Tab::Backup => self.backup_tab(ui),
                        Tab::Restore => self.restore_tab(ui),
                        Tab::History => self.history_tab(ui),
                    }
                });
            });
    }

    // -----------------------------------------------------------------------
    // Back up screen

    fn backup_tab(&mut self, ui: &mut egui::Ui) {
        let dark = self.cfg.dark_mode;
        let pal = theme::palette(dark);
        let busy = self.busy();
        let idx = self.selected;
        let job_id = self.cfg.jobs[idx].id;
        if self.excludes_for != Some(job_id) {
            self.excludes_buf = self.cfg.jobs[idx].excludes.join(", ");
            self.excludes_for = Some(job_id);
        }
        let mut changed = false;

        // What to back up
        theme::card(dark).show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_title(ui, "What to back up", "Folders and files that go into each backup.");
            let job = &mut self.cfg.jobs[idx];
            let mut remove = None;
            if job.sources.is_empty() {
                ui.label(RichText::new("Nothing chosen yet — add a folder below.").color(WARN));
            }
            for (i, src) in job.sources.iter().enumerate() {
                ui.horizontal(|ui| {
                    let exists = src.exists();
                    let icon = if src.is_file() { "📄" } else { "📁" };
                    ui.label(icon);
                    ui.add(egui::Label::new(RichText::new(src.display().to_string()).monospace()).truncate());
                    if !exists {
                        ui.label(RichText::new("not found").color(ERR).size(12.0));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✖ Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                });
            }
            if let Some(i) = remove {
                job.sources.remove(i);
                changed = true;
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button("📁  Add folder…").clicked() {
                    if let Some(dirs) = rfd::FileDialog::new().set_title("Choose folders to back up").pick_folders() {
                        for d in dirs {
                            if !job.sources.contains(&d) {
                                job.sources.push(d);
                            }
                        }
                        changed = true;
                    }
                }
                if ui.button("📄  Add files…").clicked() {
                    if let Some(files) = rfd::FileDialog::new().set_title("Choose files to back up").pick_files() {
                        for f in files {
                            if !job.sources.contains(&f) {
                                job.sources.push(f);
                            }
                        }
                        changed = true;
                    }
                }
                ui.label(RichText::new("or type a path:").color(pal.muted));
                let resp = ui.add(egui::TextEdit::singleline(&mut self.new_source).hint_text("/path/to/folder").desired_width(220.0));
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.button("Add").clicked() || enter) && !self.new_source.trim().is_empty() {
                    let p = PathBuf::from(self.new_source.trim());
                    if !job.sources.contains(&p) {
                        job.sources.push(p);
                    }
                    self.new_source.clear();
                    changed = true;
                }
            });
        });
        ui.add_space(6.0);

        // Where to save
        theme::card(dark).show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_title(ui, "Where to save backups", "A folder on another drive, a USB stick or a network share is safest.");
            let job = &mut self.cfg.jobs[idx];
            ui.horizontal(|ui| {
                let mut text = job.destination.display().to_string();
                if ui.add(egui::TextEdit::singleline(&mut text).desired_width((ui.available_width() - 210.0).max(200.0))).changed() {
                    job.destination = PathBuf::from(text);
                    changed = true;
                }
                if ui.button("Browse…").clicked() {
                    if let Some(d) = rfd::FileDialog::new().set_title("Where should backups be saved?").pick_folder() {
                        job.destination = d;
                        changed = true;
                    }
                }
                if ui.button("Open folder").clicked() {
                    let _ = std::fs::create_dir_all(&job.destination);
                    open_in_file_manager(&job.destination);
                }
            });
            let same = job.sources.iter().any(|s| job.destination.starts_with(s));
            if same {
                ui.label(RichText::new("The backup folder is inside a folder being backed up. It will be skipped automatically, but another drive is safer.").color(WARN).size(12.0));
            }
        });
        ui.add_space(6.0);

        // Options
        theme::card(dark).show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_title(ui, "Options", "");
            let job = &mut self.cfg.jobs[idx];
            egui::Grid::new("opts").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
                ui.label("Skip files named");
                ui.vertical(|ui| {
                    if ui.add(egui::TextEdit::singleline(&mut self.excludes_buf).desired_width(380.0)).changed() {
                        job.excludes = self.excludes_buf.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                        changed = true;
                    }
                    ui.label(RichText::new("Separate with commas. * matches anything, e.g. *.tmp, node_modules, .git").size(12.0).color(pal.muted));
                });
                ui.end_row();

                ui.label("Keep");
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut job.keep_last).range(0..=999).speed(0.2)).changed();
                    ui.label(if job.keep_last == 0 { "backups (0 = keep all)".to_string() } else { "newest backups, older ones are deleted".to_string() });
                });
                ui.end_row();

                ui.label("Compression");
                changed |= ui.checkbox(&mut job.compress, "Compress files to save space").changed();
                ui.end_row();

                ui.label("Automatic");
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(&mut job.auto_enabled, "Back up every").changed();
                    changed |= ui.add_enabled(job.auto_enabled, egui::DragValue::new(&mut job.auto_every_hours).range(1..=720).suffix(" h")).changed();
                    ui.label(RichText::new("while LabBackup is open").color(pal.muted));
                });
                ui.end_row();

                ui.label("Password");
                ui.vertical(|ui| {
                    changed |= ui.checkbox(&mut job.encrypt, "Protect backups with a password (AES-256)").changed();
                    if job.encrypt {
                        let entry = self.passwords.entry(job.id).or_default();
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut entry.0).password(!self.show_password).hint_text("Password").desired_width(170.0));
                            ui.add(egui::TextEdit::singleline(&mut entry.1).password(!self.show_password).hint_text("Repeat password").desired_width(170.0));
                            ui.checkbox(&mut self.show_password, "Show");
                        });
                        ui.label(RichText::new("The password is never saved. Without it the backup can't be restored.").size(12.0).color(WARN));
                    }
                });
                ui.end_row();
            });
        });
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            let job = &self.cfg.jobs[idx];
            let info = match job.last_run {
                Some(t) => format!("Last backup: {}", t.format("%Y-%m-%d %H:%M")),
                None => "This set hasn't been backed up yet.".into(),
            };
            ui.label(RichText::new(info).color(pal.muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.confirm_delete_job {
                    if ui.button(RichText::new("Yes, delete set").color(ERR)).clicked() {
                        self.cfg.jobs.remove(idx);
                        self.selected = self.selected.saturating_sub(1);
                        self.confirm_delete_job = false;
                        self.restore = RestoreState::default();
                        changed = true;
                    }
                    if ui.button("Keep it").clicked() {
                        self.confirm_delete_job = false;
                    }
                    ui.label(RichText::new("Delete this set? Backup files stay on disk.").color(pal.muted));
                } else if ui.add_enabled(!busy, egui::Button::new("🗑  Delete set")).clicked() {
                    self.confirm_delete_job = true;
                }
            });
        });

        if changed {
            self.dirty = true;
            self.restore.listed_dir = None;
        }
    }

    // -----------------------------------------------------------------------
    // Restore screen

    fn refresh_archives(&mut self) {
        let Some(job) = self.job() else { return };
        let dest = job.destination.clone();
        let job_id = job.id;
        if self.restore.listed_dir.as_ref() == Some(&dest) {
            return;
        }
        let mut list = engine::list_archives(&dest);
        // Show this set's backups first, then any others found in the same folder.
        list.sort_by_key(|a| (a.manifest.job_id != job_id, std::cmp::Reverse(a.manifest.created)));
        self.restore.archives = list;
        self.restore.listed_dir = Some(dest);
        if let Some(chosen) = &self.restore.chosen {
            if !chosen.path.exists() {
                self.restore.chosen = None;
                self.restore.entries.clear();
            }
        }
        // Start with the newest backup of this set selected.
        if self.restore.chosen.is_none() {
            if let Some(first) = self.restore.archives.iter().find(|a| a.manifest.job_id == job_id).cloned() {
                self.choose_archive(first);
            }
        }
    }

    fn choose_archive(&mut self, info: ArchiveInfo) {
        self.restore.error = None;
        self.restore.confirm_delete = false;
        match engine::read_entries(&info.path) {
            Ok(entries) => {
                self.restore.checked = vec![true; entries.len()];
                self.restore.entries = entries;
            }
            Err(e) => {
                self.restore.entries.clear();
                self.restore.checked.clear();
                self.restore.error = Some(e);
            }
        }
        self.restore.chosen = Some(info);
    }

    fn restore_tab(&mut self, ui: &mut egui::Ui) {
        let dark = self.cfg.dark_mode;
        let pal = theme::palette(dark);
        self.refresh_archives();
        let job_id = self.job().map(|j| j.id).unwrap_or(0);
        let dest = self.job().map(|j| j.destination.clone()).unwrap_or_default();
        let mut pick = None;

        theme::card(dark).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                section_title(ui, "1. Choose a backup", &format!("Found in {}", dest.display()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.button("Open backup file…").clicked() {
                        if let Some(f) = rfd::FileDialog::new().add_filter("LabBackup archive", &["zip"]).pick_file() {
                            match engine::read_manifest(&f) {
                                Ok(info) => pick = Some(info),
                                Err(e) => self.restore.error = Some(e),
                            }
                        }
                    }
                    if ui.button("⟳ Refresh").clicked() {
                        self.restore.listed_dir = None;
                    }
                });
            });
            if self.restore.archives.is_empty() {
                ui.label(RichText::new("No backups found in this folder yet. Make one on the Back up tab, or open a backup file from somewhere else.").color(pal.muted));
            } else {
                egui::ScrollArea::vertical().id_salt("archives").max_height(190.0).show(ui, |ui| {
                    egui::Grid::new("archive_grid").num_columns(5).striped(true).spacing([18.0, 6.0]).show(ui, |ui| {
                        for h in ["Date", "Backup set", "Files", "Size", ""] {
                            ui.label(RichText::new(h).strong().size(12.0).color(pal.muted));
                        }
                        ui.end_row();
                        for a in &self.restore.archives {
                            let is_sel = self.restore.chosen.as_ref().map(|c| c.path == a.path).unwrap_or(false);
                            let m = &a.manifest;
                            if ui.selectable_label(is_sel, format!("{}  ({})", m.created.format("%Y-%m-%d %H:%M:%S"), relative_time(m.created))).clicked() {
                                pick = Some(a.clone());
                            }
                            let name = if m.job_id == job_id { RichText::new(&m.job_name) } else { RichText::new(format!("{} (other set)", m.job_name)).color(pal.muted) };
                            ui.label(name);
                            ui.label(m.file_count.to_string());
                            ui.label(human_bytes(a.size_on_disk));
                            ui.label(if m.encrypted { "🔒" } else { "" });
                            ui.end_row();
                        }
                    });
                });
            }
        });
        if let Some(info) = pick {
            self.choose_archive(info);
        }
        if let Some(e) = &self.restore.error {
            ui.label(RichText::new(e).color(ERR));
        }
        let Some(info) = self.restore.chosen.clone() else { return };
        ui.add_space(6.0);

        // Files inside the chosen backup
        theme::card(dark).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let m = &info.manifest;
            section_title(
                ui,
                "2. Pick files to restore",
                &format!("Backup of “{}” made {} on {}", m.job_name, m.created.format("%Y-%m-%d %H:%M"), m.host),
            );
            ui.horizontal_wrapped(|ui| {
                for s in &m.sources {
                    ui.label(RichText::new(format!("{} {}", if s.is_file { "📄" } else { "📁" }, s.path.display())).size(12.0).color(pal.muted));
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.restore.filter).hint_text("🔍 Search files").desired_width(260.0));
                let needle = self.restore.filter.to_lowercase();
                if ui.button("Select all").clicked() {
                    for (i, e) in self.restore.entries.iter().enumerate() {
                        if needle.is_empty() || e.rel.to_lowercase().contains(&needle) {
                            self.restore.checked[i] = true;
                        }
                    }
                }
                if ui.button("Select none").clicked() {
                    for (i, e) in self.restore.entries.iter().enumerate() {
                        if needle.is_empty() || e.rel.to_lowercase().contains(&needle) {
                            self.restore.checked[i] = false;
                        }
                    }
                }
                let (n, bytes) = self.selected_totals();
                ui.label(RichText::new(format!("{n} files selected · {}", human_bytes(bytes))).color(pal.muted));
            });

            let needle = self.restore.filter.to_lowercase();
            let visible: Vec<usize> = self
                .restore
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.is_dir && (needle.is_empty() || e.rel.to_lowercase().contains(&needle)))
                .map(|(i, _)| i)
                .collect();
            let row_h = 22.0;
            egui::Frame::new().fill(ui.visuals().extreme_bg_color).corner_radius(6).inner_margin(6).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if visible.is_empty() {
                    ui.label(RichText::new("No files match.").color(pal.muted));
                }
                egui::ScrollArea::vertical().id_salt("entries").max_height(240.0).auto_shrink([false, true]).show_rows(ui, row_h, visible.len(), |ui, range| {
                    for &i in &visible[range] {
                        let e = &self.restore.entries[i];
                        let folder = m.sources.iter().find(|s| s.index == e.source).map(|s| s.folder_name.as_str()).unwrap_or("?");
                        ui.horizontal(|ui| {
                            ui.set_height(row_h - 4.0);
                            ui.checkbox(&mut self.restore.checked[i], "");
                            ui.add(egui::Label::new(RichText::new(format!("{folder}/{}", e.rel)).monospace()).truncate());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(e.modified.clone().unwrap_or_default()).size(12.0).color(pal.muted));
                                ui.add_sized([70.0, 16.0], egui::Label::new(RichText::new(human_bytes(e.size)).size(12.0).color(pal.muted)));
                            });
                        });
                    }
                });
            });
        });
        ui.add_space(6.0);

        // Where and how to restore
        let mut go = false;
        let mut verify = false;
        let mut delete = false;
        theme::card(dark).show(ui, |ui| {
            ui.set_width(ui.available_width());
            section_title(ui, "3. Restore", "");
            egui::Grid::new("restore_opts").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
                ui.label("Put files back");
                ui.vertical(|ui| {
                    ui.radio_value(&mut self.restore.to_other, false, "In their original locations");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut self.restore.to_other, true, "Into another folder:");
                        ui.add_enabled(self.restore.to_other, egui::TextEdit::singleline(&mut self.restore.other_dir).desired_width(300.0).hint_text("Choose a folder"));
                        if ui.add_enabled(self.restore.to_other, egui::Button::new("Browse…")).clicked() {
                            if let Some(d) = rfd::FileDialog::new().set_title("Restore into…").pick_folder() {
                                self.restore.other_dir = d.display().to_string();
                            }
                        }
                    });
                });
                ui.end_row();

                ui.label("If a file exists");
                let c = self.restore.conflict.get_or_insert(Conflict::KeepBoth);
                ui.horizontal(|ui| {
                    ui.radio_value(c, Conflict::KeepBoth, "Keep both");
                    ui.radio_value(c, Conflict::Skip, "Skip it");
                    ui.radio_value(c, Conflict::Overwrite, "Replace it");
                });
                ui.end_row();

                if info.manifest.encrypted {
                    ui.label("Password");
                    ui.add(egui::TextEdit::singleline(&mut self.restore.password).password(true).hint_text("Backup password").desired_width(220.0));
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let (n, _) = self.selected_totals();
                let ok = n > 0 && !self.busy() && (!self.restore.to_other || !self.restore.other_dir.trim().is_empty());
                if ui.add_enabled(ok, theme::primary_button(&format!("⟲  Restore {n} files"))).clicked() {
                    go = true;
                }
                if ui.add_enabled(!self.busy(), egui::Button::new("✔  Verify backup")).on_hover_text("Read every file and check it isn't damaged").clicked() {
                    verify = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.restore.confirm_delete {
                        if ui.button(RichText::new("Yes, delete it").color(ERR)).clicked() {
                            delete = true;
                        }
                        if ui.button("No").clicked() {
                            self.restore.confirm_delete = false;
                        }
                        ui.label(RichText::new("Delete this backup file for good?").color(pal.muted));
                    } else if ui.add_enabled(!self.busy(), egui::Button::new("🗑  Delete backup")).clicked() {
                        self.restore.confirm_delete = true;
                    }
                });
            });
        });

        let password = if info.manifest.encrypted { Some(self.restore.password.clone()) } else { None };
        if go {
            if password.as_deref() == Some("") {
                self.status = Some((false, "This backup is password protected. Enter its password.".into()));
                return;
            }
            let selected: Vec<ArchiveEntry> = self
                .restore
                .entries
                .iter()
                .zip(&self.restore.checked)
                .filter(|(e, c)| **c && !e.is_dir)
                .map(|(e, _)| e.clone())
                .collect();
            let into = if self.restore.to_other { Some(PathBuf::from(self.restore.other_dir.trim())) } else { None };
            let conflict = self.restore.conflict.unwrap_or(Conflict::KeepBoth);
            let name = info.manifest.job_name.clone();
            let info2 = info.clone();
            self.spawn(ui.ctx(), HistoryKind::Restore, None, name, move |rep| {
                engine::run_restore(&info2, &selected, into.as_deref(), conflict, password.as_deref(), rep)
            });
        } else if verify {
            let name = info.manifest.job_name.clone();
            let info2 = info.clone();
            self.spawn(ui.ctx(), HistoryKind::Verify, None, name, move |rep| engine::run_verify(&info2, password.as_deref(), rep));
        } else if delete {
            match std::fs::remove_file(&info.path) {
                Ok(()) => {
                    self.status = Some((true, format!("Deleted {}", info.path.file_name().unwrap_or_default().to_string_lossy())));
                    self.restore.chosen = None;
                    self.restore.entries.clear();
                    self.restore.listed_dir = None;
                }
                Err(e) => self.status = Some((false, format!("Couldn't delete: {e}"))),
            }
            self.restore.confirm_delete = false;
        }
    }

    fn selected_totals(&self) -> (usize, u64) {
        self.restore
            .entries
            .iter()
            .zip(&self.restore.checked)
            .filter(|(e, c)| **c && !e.is_dir)
            .fold((0, 0), |(n, b), (e, _)| (n + 1, b + e.size))
    }

    // -----------------------------------------------------------------------
    // History screen

    fn history_tab(&mut self, ui: &mut egui::Ui) {
        let dark = self.cfg.dark_mode;
        let pal = theme::palette(dark);
        let name = self.job().map(|j| j.name.clone()).unwrap_or_default();
        let mut clear = false;
        theme::card(dark).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                section_title(ui, "History", "Every backup, restore and check, newest first.");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.button("Clear history").clicked() {
                        clear = true;
                    }
                });
            });
            if self.cfg.history.is_empty() {
                ui.label(RichText::new("Nothing has happened yet.").color(pal.muted));
                return;
            }
            egui::Grid::new("history").num_columns(4).striped(true).spacing([16.0, 8.0]).show(ui, |ui| {
                for h in ["When", "What", "Backup set", "Result"] {
                    ui.label(RichText::new(h).strong().size(12.0).color(pal.muted));
                }
                ui.end_row();
                for h in self.cfg.history.iter().take(300) {
                    let mine = h.job == name;
                    ui.label(h.time.format("%Y-%m-%d %H:%M").to_string());
                    ui.label(h.kind.label());
                    ui.label(if mine { RichText::new(&h.job).strong() } else { RichText::new(&h.job) });
                    ui.horizontal(|ui| {
                        let (icon, col) = if h.ok { ("✔", OK) } else { ("⚠", ERR) };
                        ui.label(RichText::new(icon).color(col));
                        ui.add(egui::Label::new(&h.message).wrap());
                    });
                    ui.end_row();
                }
            });
        });
        if clear {
            self.cfg.history.clear();
            self.dirty = true;
        }
    }

    fn handle_screenshot(&mut self, ui: &mut egui::Ui) {
        let Some(shot) = &mut self.screenshot else { return };
        ui.ctx().request_repaint();
        if shot.done.load(Ordering::Relaxed) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.frames >= 120 && !shot.requested {
            shot.requested = true;
            let path = shot.path.clone();
            let done = shot.done.clone();
            ui.ctx().request_screenshot(move |img| {
                if let Err(e) = save_png(&path, &img) {
                    eprintln!("Couldn't save screenshot: {e}");
                }
                done.store(true, Ordering::Relaxed);
            });
        }
        if self.frames > 900 {
            eprintln!("Screenshot timed out");
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

const ACCENT_BAR: Color32 = theme::ACCENT_DARK;

impl eframe::App for LabBackupApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frames += 1;
        self.poll_task();
        self.auto_backups(ui.ctx());

        self.top_bar(ui);
        self.status_bar(ui);
        self.sidebar(ui);
        self.central(ui);

        self.save_if_needed(false);
        if self.busy() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        } else {
            ui.ctx().request_repaint_after(Duration::from_secs(30));
        }
        self.handle_screenshot(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(t) = &self.task {
            t.cancel.store(true, Ordering::Relaxed);
        }
        self.dirty = true;
        self.save_if_needed(true);
    }
}

// ---------------------------------------------------------------------------

fn section_title(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.label(RichText::new(title).strong().size(16.0));
        if !subtitle.is_empty() {
            ui.label(RichText::new(subtitle).size(12.0).weak());
        }
    });
    ui.add_space(4.0);
}

fn relative_time(t: chrono::DateTime<Local>) -> String {
    let mins = (Local::now() - t).num_minutes();
    match mins {
        m if m < 1 => "just now".into(),
        m if m < 60 => format!("{m} min ago"),
        m if m < 60 * 24 => format!("{} h ago", m / 60),
        m => format!("{} days ago", m / (60 * 24)),
    }
}

fn open_in_file_manager(path: &Path) {
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = "xdg-open";
    let _ = std::process::Command::new(cmd).arg(path).spawn();
}

fn save_png(path: &Path, img: &egui::ColorImage) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
    }
    let [w, h] = img.size;
    let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| [c.r(), c.g(), c.b(), c.a()]).collect();
    image::save_buffer(path, &bytes, w as u32, h as u32, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())
}
