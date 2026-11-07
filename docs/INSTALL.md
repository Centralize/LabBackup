# Installing LabBackup

LabBackup is one program file with nothing else to install. It runs on
Windows 10/11, macOS 11 or newer, and modern Linux desktops (X11 or Wayland).

There is no ready-made installer yet, so you build the program once from its
source code. It takes about five minutes and the steps are the same on every
system.

---

## 1. Install Rust (one time only)

Rust is the toolkit LabBackup is written in.

| System  | What to do |
|---------|------------|
| Windows | Download and run **rustup-init.exe** from <https://rustup.rs>. If it asks to install the *Visual Studio C++ Build Tools*, say yes and accept the defaults. |
| macOS   | Open **Terminal** and run `xcode-select --install`, then `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Linux   | Open a terminal and run `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |

Close the terminal and open a new one afterwards. Then type `cargo --version`.
If you see a version number, Rust is ready.

### Extra packages on Linux

The window and the folder-picker dialogs need a few system libraries:

- **Debian / Ubuntu / Mint**
  ```sh
  sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev \
       libx11-dev libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev \
       xdg-desktop-portal xdg-desktop-portal-gtk
  ```
- **Fedora**
  ```sh
  sudo dnf install gcc pkgconf-pkg-config libxkbcommon-devel wayland-devel \
       libX11-devel libXcursor-devel libXrandr-devel libXi-devel mesa-libGL-devel \
       xdg-desktop-portal xdg-desktop-portal-gtk
  ```
- **Arch**
  ```sh
  sudo pacman -S base-devel libxkbcommon wayland libx11 libxcursor libxrandr \
       libxi mesa xdg-desktop-portal xdg-desktop-portal-gtk
  ```

On KDE you can use `xdg-desktop-portal-kde` in place of the `-gtk` one, and on
GNOME `xdg-desktop-portal-gnome`. If you have no portal, the **Browse…**
buttons won't open, but you can still type folder paths by hand.

---

## 2. Build LabBackup

Open a terminal in the `app` folder (the one with `Cargo.toml` in it) and run:

```sh
cargo build --release
```

The first build downloads its building blocks and takes a few minutes. Later
builds are quick. When it's done, the program is here:

| System  | Program file |
|---------|--------------|
| Windows | `target\release\labbackup.exe` |
| macOS   | `target/release/labbackup` |
| Linux   | `target/release/labbackup` |

You can copy that one file anywhere you like, for example to a USB stick. It
doesn't need the rest of the folder.

---

## 3. Put it somewhere handy

### Windows
1. Make a folder such as `C:\Program Files\LabBackup` and copy `labbackup.exe` into it.
2. Right-click `labbackup.exe` and choose **Show more options → Send to → Desktop (create shortcut)**.
3. To pin it, right-click the shortcut and choose **Pin to Start** or **Pin to taskbar**.

Windows SmartScreen may warn the first time, because the program isn't signed.
Click **More info → Run anyway**.

### macOS
1. Copy `labbackup` into a folder such as `~/Applications`.
2. Double-click it, or run it from Terminal with `~/Applications/labbackup`.
3. If macOS says it is from an *unidentified developer*, right-click it and
   choose **Open**, then **Open** again. You only need to do this once.
   (Or, in Terminal: `xattr -d com.apple.quarantine ~/Applications/labbackup`.)
4. The first time you back up your Documents, Desktop or Downloads, macOS asks
   whether LabBackup may use those folders. Click **Allow**. If you clicked
   *Don't Allow*, turn it on again in **System Settings → Privacy & Security →
   Files and Folders**, or give it **Full Disk Access**.

### Linux
1. Copy the program into your personal programs folder:
   ```sh
   mkdir -p ~/.local/bin && cp target/release/labbackup ~/.local/bin/
   ```
2. To add it to your app menu, make a file named
   `~/.local/share/applications/labbackup.desktop` with this text:
   ```ini
   [Desktop Entry]
   Type=Application
   Name=LabBackup
   Comment=Back up and restore your folders
   Exec=labbackup
   Terminal=false
   Categories=Utility;Archiving;
   ```
   It shows up in the menu after you log out and back in, or right away on most desktops.

---

## 4. Start it automatically (optional)

LabBackup only makes **automatic** backups while it's open. To keep it open,
start it when you log in:

- **Windows:** press `Win + R`, type `shell:startup`, press Enter, and put a shortcut to `labbackup.exe` in the folder that opens.
- **macOS:** **System Settings → General → Login Items → +**, then choose `labbackup`.
- **Linux:** copy the `labbackup.desktop` file above into `~/.config/autostart/`.

---

## Where LabBackup keeps its settings

Your backup sets and history are saved in one small settings file:

| System  | Location |
|---------|----------|
| Windows | `%APPDATA%\LabBackup\LabBackup\config\config.json` |
| macOS   | `~/Library/Application Support/org.LabBackup.LabBackup/config.json` |
| Linux   | `~/.config/labbackup/config.json` |

Passwords are **never** saved in this file.

**Portable use:** to keep the settings next to the program (for example on a
USB stick), set the `LABBACKUP_CONFIG` environment variable to the full path
of the settings file you want to use before starting LabBackup.

---

## Updating

Get the new source code, run `cargo build --release` again, and replace the old
program file with the new one. Your settings and backups stay as they are.

## Uninstalling

Delete the program file, its shortcut, and (if you want to) the settings file
listed above. Your backup files are never deleted by uninstalling. They stay
in the folders you chose.

---

## Command-line options

| Option | What it does |
|--------|--------------|
| `--tab backup`, `--tab restore`, `--tab history` | Opens that screen first. |
| `--screenshot picture.png` | Opens the window, saves a picture of it and closes. Used for automated checks. |
| `--help` | Lists these options. |

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `cargo: command not found` | Open a **new** terminal after installing Rust, or run `source ~/.cargo/env`. |
| Linux: build stops with an error about `xkbcommon`, `wayland` or `X11` | Install the Linux packages listed in step 1. |
| Linux: **Browse…** does nothing | Install `xdg-desktop-portal` plus a backend (see step 1), or type the path into the box. |
| The window is blank or won't open on an old computer or a virtual machine | Update the graphics driver. LabBackup needs OpenGL 2.0 or newer. |
| macOS: "can't be opened" | See step 3 for macOS (right-click → Open). |
