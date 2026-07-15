# KIT-ILIAS-downloader

Bulk download script for **ILIAS 9** (KIT ILIAS).

Download content from ILIAS. That includes:

* files
* exercise sheets and solutions
* Opencast lectures
* forum posts

> Make sure you use the **`main`** branch of this repository.

## Installation

All steps below are done in a **terminal** (Terminal on macOS, PowerShell or Command Prompt on Windows).

### Option A: Clone and build 

**macOS / Linux:**

```bash
git clone -b main https://github.com/kagayachan/KIT-ILIAS-downloader.git
cd KIT-ILIAS-downloader
cargo build --release
```

The program is at `./target/release/KIT-ILIAS-downloader`.

**Windows (PowerShell):**

```powershell
git clone -b main https://github.com/kagayachan/KIT-ILIAS-downloader.git
cd KIT-ILIAS-downloader
cargo build --release
```

The program is at `.\target\release\KIT-ILIAS-downloader.exe`.

If you do not have Rust yet, install it from https://www.rust-lang.org/tools/install first.

### Option B: Download a release binary from the terminal

Go to [releases](../../releases), and download directly the executable file for your operating system.:


## Usage

Open a terminal. Navigate to the directory that contains the binary (or use the path from `cargo build`).

### batch download (all courses)

This downloads **all courses you are enrolled in** into one folder. You will be asked for your KIT username and password in the terminal.

**macOS / Linux** (after `git clone` + `cargo build`):

```bash
cd KIT-ILIAS-downloader
./target/release/KIT-ILIAS-downloader -o ~/Downloads/ilias --no-videos
```

**macOS / Linux** (after downloading a release tarball):

```bash
./KIT-ILIAS-downloader -o ~/Downloads/ilias --no-videos
```

**Windows (PowerShell)** (after `git clone` + `cargo build`):

```powershell
cd KIT-ILIAS-downloader
.\target\release\KIT-ILIAS-downloader.exe -o $env:USERPROFILE\Downloads\ilias --no-videos
```

**Windows (PowerShell)** (after downloading a release zip):

```powershell
.\KIT-ILIAS-downloader.exe -o $env:USERPROFILE\Downloads\ilias --no-videos
```

Example output:

```
KIT account username: uabcd
KIT account password:
Logging into ILIAS using KIT account..
Logging into Shibboleth..
Logging into ILIAS..
Logged in!
Writing 2311616 – Communication Systems and Protocols (SS 2026)/CSP_SS2026_Session 01_General Information.pdf
...
done
```

### Download only dashboard favourites

```bash
./target/release/KIT-ILIAS-downloader -o ~/Downloads/ilias --desktop --no-videos
```

### Download a specific course or folder

Use `--sync-url` with a link copied from an ILIAS page (right-click a link inside ILIAS → copy link address, **not** the browser address bar):

```bash
./target/release/KIT-ILIAS-downloader -o ~/Downloads/ilias/ProPa \
  --sync-url 'https://ilias.studium.kit.edu/goto.php/crs/2914319' \
  --no-videos
```

### Options

```
KIT-ILIAS-downloader 0.3.9

USAGE:
    KIT-ILIAS-downloader [FLAGS] [OPTIONS] --output <output>

FLAGS:
        --all                 Download all courses (default when --sync-url is not set)
        --check-videos        Re-check OpenCast lectures (slow)
        --combine-videos      Combine videos if there is more than one stream (requires ffmpeg)
        --content-tree        Use content tree (experimental)
        --debug-html          Save fetched HTML to <output>/.debug/ for troubleshooting
        --desktop             Download only dashboard favourites instead of all courses
    -f                        Re-download already present files
    -t, --forum               Download forum content
    -h, --help                Prints help information
        --keep-session        Attempt to re-use session cookies
        --keyring             Use the system keyring
    -n, --no-videos           Do not download Opencast videos，which can make task faster
        --save-ilias-pages    Save overview pages of ILIAS courses and folders
    -s, --skip-files          Do not download files
    -V, --version             Prints version information
    -v                        Verbose logging

OPTIONS:
    -j, --jobs <jobs>              Parallel download jobs [default: 1]
    -o, --output <output>          Output directory
        --pass-path <pass-path>    Path inside `pass(1)` to the password for your KIT account
    -P, --password <password>      KIT account password
    -p, --proxy <proxy>            Proxy, e.g. socks5h://127.0.0.1:1080
        --rate <rate>              Requests per minute [default: 8]
        --sync-url <sync-url>      ILIAS page to download
    -U, --username <username>      KIT account username
```

### .iliasignore

`.gitignore` syntax can be used in a `.iliasignore` file (located in the output directory):

```ignore
# example 1: only download a single course
/*/
!/InsertCourseHere/
# example 2: only download files related to one tutorial
/Course/Tutorien/*/
!/Course/Tutorien/Tut* 3/
```

### Credentials

By default, the program asks for your KIT username and password in the terminal when it starts.

You can also pass them on the command line:

```bash
./target/release/KIT-ILIAS-downloader -U uabcd -P 'your-password' -o ~/Downloads/ilias --no-videos
```

With `--keyring`, the password can be read from the system password store:

```bash
./target/release/KIT-ILIAS-downloader -U uabcd --keyring -o ~/Downloads/ilias --no-videos
```

If you use [pass](https://www.passwordstore.org/), use `--pass-path`:

```bash
./target/release/KIT-ILIAS-downloader -U uabcd --pass-path edu/kit/uskyk -o ~/Downloads/ilias --no-videos
```

When running the downloader multiple times in a short period of time, you may want to use the `--keep-session` flag.
If specified, the downloader will save and restore session cookies (`.iliassession` file in the output directory).

## Mobile (iPad / Android): userscript

The CLI cannot run on phones or tablets. For iPad and Android there is a **userscript** in
[`userscript/kit-ilias-downloader.user.js`](userscript/kit-ilias-downloader.user.js) that runs inside your browser,
reuses your existing ILIAS login session (no password handling needed) and downloads files as a **single ZIP**
(directory structure preserved).

### Installation

| Platform | Browser | Userscript manager |
|---|---|---|
| Android | Firefox | [Tampermonkey](https://addons.mozilla.org/firefox/addon/tampermonkey/) or [Violentmonkey](https://addons.mozilla.org/firefox/addon/violentmonkey/) |
| iPad / iPhone | Safari | [Userscripts](https://apps.apple.com/app/userscripts/id1463298887) (App Store) |
| Desktop | any | Tampermonkey / Violentmonkey |

1. Install the userscript manager for your platform (table above).
2. Open the raw file `userscript/kit-ilias-downloader.user.js` from this repository — the manager will offer to install it.
   (With the iOS Userscripts app, save the file into the app's scripts folder instead.)
3. Log into [ILIAS](https://ilias.studium.kit.edu/) in the same browser.

### Usage

1. Navigate to the ILIAS page you want to download (a course, a folder, or the dashboard).
2. Tap the green **"ILIAS ↓"** button in the bottom-right corner.
3. Choose the scope:
   * **Current page (recursive)** – everything below the course/folder you are looking at
   * **Dashboard favourites** – same as the CLI's `--desktop`
   * **All courses** – same as the CLI's default (membership overview)
4. Tap **Download as ZIP**. Progress (pages crawled, files fetched, total size) is shown live;
   when finished, the browser saves one `.zip` file (on iOS it lands in *Files*, on Android in *Downloads*).

### Limitations

* The whole ZIP is assembled **in memory** — fine for lecture notes and exercise sheets, but
  Opencast videos are **skipped by default**. The "Include Opencast videos" switch exists, but expect
  high memory usage on mobile devices; prefer the CLI for full video mirrors.
* Forums, wikis, surveys and interactive presentations are counted as "skipped" (use the CLI for those).
* Keep the browser tab in the foreground while it runs — mobile browsers pause background tabs.
* Downloads are rate-limited (default 20 requests/minute, like the CLI's `--rate`) to avoid stressing ILIAS.
* If ILIAS logs you out mid-run, the script stops with a "session expired" error — reload, log in and retry.

### Manual test checklist (after changing the script)

Since crawling requires a real KIT account, test manually in a logged-in browser, in this order:

1. a small folder ("Current page" scope) — check ZIP contents and file extensions,
2. a whole course — check nested folder structure and session/`expand` handling,
3. "Dashboard favourites" and "All courses" scopes,
4. optional: one course with "Include Opencast videos" enabled (desktop browser recommended).

Offline unit tests for the parsing logic (no account needed):

```bash
cd userscript/test
npm install
npm test            # parsing/selector tests against saved HTML fixtures
node zip-smoke.js   # verifies ZIP generation with the bundled fflate
```




## Other useful programs

- https://github.com/Garmelon/PFERD
- https://github.com/DeOldSax/iliasDownloaderTool
- https://github.com/brantsch/kit-ilias-fuse
- https://github.com/Mr-Pine/IliasUploaderUtility (unlike the other tools, this one uploads files)
- https://github.com/I-Al-Istannen/ilias-tests (unlike the other tools, this one processes "tests")

