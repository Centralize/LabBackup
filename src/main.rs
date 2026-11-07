#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod engine;
mod theme;

use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    let mut screenshot: Option<PathBuf> = None;
    let mut tab = String::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--screenshot" => screenshot = args.next().map(PathBuf::from),
            "--tab" => tab = args.next().unwrap_or_default(),
            "--help" | "-h" => {
                println!("LabBackup - back up and restore folders\n\nOptions:\n  --screenshot <file.png>  draw ~120 frames, save a picture of the window and exit\n  --tab <backup|restore|history>  screen to open first");
                return Ok(());
            }
            _ => {}
        }
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("LabBackup")
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([820.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "LabBackup",
        options,
        Box::new(move |cc| Ok(Box::new(app::LabBackupApp::new(cc, screenshot, &tab)))),
    )
}
