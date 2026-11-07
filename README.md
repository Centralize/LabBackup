# LabBackup

A simple desktop app for backing up and restoring folders and files on
**Windows, macOS and Linux**.

- **Backup sets** save which folders to protect, where the copies go, and the options.
- Each backup is one **dated .zip file**, compressed if you want and
  optionally locked with an **AES-256 password**.
- **Restore** everything or just the files you pick, to their original places
  or to another folder, without ever overwriting by accident.
- **Verify** checks that a backup isn't damaged.
- **Keep the newest _N_** backups. Older ones are cleaned up for you.
- **Automatic backups** every few hours while the app is open.
- **History** of every backup, restore and check. Light and dark themes.

## Documentation

| Guide | For |
|-------|-----|
| [Installation](docs/INSTALL.md) | Building and installing on Windows, macOS and Linux, starting at login, settings location, uninstalling. |
| [User guide](docs/USER_GUIDE.md) | Step-by-step help: first backup, restoring files, passwords, automatic backups, troubleshooting. |

## Quick start

```sh
# 1. Install Rust from https://rustup.rs (Linux also needs a few packages, see docs/INSTALL.md)
# 2. In this folder:
cargo build --release
# 3. Run it:
#    Windows:      target\release\labbackup.exe
#    macOS/Linux:  ./target/release/labbackup
```

Then open the **My Documents** set it creates for you, choose **Where to save
backups** (ideally another drive), and click **Back up now**.

## For developers

- `src/engine.rs` does the backing up, listing, restoring and verifying, on a worker thread.
- `src/app.rs` is the window (eframe/egui). `src/theme.rs` holds the colours. `src/config.rs` holds the saved settings.
- `cargo test` runs round-trip backup → verify → restore tests, with and without a password.
- `labbackup --screenshot out.png [--tab backup|restore|history]` saves a
  picture of the window after about 120 frames and exits.
- `LABBACKUP_CONFIG=/path/config.json` uses a different settings file (for testing or portable use).
