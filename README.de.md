# KIT-ILIAS-downloader

**Language / Sprache:** [English](README.md) | [Deutsch](README.de.md)

Bulk-Download-Skript für **ILIAS 9** (KIT ILIAS).

Inhalte aus ILIAS herunterladen. Dazu gehören:

* Dateien
* Übungsblätter und Lösungen
* Opencast-Vorlesungen
* Forenbeiträge

> Stelle sicher, dass du den Branch **`main`** dieses Repositories verwendest.

## Installation

Alle folgenden Schritte erfolgen in einem **Terminal** (Terminal unter macOS, PowerShell oder Eingabeaufforderung unter Windows).

### Option A: Klonen und bauen

**macOS / Linux:**

```bash
git clone -b main https://github.com/kagayachan/KIT-ILIAS-downloader.git
cd KIT-ILIAS-downloader
cargo build --release
```

Das Programm befindet sich unter `./target/release/KIT-ILIAS-downloader`.

**Windows (PowerShell):**

```powershell
git clone -b main https://github.com/kagayachan/KIT-ILIAS-downloader.git
cd KIT-ILIAS-downloader
cargo build --release
```

Das Programm befindet sich unter `.\target\release\KIT-ILIAS-downloader.exe`.

Falls Rust noch nicht installiert ist, installiere es zuerst unter https://www.rust-lang.org/tools/install.

### Option B: Release-Binary über das Terminal herunterladen

Öffne die [Releases](../../releases) und lade die ausführbare Datei für dein Betriebssystem herunter.


## Verwendung

Öffne ein Terminal. Wechsle in das Verzeichnis mit der Binary (oder verwende den Pfad aus `cargo build`).

### Batch-Download (alle Kurse)

Lädt **alle Kurse, in denen du eingeschrieben bist**, in einen Ordner herunter. Im Terminal wirst du nach KIT-Benutzername und Passwort gefragt.

**macOS / Linux** (nach `git clone` + `cargo build`):

```bash
cd KIT-ILIAS-downloader
./target/release/KIT-ILIAS-downloader -o ~/Downloads/ilias --no-videos
```

**macOS / Linux** (nach dem Herunterladen eines Release-Tarballs):

```bash
./KIT-ILIAS-downloader -o ~/Downloads/ilias --no-videos
```

**Windows (PowerShell)** (nach `git clone` + `cargo build`):

```powershell
cd KIT-ILIAS-downloader
.\target\release\KIT-ILIAS-downloader.exe -o $env:USERPROFILE\Downloads\ilias --no-videos
```

**Windows (PowerShell)** (nach dem Herunterladen eines Release-ZIPs):

```powershell
.\KIT-ILIAS-downloader.exe -o $env:USERPROFILE\Downloads\ilias --no-videos
```

Beispielausgabe:

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

### Nur Dashboard-Favoriten herunterladen

```bash
./target/release/KIT-ILIAS-downloader -o ~/Downloads/ilias --desktop --no-videos
```

### Einen bestimmten Kurs oder Ordner herunterladen

Verwende `--sync-url` mit einem Link von einer ILIAS-Seite (Rechtsklick auf einen Link in ILIAS → Link-Adresse kopieren, **nicht** die Adresszeile des Browsers):

```bash
./target/release/KIT-ILIAS-downloader -o ~/Downloads/ilias/ProPa \
  --sync-url 'https://ilias.studium.kit.edu/goto.php/crs/2914319' \
  --no-videos
```

### Optionen

```
KIT-ILIAS-downloader 0.3.9

USAGE:
    KIT-ILIAS-downloader [FLAGS] [OPTIONS] --output <output>

FLAGS:
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

Die `.gitignore`-Syntax kann in einer `.iliasignore`-Datei (im Ausgabeverzeichnis) verwendet werden:

```ignore
# Beispiel 1: nur einen einzelnen Kurs herunterladen
/*/
!/InsertCourseHere/
# Beispiel 2: nur Dateien zu einem Tutorium herunterladen
/Course/Tutorien/*/
!/Course/Tutorien/Tut* 3/
```

### Zugangsdaten

Standardmäßig fragt das Programm beim Start im Terminal nach KIT-Benutzername und Passwort.

Du kannst sie auch auf der Kommandozeile übergeben:

```bash
./target/release/KIT-ILIAS-downloader -U uabcd -P 'your-password' -o ~/Downloads/ilias --no-videos
```

Mit `--keyring` kann das Passwort aus dem System-Passwortspeicher gelesen werden:

```bash
./target/release/KIT-ILIAS-downloader -U uabcd --keyring -o ~/Downloads/ilias --no-videos
```

Wenn du [pass](https://www.passwordstore.org/) verwendest, nutze `--pass-path`:

```bash
./target/release/KIT-ILIAS-downloader -U uabcd --pass-path edu/kit/uskyk -o ~/Downloads/ilias --no-videos
```

Wenn du den Downloader kurz hintereinander mehrfach startest, kann die Flag `--keep-session` hilfreich sein.
Falls angegeben, speichert und stellt der Downloader Session-Cookies wieder her (Datei `.iliassession` im Ausgabeverzeichnis).




## Weitere nützliche Programme

- https://github.com/Garmelon/PFERD
- https://github.com/DeOldSax/iliasDownloaderTool
- https://github.com/brantsch/kit-ilias-fuse
- https://github.com/Mr-Pine/IliasUploaderUtility (im Gegensatz zu den anderen Tools lädt dieses Dateien hoch)
- https://github.com/I-Al-Istannen/ilias-tests (im Gegensatz zu den anderen Tools verarbeitet dieses „Tests“)
