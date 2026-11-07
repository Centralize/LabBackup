# LabBackup User Guide

LabBackup keeps safe copies of your folders and files and brings them back
when you need them. It works the same way on Windows, macOS and Linux.

Not installed yet? See [INSTALL.md](INSTALL.md).

---

## Contents

1. [The window at a glance](#1-the-window-at-a-glance)
2. [Your first backup in 3 steps](#2-your-first-backup-in-3-steps)
3. [Backup sets](#3-backup-sets)
4. [The Back up screen](#4-the-back-up-screen)
5. [Restoring files](#5-restoring-files)
6. [Checking a backup](#6-checking-a-backup)
7. [History](#7-history)
8. [Password-protected backups](#8-password-protected-backups)
9. [Automatic backups](#9-automatic-backups)
10. [What a backup file is](#10-what-a-backup-file-is)
11. [Good habits](#11-good-habits)
12. [Questions and problems](#12-questions-and-problems)

---

## 1. The window at a glance

```
┌──────────────────────────────────────────────────────────────────────┐
│ LabBackup                                   2 backup set(s)  [Light] │  ← top bar
├───────────────┬──────────────────────────────────────────────────────┤
│ BACKUP SETS   │ Lab results              Last backup 2 h ago [Back up now]
│ ┌───────────┐ │  Back up   Restore   History                         │  ← tabs
│ │Lab results│ │ ┌──────────────────────────────────────────────────┐ │
│ └───────────┘ │ │ What to back up / Where to save / Options         │ │
│ ┌───────────┐ │ └──────────────────────────────────────────────────┘ │
│ │Instruments│ │                                                      │
│ └───────────┘ │                                                      │
│[+ New set]    │                                                      │
├───────────────┴──────────────────────────────────────────────────────┤
│ Ready.  /  progress bar while working  [Cancel]                       │  ← status bar
└──────────────────────────────────────────────────────────────────────┘
```

- **Left side:** your *backup sets*. Click one to work with it.
- **Top right:** **Back up now** runs a backup of the selected set right away.
- **Tabs:** **Back up** (settings), **Restore** (get files back) and **History** (what has happened).
- **Bottom bar:** shows progress while something is running, with a **Cancel**
  button. Afterwards it shows the result in green (it worked) or red (something needs your attention).
- **Light / Dark** in the top-right corner switches the colours.

---

## 2. Your first backup in 3 steps

The first time you open LabBackup, it has already made a set called
**My Documents** for your Documents folder.

1. **Choose what to protect.** On the **Back up** tab, under *What to back up*,
   click **Add folder…** and choose a folder. Add as many folders as you like.
2. **Choose where the copies go.** Under *Where to save backups*, click
   **Browse…** and choose a folder on **another drive**, such as a USB stick,
   an external disk or a network share.
3. **Click _Back up now_** (top right). The bottom bar shows how far along it is.
   When it turns green, your backup is done.

That's it. Next time, just click **Back up now** again. Each backup is a
separate, dated copy, so older versions are kept too.

---

## 3. Backup sets

A **backup set** is a saved recipe: *these folders* go *there*, with *these
options*. You can have as many sets as you like, for example:

- "Lab results": the measurement folders, every 6 hours, to the network drive
- "Instrument configs": a few settings files, password protected, to a USB stick
- "My Documents": the Documents folder, kept for 30 versions

| To… | Do this |
|-----|---------|
| Make a new set | Click **+ New backup set** at the bottom left. |
| Rename a set | Click its name at the top of the screen and type a new one. |
| Switch between sets | Click the set on the left. |
| Delete a set | On the **Back up** tab, click **Delete set** (bottom right), then **Yes, delete set**. The set's backup **files are not deleted**. They stay on disk and can still be restored with **Open backup file…**. |

Each card on the left shows when the set was last backed up (in orange if
never), how many folders it holds, whether automatic backups are on, and a
lock if it's password protected.

Changes are saved by themselves. There's no Save button.

---

## 4. The Back up screen

### What to back up
- **Add folder…** adds one or more folders, with everything inside them.
- **Add files…** adds single files.
- **Or type a path:** type or paste a folder path and press **Enter** or **Add**.
  This is handy for network paths such as `\\server\share\data` (Windows) or
  `/mnt/lab` (Linux and macOS).
- **Remove** takes an item off the list. It never deletes anything from your disk.
- An item marked **not found** no longer exists (for example a USB drive that
  isn't plugged in). The backup stops with a message until you plug it in or remove it.

### Where to save backups
- Type a folder or use **Browse…**. The folder is created if it doesn't exist.
- **Open folder** shows the backup folder in Explorer, Finder or your file manager.
- If the backup folder is *inside* one of the folders being backed up, you'll
  see a warning. LabBackup skips it automatically so backups don't contain
  older backups, but a separate drive is much safer.

### Options

| Option | What it means |
|--------|---------------|
| **Skip files named** | Names of files or folders to leave out, separated by commas. `*` means "anything" and `?` means "any one letter". The default skips temporary files: `*.tmp, ~$*, .DS_Store, Thumbs.db`. Other examples: `*.log`, `node_modules`, `.git`, `cache`. Capital and small letters count as the same. |
| **Keep _N_ newest backups** | When a new backup finishes, older backups **of this set** beyond this number are deleted from the backup folder. `0` keeps every backup forever. |
| **Compress files** | Makes backups smaller. Turn it off for folders full of photos, videos or zip files, which don't shrink: backups get faster and stay about the same size. |
| **Back up every _N_ h** | Automatic backups. See [section 9](#9-automatic-backups). |
| **Protect with a password** | Locks the backup with AES-256. See [section 8](#8-password-protected-backups). |

### While a backup runs
- The bottom bar shows the files and amount done, the time so far, and the file being copied.
- **Cancel** stops it. A cancelled backup leaves nothing behind (no half-finished file).
- You can keep using the app, but only one backup, restore or check runs at a time.
- Files that can't be read (for example a file another program has locked)
  are skipped. The result says how many were skipped, and the backup itself
  lists them.

---

## 5. Restoring files

Open the **Restore** tab. It works in three numbered steps.

### Step 1: Choose a backup
The list shows every backup found in this set's backup folder, newest first,
with its date, file count, size and a lock if it's password protected. The
newest one is selected for you. Click another date to pick a different backup.

- **Refresh** reads the folder again (for example after plugging in a USB drive).
- **Open backup file…** opens a backup from anywhere: another drive, another
  computer, or a set you deleted.
- Backups from *other* sets saved in the same folder are listed too, marked
  "(other set)".

### Step 2: Pick files to restore
- Every file in the backup is ticked to begin with.
- Type in **Search files** to find files by name or folder. **Select all** and
  **Select none** apply only to the files you can see in the search results. For example: search
  `report`, click **Select none**, then clear the search, and every file except
  the reports stays ticked.
- The line above the list shows how many files are ticked and their total size.

### Step 3: Restore

| Choice | Meaning |
|--------|---------|
| **In their original locations** | Puts files back exactly where they were backed up from. |
| **Into another folder** | Puts them in the folder you choose, inside a subfolder named after each backed-up folder. This is the safest way to look at old versions without touching your current files. |
| **If a file exists → Keep both** *(default)* | The restored copy is saved as `name (restored 1).ext` next to the existing file. Nothing is overwritten. |
| **If a file exists → Skip it** | Existing files are left alone. Only missing files come back. Good for "put back what I deleted". |
| **If a file exists → Replace it** | Existing files are overwritten with the backed-up version. |

Click **Restore _N_ files**. Restored files keep their original "last changed" dates.

> **Tip: getting back one deleted file.** Pick the newest backup, search for
> the file, click **Select none**, tick just that file, choose *original
> locations* + *Skip it*, and click **Restore 1 files**.

**Delete backup** (bottom right) removes the selected backup file from disk
for good, after you confirm.

---

## 6. Checking a backup

On the **Restore** tab, click **Verify backup**. LabBackup reads every file in
the selected backup and checks it against the fingerprint saved when it was
made. You'll see either *"All N files in this backup are intact"* or a list of
damaged files. Nothing is written to your disk.

Do this now and then, especially for backups on USB sticks and old disks.

---

## 7. History

The **History** tab lists every backup, restore and check (the last 500),
newest first, with the time, the set, and the result: a green tick or a red
warning with the reason. Rows for the selected set are shown in bold.
**Clear history** empties the list. It doesn't touch any backups.

---

## 8. Password-protected backups

1. Tick **Protect backups with a password (AES-256)** on the **Back up** tab.
2. Type the password twice. Tick **Show** to see what you typed.
3. Back up as usual. The set shows a lock.

Things to know:

- **The password is never saved.** You type it again each time you open
  LabBackup, before backing up. That way nobody can read it from your computer.
- **If you forget the password, the backup can't be opened.** Not by
  LabBackup, and not by anyone else. Write it down somewhere safe.
- To restore, type the password in the **Password** box in step 3 of the
  Restore tab. A wrong password gives *"The password is wrong."*
- The list of backups, the dates, the folder names and the file names stay
  readable without the password. **The file contents are locked.**

---

## 9. Automatic backups

Tick **Back up every _N_ h** and choose how many hours (1 to 720). While
LabBackup is open, it checks about every half minute and starts a backup of any
set whose last backup is older than that. A set that has never been backed up
runs right away.

- Automatic backups **only run while LabBackup is open**. To keep it running,
  have it start when you log in. See *Start it automatically* in
  [INSTALL.md](INSTALL.md#4-start-it-automatically-optional).
- For password-protected sets, automatic backups wait until you've typed the
  password in this session.
- If the computer was off or LabBackup was closed, the missed backup runs
  shortly after you open it again.

---

## 10. What a backup file is

Each backup is one ordinary **.zip** file, named after the set and the time it was made:

```
Lab-results_2026-11-07_18-10-00.zip
```

Inside it:

- `data/0/…`, `data/1/…` are your folders, one numbered folder per item in *What to back up*.
- `labbackup-manifest.json` is a small description: which set, when, which
  computer, the original paths, the number of files, and any files that were skipped.

Because it's a normal zip file, you can open it **without LabBackup** in an
emergency: double-click it in Windows, macOS or most Linux file managers. For
password-protected backups you need a tool that understands AES zip
encryption, such as **7-Zip** (Windows), **Keka** (macOS) or **p7zip** (Linux).

While a backup is being written, it's called `….zip.partial`. If you ever find
one left over after a crash or power cut, it's incomplete and you can delete it.

---

## 11. Good habits

- **Keep backups on a different drive** from the originals. A backup on the
  same disk is lost along with the disk.
- **Have two places**, for example a USB disk at your desk and a network share or a
  disk kept somewhere else. Make two sets with the same folders and different destinations.
- **Try a restore now and then** using *Into another folder*, so you know it works before you need it.
- **Verify** backups on removable drives every few months.
- **Plug in the backup drive before you click _Back up now_.** If the drive is
  missing, LabBackup may make the folder on your main disk.

---

## 12. Questions and problems

| Question or problem | Answer |
|---------------------|--------|
| *"Add at least one folder to back up."* | The set has nothing under *What to back up*. |
| *"Can't open … "* when backing up | A folder in the list is missing or you don't have permission. Plug the drive in, or remove that item. |
| *"N item(s) couldn't be read and were skipped."* | Some files were in use or protected. Close the program using them and back up again. |
| *"This zip file wasn't made by LabBackup."* | **Open backup file…** only opens LabBackup backups. Other zip files open with your normal zip tool. |
| *"This backup is password protected…"* / *"The password is wrong."* | Type the backup's password in the Restore tab. Capital and small letters matter. |
| The Restore list is empty | Check that *Where to save backups* points at the right folder and that the drive is plugged in, then click **Refresh**. Or use **Open backup file…**. |
| A backup is very slow | Big folders of photos or videos: turn off **Compress files**. On network drives, speed depends on the network. |
| I deleted a set by mistake | Make a new set with the same destination. Its old backups appear in the Restore list as "(other set)" and can still be restored. |
| macOS: Documents/Desktop backups are empty or skipped | Give LabBackup permission in **System Settings → Privacy & Security → Files and Folders** (or Full Disk Access). |
| Linux: **Browse…** doesn't open | Install `xdg-desktop-portal` (see INSTALL.md) or type the path. |
| Where are my settings? | See *Where LabBackup keeps its settings* in [INSTALL.md](INSTALL.md#where-labbackup-keeps-its-settings). |

### What LabBackup doesn't do (yet)
- Back up while it's closed. It has no background service.
- Back up straight to cloud storage. You can save to a folder that a cloud app
  syncs (OneDrive, Dropbox, Google Drive, iCloud Drive) instead.
- Copy shortcuts and symbolic links. They're skipped. The files they point to
  are backed up only if they're inside a chosen folder.
