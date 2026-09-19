# rstpd

A lightweight, native Windows text editor: a new Rust application, not a
Notepad++ port. No plugins, extension discovery, embedded browser, command
runner, script host, or automatic updater.

The application, document/session management, compare workflow, JSON tools,
search, encoding conversion, and UI integration are written in Rust.
**Scintilla and Lexilla are statically linked C++ editor components**, with a
small C++ adapter for reference-counted document release.
This is not a pure-Rust implementation of the editing engine. No Notepad++
application code or plugin code is included.

## Run

Download the portable Windows ZIP from the
[GitHub releases](https://github.com/Sockolet/rstpd/releases).
For a source checkout, use the build instructions below to create `dist`.

Open `dist\rstpd-0.2.4\rstpd.exe`, or extract the portable ZIP and open
`rstpd-0.2.4\rstpd.exe`. No installation or administrator access is needed.
Windows 10/11, x64. Keep the redistribution notices with the executable.

```powershell
.\dist\rstpd-0.2.4\rstpd.exe
.\dist\rstpd-0.2.4\rstpd.exe .\example.rs .\example.json
```

Use `--session-dir "C:\path\to\session"` for a separate workspace. Only one
instance may use a session directory at a time. Opening another instance
does not forward filenames to the existing instance.

Close an older rstpd instance before opening this release with the same session.
Version 0.2 reads version-1 sessions and saves version-2 sessions, including
language/completion definitions. Version 0.1 deliberately refuses version-2
sessions rather than silently dropping the new settings.

## Features

| Area | Implementation |
|---|---|
| Native UI | Windows title bar, menus, dialogs and controls; DPI-aware Segoe UI chrome, light/dark/system themes, closeable tabs and a compact Lucide icon toolbar |
| Highlighting | All 94 source-language inventory entries mapped, plus the full pinned Lexilla catalog; filename detection, mode selection, and data-only UDL 2.0/2.1 import |
| Markdown | CommonMark/GFM parsing with visible headings, emphasis, links, lists, quotes, tasks, tables, inline/fenced/indented code and strikethrough |
| Completion | Keywords, local declarations, member/context suggestions, common built-in APIs, parameter hints and importable completion signatures |
| Multiple edits | Alt+drag rectangular selection, Ctrl+click multiple carets, multi-selection typing and paste |
| Split screen | Two editable views of the same document or two different documents |
| Document map | Clickable compact view with the visible text range highlighted; scroll the map for long documents |
| Search | Normal, extended and advanced regex, including look-ahead/look-behind and pattern backreferences; case/whole-word options, wrap-around and replacement |
| Compare | Debounced background comparison, changed-line and inline character highlighting, linked scrolling and difference navigation |
| JSON | Lossless JSON/JSON5 pretty-print/minify, automatically refreshed tree, RFC 6901 pointers and source-span navigation |
| Text operations | Upper/lower/title/sentence/inverted case; case-sensitive, case-insensitive, natural and exact decimal sorting; reverse/join, deduplication and whitespace operations |
| Encoding | UTF-8, UTF-16/32 LE/BE, Windows/ISO/OEM code pages, Shift-JIS, EUC-JP, ISO-2022-JP, GBK/GB18030, Big5, EUC-KR, KOI8 and Mac encodings |
| Line endings | CRLF, LF and CR conversion |
| Recovery | Background atomic snapshots of all tabs, including unnamed documents; restore after restart or crash |

To choose documents for a split, click a pane and then choose its tab.
The icon toolbar keeps the same New, Open, Save, Find, Split, Compare, JSON tree
and Document map actions. Hover for a descriptive tooltip and shortcut.
Button names remain available to accessibility tools, and keyboard shortcuts
and menus are unchanged. Icons are drawn natively at the current DPI and
recolored for light/dark themes; no icon font or browser runtime is required.
The right-hand document's tab is marked `[R]`. Drag the divider to resize.
**Compare** compares the active tab with the next tab (wrapping at the end).
Editing either document automatically schedules a comparison refresh.
Old worker results are discarded if the documents changed in the meantime;
refresh does not move your editing caret. EOL representation is ignored during
line comparison.

Line operations affect complete selected lines, or the whole document when
there is no selection. Case conversion operates on selected text, including
rectangular/multiple selections. These edits participate in undo.
Numeric sorting compares decimal digits without floating-point rounding, so
large integer values remain correctly ordered. Every selected line must contain
a decimal number; invalid input is reported before any edits are applied.

The JSON tree refreshes after a short typing pause and preserves expanded paths.
Incomplete or invalid JSON temporarily pauses navigation instead of pointing
at stale positions. JSON5 formatting keeps comments, quoted/unquoted keys,
trailing commas, hex values and original number spellings; it does not convert
JSON5 into strict JSON. Minification retains comments and required line breaks.

Markdown uses a Rust parser rather than the limited legacy Markdown lexer.
Headings and strong text use bold, emphasis uses italics, links are underlined,
code has a contrasting foreground/background, and strikethrough is drawn over
the source text. Backtick and tilde fences are recognized, including unfinished
fences during editing. Styling refreshes in the background and works in split
views and the document map. This is source highlighting, not a Markdown preview;
embedded language-specific token coloring inside code fences is not provided.

Other lexers use semantic style names, metadata and bundled font attributes.
Types, functions, properties, tags, strings and comments are no longer flattened
into the same generic identifier color, including styles used by embedded markup.

## Language definitions and completion

The Language menu starts with **Plain text**, followed by fixed **A-C, D-F,
G-I, J-L, M-O, P-R, S-U and V-Z** groups. Languages are alphabetized within
each group, with additional columns for longer lists. Imported definitions
have their own **User-defined** group; import/removal commands are at the bottom.

`assets\language-coverage.tsv` records the 94 source-language entries and their
extensions from the upstream inventory with model date July 14, 2026, checked
September 19, 2026. The internal `searchResult` pane format is not a source-file
language. Shared/ambiguous extensions can still be selected explicitly in
**Language**; matching the inventory is not a guarantee of identical token
colors for every dialect.

**Language > Import user-defined language XML** accepts UDL 2.0/2.1 data.
Keyword groups, case/prefix rules, comments, delimiter alternatives/escapes,
nesting, operators, number rules, folding, colors and installed-font settings
are processed by a Rust highlighter. XML entities/DTDs are rejected. Definitions
never load libraries, run scripts, or access resources named inside the XML.
Use **Remove current user-defined language** to remove one without changing text.

Completion uses keywords, nearby code and declaration/signature analysis within
a 64 KiB context window. It recognizes common string/list/map types, local
class methods and common APIs. Type inference is heuristic, not a project-wide
compiler or language server. Common APIs are built-in; other library signatures
can be imported with **Language > Import completion API for current language**
using `AutoComplete / KeyWord / Overload / Param` XML data.
Both kinds of definitions persist in the session.

```powershell
.\dist\rstpd-0.2.4\rstpd.exe --import-language .\language.xml .\example.rstlang
.\dist\rstpd-0.2.4\rstpd.exe --completion-api .\functions.xml .\example.rs
```

The completion API import is associated with the active file's language.

## Keyboard

| Shortcut | Action |
|---|---|
| Ctrl+N / Ctrl+O / Ctrl+S | New / open / save |
| Ctrl+Shift+S / Ctrl+W | Save as / close tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Ctrl+F or Ctrl+H | Find and replace |
| F3 / Shift+F3 | Next / previous match |
| Ctrl+D / Ctrl+Shift+L | Select next / all occurrences |
| Ctrl+U / Ctrl+Shift+U | Lowercase / uppercase selections |
| Ctrl+Space / Ctrl+Shift+Space | Completion suggestions / function parameter hint |
| Ctrl+Alt+Right / F6 | Toggle split / focus other pane |
| F7 / Shift+F7 | Next / previous difference |
| Ctrl+Alt+J / Ctrl+Alt+T | Format JSON / toggle JSON tree |
| Ctrl+mouse wheel | Editor zoom |
| Escape | Close search bar |

Extended search/replacement recognizes `\n`, `\r`, `\t`, `\\`, `\0`,
`\xHH` and `\uHHHH`. Invalid escapes are reported, not silently interpreted.
Regex uses the Rust `fancy-regex` engine, with multiline/CRLF-aware anchors and Unicode
word boundaries. Replacement captures use `$1` or `${name}`; `$$` is a
literal dollar. Look-around and pattern backreferences are supported, for
example `(?<=prefix:)(\w+)\s+\1(?!x)`. This is not complete Boost/PCRE dialect
compatibility. Backtracking is limited to 500,000 steps, bulk operations check
a two-second processing budget, and oversized expansions fail before replacing
the document. Regex errors are reported, not treated as "no match".

## Recovery and data safety

Recovery is stored as **plaintext** in
`%LOCALAPPDATA%\rstpd\session.json`, under the user's normal directory
permissions. This may include sensitive unsaved text. Nothing is uploaded.
Existing installations with recovery state in `%LOCALAPPDATA%\RSTPad` keep
using that legacy folder, including its lock file, unless recovery state
already exists in the new `rstpd` folder. Nothing is moved or deleted during
the rename. `--session-dir` still explicitly selects a separate session.
Snapshots run approximately every three seconds after changes; a crash can
lose edits since the last completed snapshot. Normal exit waits for the final
snapshot and refuses to exit if that save fails.

Closing the app preserves every tab without asking for filenames.
Closing an individual dirty tab offers Save / Discard / Cancel; **Discard
removes that tab's recovery copy**. Autosave never writes the original files.
Explicit Save uses a flushed temporary file and Windows atomic replacement.
An external-change check warns before overwriting a file that changed on disk.
It is a check, not an exclusive lock against other editors.

Invalid recovery files are left untouched and reported at startup. To recover
manually, keep a copy of the file, then move it out of the session directory.
On reopen, named tabs use their recovery snapshots; a changed disk version is
not silently substituted for the recovered text.

Without a Unicode BOM, valid UTF-8 is preferred; otherwise the editor opens as
Windows-1252 and reports the assumption (`.nfo` files use OEM 437 instead).
Use **Encoding > Reopen** to explicitly
reinterpret bytes. Reopen discards current edits only after confirmation.
Conversions that cannot represent every character are rejected.

## Deliberate boundaries

- Maximum document size: 128 MiB, including decoded UTF-8 text.
- Search, line operations and JSON tools: 16 MiB. Compare: 16 MiB combined.
- Search/replacement expressions: 32 KiB; capture expansion is size-bounded.
- Recovery file: 256 MiB; at most 256 tabs. Failed backups are visible in the status bar.
- JSON trees: 20,000 nodes and 128 nesting levels. Duplicate
  object keys are rejected to prevent silent data loss. Number spellings and
  object key order are preserved. JSON and JSON5 are supported.
  Tree pointers are limited to 64 KiB each and tree metadata to 8 MiB total.
  Formatting is independent of the tree-node limit, with a 500,000-token bound.
- Compare includes inline differences but is not a merge tool. A processing
  budget and 32 KiB inline-line limit may produce coarser highlighting.
- Completion is bounded local analysis plus API data, not language-server
  semantic completion. At most 10,000 imported overloads are retained.
- Language imports are limited to 1 MiB each and 64 installed definitions.
  UDL 1.x, arbitrary lexer libraries, executable plugin APIs and UI theme packs
  are unsupported. Custom highlighting has a nesting/time budget.
- This is an initial independent release, not complete Notepad++ feature parity.
  No printing, macro recording, installer, code signing or update service.

## Build from source

Install Rust 1.98 or newer (MSVC x64 toolchain) and Visual Studio's **Desktop development
with C++** workload, including the Windows SDK. PowerShell 7 is recommended.

```powershell
.\scripts\build.ps1
```

The script verifies pinned source archive hashes, extracts editor sources and
language data, runs tests, builds the release binary, copies licenses, and
creates `dist\rstpd-0.2.4-windows-x64.zip` with a SHA-256 sidecar. Release
directories are versioned so building does not overwrite a running older EXE.
Rust dependencies are locked in `Cargo.lock`; the first build needs access to
the Rust package registry. The native source archives are already included.

For development, once Rust is on PATH:

```powershell
.\scripts\bootstrap.ps1
cargo run --locked
cargo test --locked --tests
cargo clippy --locked --all-targets -- -D warnings
.\scripts\smoke.ps1
```

The smoke script uses isolated profiles and sessions. It checks no-argument
first launch, startup with empty/recovered sessions, native controls, and
live comparison/JSON, advanced search, encoding, parameter hints, and imported
definition persistence. It terminates only the process it
started to simulate a crash, verifies recovery, and removes its temporary
session files without accessing your real recovery data. Own-window screenshots
are written to `target`.
Markdown checks verify rendered colors/font attributes in light/dark themes,
live editing, split/map views and recovery, rather than only checking lexer IDs.

## Source layout and trust boundary

- `src\ui.rs`: native Windows UI, event queue and command wiring.
- `src\toolbar.rs`: theme-aware Lucide vector icons and native tooltip ownership.
- `src\editor.rs`: editor/lexer FFI boundary and view configuration.
- `src\native_bridge.cxx`: minimal adapter releasing owned native document references.
- `src\completion.rs`: contextual suggestions, declarations, signatures and API data import.
- `src\udl.rs`: non-executable language definitions and Rust container highlighting.
- `src\markdown.rs`: Markdown source spans, semantic styles and folding.
- `src\syntax.rs`: shared semantic token roles and font-attribute selection.
- `src\json_tools.rs`: bounded JSON/JSON5 validation and lossless formatting.
- `src\core.rs`: encoding, bounded search, text transforms, diff and JSON spans.
- `src\session.rs`: atomic persistence, session validation and recovery worker.
- `src\languages.rs`: app-owned language aliases and keyword selection.
- `build.rs`: static native builds and build-time extraction of lexer constants,
  keyword data and semantic style roles.

There is no runtime lexer DLL loading or plugin path. The build reads only
language keyword/style data from SciTE; none of SciTE's commands or application
code are compiled into rstpd. JSON nodes are plain text, not rendered HTML.
Document contents do not launch programs or fetch resources.

Native document references have explicit ownership and automatic release,
including after their views are destroyed. Editor views are UI-thread-bound.
Scalar messages are checked against the pinned interface; pointer-bearing
messages require an explicit unsafe call behind typed buffer wrappers, and
text ranges are checked for bounds and UTF-8 boundaries.

Removing plugins eliminates that extension surface; it does **not** make an
editor vulnerability-free. Scintilla/Lexilla are native C++ dependencies and
remain part of the trust boundary. Dependency updates require rebuilding and
redistributing the application. This project is not affiliated with Notepad++.
