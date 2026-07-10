# KIT-ILIAS-downloader

Bulk download script for **ILIAS 9** (KIT ILIAS).

Download your KIT ILIAS course materials to your computer — slides, PDFs, documents, and more. **No programming knowledge is required.**

---

## What does it download?

Download content from ILIAS. That includes:

* **Files** — PDFs, PowerPoint slides, Word documents, ZIP archives, etc.
* **Exercise sheets and solutions**
* **Opencast lectures** (optional, large files)
* **Forum posts** (optional)

By default, the program downloads **all courses you are enrolled in** and saves them into folders:

```
ilias/
├── 2304300 – Electrocatalysis/
│   ├── Lecture/
│   │   └── Vorlesung_01.pdf
│   └── Exercise/
└── 2311616 – Communication Systems and Protocols/
    └── ...
```

---

## Installation

> **Important:** On GitHub, make sure you are on the **`main`** branch before downloading.
> Use the branch dropdown at the top-left of the repository page and select **`main`**.

### Option 1: Download a pre-built program (recommended)

Go to the **[Releases](../../releases)** page and download the file for your operating system:

| Your computer | File to download |
|---------------|------------------|
| Windows (64-bit) | `KIT-ILIAS-downloader-x86_64-pc-windows-msvc.zip` |
| macOS (Apple Silicon) | `KIT-ILIAS-downloader-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `KIT-ILIAS-downloader-x86_64-apple-darwin.tar.gz` |
| Linux (64-bit) | `KIT-ILIAS-downloader-x86_64-unknown-linux-gnu.tar.gz` |

**Extract the archive:**

- **Windows:** Right-click the `.zip` → **Extract All** → you get `KIT-ILIAS-downloader.exe`
- **macOS / Linux:** Extract the `.tar.gz` → you get `KIT-ILIAS-downloader`

> **macOS:** If the system blocks the app, go to **System Settings → Privacy & Security → Open Anyway**.

### Option 2: Build from source (optional)

Requires [Rust](https://www.rust-lang.org/tools/install).

```bash
git clone https://github.com/kagayachan/KIT-ILIAS-downloader.git
cd KIT-ILIAS-downloader
git checkout main
cargo build --release
```

The program will be at `target/release/KIT-ILIAS-downloader` (or `.exe` on Windows).

---

## Usage

Open a terminal, go to the folder that contains the program, and run:

**Windows:**
```
KIT-ILIAS-downloader.exe -o C:\Users\YourName\Downloads\ilias --no-videos
```

**macOS / Linux:**
```
./KIT-ILIAS-downloader -o ~/Downloads/ilias --no-videos
```

The program will ask for your KIT account in the terminal:

```
KIT account username: uabcd
KIT account password: (nothing appears while typing — this is normal)
```

When you see `Logged in!` and `Writing ...`, files are being saved to the folder you chose with `-o`.

> **Default behaviour:** Downloads **all your enrolled courses**. Use `--desktop` to download only dashboard favourites instead.

### Download one specific course

Use `--sync-url` with a link copied from **inside ILIAS** (right-click a course link → Copy link address — **not** the browser address bar):

```
./KIT-ILIAS-downloader -o ~/Downloads/ilias --sync-url "https://ilias.studium.kit.edu/goto.php/crs/1234567" --no-videos
```

---

## Common tasks

### Download only slides and documents (no videos)

Add `--no-videos` (shown in the examples above). This is the fastest option.

### Also download lecture videos

Remove `--no-videos` from the command:

```
./KIT-ILIAS-downloader -o ~/Downloads/ilias
```

Videos are large and take much longer.

### Download only your dashboard favourites (not all courses)

Add `--desktop`:

```
./KIT-ILIAS-downloader -o ~/Downloads/ilias --desktop --no-videos
```

### Get updated files (professor replaced a file)

The program **skips files that already exist** on your computer. To re-download everything, add `-f`:

```
./KIT-ILIAS-downloader -o ~/Downloads/ilias -f --no-videos
```

### Download only new files

Run the same command again **without** `-f`. Only newly added files will be downloaded.

---

## Options (quick reference)

| Option | What it does |
|--------|-------------|
| `-o <folder>` | **Required.** Where to save downloaded files. |
| `--no-videos` | Skip Opencast lecture videos. |
| `-f` | Re-download files even if they already exist. |
| `--desktop` | Download dashboard favourites only (not all courses). |
| `--sync-url <url>` | Download one specific ILIAS page and its contents. |
| `-t` / `--forum` | Also download forum posts. |
| `-v` / `-vv` | Show more details while running. |
| `--debug-html` | Save web pages for troubleshooting (`<output>/.debug/`). |
| `-U <username>` | Provide KIT username on the command line. |
| `-h` | Show all available options. |

---

## How updated files are handled

| Situation | What happens |
|-----------|-------------|
| New file with a **new name** | Downloaded on the next run. |
| Professor **replaces** a file (same name) | **Not** updated automatically — use `-f`. |
| File removed from ILIAS | Stays on your computer. |

---

## Troubleshooting

### "Logged in!" but no files downloaded

- Check that you are enrolled in courses on ILIAS.
- Run with `-vv` for details: `./KIT-ILIAS-downloader -o ~/Downloads/ilias -vv --no-videos`
- Add `--debug-html` and inspect the `.debug/` folder in your output directory.

### "no SAML response, incorrect password?"

Your KIT password was wrong. Run again and type it carefully (same password as the ILIAS website).

### Downloaded files have no file extension

Update to the latest release on the **`main`** branch, delete the broken files, and run again with `-f`.

---

## Advanced

### Filter courses with `.iliasignore`

Create `.iliasignore` in your output folder:

```ignore
# Only download one course:
/*/
!/My Course Name/
```

### Shorter folder names with `course_names.toml`

Create `course_names.toml` in your output folder:

```toml
"24030 – Programmierparadigmen" = "ProPa"
```

---

## Credits

Based on [KIT-ILIAS-downloader](https://github.com/FliegendeWurst/KIT-ILIAS-downloader) by FliegendeWurst.  
Updated for ILIAS 9 compatibility.

## License

GPL-3.0-or-later — see [LICENSE](LICENSE).
