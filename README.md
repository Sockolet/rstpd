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

Open `dist\rstpd-1.4.0\rstpd.exe`, or extract the portable ZIP and open
`rstpd-1.4.0\rstpd.exe`. No installation or administrator access is needed.
Windows 10/11, x64. Keep the redistribution notices with the executable.

```powershell
.\dist\rstpd-1.4.0\rstpd.exe
.\dist\rstpd-1.4.0\rstpd.exe .\example.rs .\example.json
```

Use `--session-dir "C:\path\to\session"` for a separate workspace. Only one
instance may use a session directory at a time. Opening another instance
does not forward filenames to the existing instance.

Close an older rstpd instance before opening this release with the same session.
Version 1.1 reads version-1 sessions and saves version-2 sessions, including
language/completion definitions, optional editor-font and symbol-display preferences.
Version 0.1 deliberately refuses version-2 sessions rather than silently dropping
the new settings.

## Features

| Area | Implementation |
|---|---|
| Native UI | Windows title bar, menus, dialogs and controls; DPI-aware Segoe UI chrome, light/dark/system themes, closeable tabs and a compact Lucide icon toolbar |
| Tab management | Drag to reorder, pin tabs to the left, persist order/pins, and double-click empty space after the last tab to create a document |
| External changes | Background file monitoring; automatic reload of clean files, explicit confirmation before discarding unsaved edits |
| Editor font | Native family/size selection, 4-72 pt, saved per workspace and shared by both editing panes and future tabs |
| Character count | Total characters beside line/column, or selected out of total when text is selected |
| Show symbols | Independent whitespace, EOL, non-printing/control-character markers, Show All, indentation guides and wrap markers |
| Highlighting | All 94 source-language inventory entries mapped, plus the full pinned Lexilla catalog; filename detection, mode selection, and data-only UDL 2.0/2.1 import |
| Markdown | CommonMark/GFM parsing with visible headings, emphasis, links, lists, quotes, tasks, tables, inline/fenced/indented code and strikethrough |
| Completion | Keywords, local declarations, member/context suggestions, common built-in APIs, parameter hints and importable completion signatures |
| Multiple edits | Alt+drag rectangular selection, Ctrl+click multiple carets, multi-selection typing and paste |
| Split screen | Two editable views of the same document or two different documents |
| Document map | Clickable compact view with the visible text range highlighted; scroll the map for long documents |
| Search | Normal, extended and advanced regex, including look-ahead/look-behind and pattern backreferences; case/whole-word options, wrap-around and replacement |
| Find All | Current/all-open-tab searches with grouped matches, highlighted snippets, exact-match navigation and a resizable bottom results panel |
| Find in Files | Folder, filename filters, optional recursion/hidden files, cancellable background search and navigable disk results |
| Compare | Live line/character differences, moved-line markers, aligned panes, ignore options, selected-line/clipboard/last-saved comparisons |
| JSON | Lossless JSON/JSON5 pretty-print/minify, automatically refreshed tree, RFC 6901 pointers and source-span navigation |
| Text operations | Upper/lower/title/sentence/inverted case; case-sensitive, case-insensitive, natural and exact decimal sorting; reverse/join, deduplication and whitespace operations |
| Encoding | UTF-8, UTF-16/32 LE/BE, Windows/ISO/OEM code pages, Shift-JIS, EUC-JP, ISO-2022-JP, GBK/GB18030, Big5, EUC-KR, KOI8 and Mac encodings |
| Line endings | CRLF, LF and CR conversion |
| Recovery | Background atomic snapshots of all tabs, including unnamed documents; restore after restart or crash |

Each split pane owns its tab bar and independent selection.
The icon toolbar keeps the same New, Open, Save, Find, Split, Compare, JSON tree
and Document map actions. Hover for a descriptive tooltip and shortcut.
Button names remain available to accessibility tools, and keyboard shortcuts
and menus are unchanged. Icons are drawn natively at the current DPI and
recolored for light/dark themes; no icon font or browser runtime is required.
Dark chrome uses restrained panel boundaries, a subdued native-menu separator,
and spacing between toolbar and tabs. Native menu behavior and light-theme
rendering are preserved. Drag the visible divider to resize; its wider hit area
shows a horizontal resize cursor.
**Compare** compares the selected split documents, or the active tab with the
next tab when not split (wrapping at the end).
Editing either document automatically schedules a comparison refresh.
Old worker results are discarded if the documents changed in the meantime;
refresh does not move your editing caret. EOL representation is ignored during
line comparison.

### Tabs and external changes

Drag a tab to change its order. Right-click it, or use **File**, to pin/unpin
or move it left/right. Pinned tabs show `[P]` and stay before unpinned tabs in
their pane; dragging cannot cross that boundary. Pinning does not make the file read-only.
Order and pin state are restored with the workspace. Double-click the unused
tab-strip area immediately after the last tab to create an empty untitled tab.

The tab context menu provides **Open in split view**, which moves the
right-clicked tab to the other pane, reusing an existing split.
**Clone to other pane** explicitly shares the document between both groups.
New/open use the focused group; Ctrl+Tab cycles within that group, and F6 changes
panes. Closing a clone removes only that view. Closing a group's last tab
collapses the empty group; moving its last tab leaves a new blank tab.
The **Split** command moves the active tab to the right pane; when it is the only
tab, Split shows that document in both panes instead of leaving a blank tab.
Toggling an existing split off merges both groups without discarding documents.
Recovery stores both groups, ordering, selections, and focused pane. Older
recovery files without group metadata open all their documents in the left group.

**Compare with current view**
compares the document active before the right-click against the clicked tab,
not the next tab: the current document appears on the left and the clicked
document on the right. Comparing a tab with itself is disabled. These actions
reuse existing documents, including unsaved edits; they do not create copies.
Opening a normal split clears any active comparison.

**View > Automatically reload external changes** is enabled by default and
saved per workspace. A background worker polls named tabs about once a second,
with a periodic content check for same-size/same-timestamp rewrites. Clean
buffers reload automatically, preserving the visible selection and scroll
position where possible. Unsaved text or encoding/EOL changes require a
confirmation; choosing No preserves the buffer and the existing save-conflict
check. An unchanged rejected disk version is not repeatedly prompted.
Missing, unreadable or unstable files are reported in the status bar, not
treated as empty files. This is file refresh, not log-follow/tail mode.

### Comparison options and sources

**Tools > Comparison options** controls whitespace, case and empty-line
ignoring, moved-line detection and pane alignment. To ignore a regex, enter
it in the Find field, then choose **Use current Find expression as ignore
regex**; this always interprets the expression as a regex, independently of
the Find mode. **Clear ignore regex** removes it. Settings persist with the
workspace. Ignore options decide which lines differ; inline spans retain
original character coordinates.

Alignment uses visual annotation rows and leading pane space, never inserted
document text. Word wrapping is temporarily disabled while aligned and restored
on clearing comparison. Red/green marks indicate left/right changes; blue marks
identify matching lines that moved. F7/Shift+F7 navigate difference groups.
Alignment is capped at 20,000 spacer rows; disable alignment for larger gaps.

**Compare selected lines in both panes** needs two different split documents
and one selection in each. Selections expand to full lines; editing ends that
selection comparison, so reselect before comparing again. **Compare with
clipboard** and **Compare with last saved file** create ordinary untitled
snapshot tabs; they never save or overwrite the source file. Whole-document
comparisons continue to refresh after edits.

The independently implemented scope takes behavioral inspiration from
[ComparePlus](https://github.com/pnedev/comparePlus), not its code. There is no
plugin loading, Git/SVN comparison, merge operation or multiple simultaneous
comparison pairs.

Line operations affect complete selected lines, or the whole document when
there is no selection. Case conversion operates on selected text, including
rectangular/multiple selections. These edits participate in undo.
The status bar shows `4995 characters` with no text selected, or
`852 of 4995 characters` for a selection. Counts follow the active editing
pane and include the actual text in multiple/rectangular selections without
double-counting overlaps or counting virtual space. Characters mean Unicode
code points, not UTF-8 bytes: `U+1F680` counts as one, combining marks are
separate, and CR/LF count separately. Tabs and other stored characters count too.
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

### Editor font and zoom

Use **View > Editor font...** to select an installed scalable font family and
size from **4 to 72 pt**. The chooser offers only family and size, not font style,
color, underline or strikeout. Confirming applies the preference to both panes
and future tabs and resets both panes' transient zoom to zero; **View > Reset
zoom** subsequently returns to that chosen base size. Cancel changes neither
the preference nor zoom. Document text, dirty state and undo history are not
changed, and the Segoe UI font used by menus, tabs and other chrome is unchanged.

The preference is saved in the current workspace's recovery session, including
workspaces selected with `--session-dir`. Existing version-1/version-2 sessions
without it use Consolas 11 pt. Invalid stored preferences are reported without
rewriting recovery data. Theme/language changes and tab switches retain the
choice. Syntax bold/italic/underline and colors are retained, as are explicit
UDL font-family and size overrides; the document map stays compact at 2 pt.
Line-number margins adjust to the font size, zoom and document line count,
reserving at least two digits with modest font-relative padding rather than a
fixed minimum width. The separate folding controls remain available.
The Windows chooser reports names that cannot fit its 31-UTF-16-unit family
field rather than truncating Unicode names.

### Show symbols

Use **View > Show symbols** to toggle these options independently:

| Option | Display |
|---|---|
| Show space and tab | Dots for spaces and arrows for tabs |
| Show end of line | CR/LF markers for the actual line-ending characters |
| Show non-printing characters | Named markers for non-breaking/Unicode spaces, zero-width characters, directional formatting and separators |
| Show control characters & Unicode EOL | Named markers for C0/C1 controls, DEL, NEL, LS and PS; normal tabs and CR/LF remain controlled by the first two options |
| Show all characters | Turns the preceding four options on together, or off together when all are already enabled |
| Show indent guide | Vertical guides for indentation |
| Show wrap symbol | Visual wrap markers when **View > Word wrap** is enabled |

**Show all characters** does not alter indentation guides or wrap markers.
NEL/LS/PS are visible if either non-printing or control-character display is on.
Hidden control characters use an unboxed space so they remain selectable.
These are view settings only: bytes, encoding, dirty state and undo history are
unchanged. Settings persist per workspace, survive theme/font changes, and apply
to both editing panes and new tabs. The document map and search results remain
uncluttered. Older sessions retain the previous control-character visibility.

### Find All and search results

Open Find/Replace with **Ctrl+F** or **Ctrl+H**, enter a query, then choose
**Find all: current document** or **Find all: all open documents**. The same
commands are available under **Search**. Normal, extended and regex modes,
**Match case**, and **Whole word** use the same settings as Find Next.
Find and Replace inputs have permanent labels, contrasting fill colors and
visible borders, with an accent border on the focused field. Search and result
buttons have filled, outlined surfaces and distinct hover, pressed, focus and
disabled states in both themes. At narrower window sizes, Find All actions move
to an additional row instead of overlapping other controls.
Open documents are searched as they appear in the editor, including unsaved
edits and untitled tabs; this does not search unopened files on disk.

The bottom panel lists a summary, collapsible document groups, and one row per
match with its line, column and a highlighted snippet. Columns count Unicode
characters and honor tab stops. Multiline and zero-width matches are labeled;
navigation selects the full match even when the preview is shortened.

- Double-click a match, or move the results caret to it and press **Enter**.
- **F4 / Shift+F4** or **Next / Previous** navigates and wraps through matches.
- Drag the divider above the panel to resize it.
- **Close** hides the panel without losing results; **Ctrl+Alt+R** reopens it.
- **Clear** removes the displayed results; **Cancel** stops an active search.
- Results are read-only but can be selected/copied. Editing commands cannot
  accidentally modify the source document while the results editor has focus.

Each search replaces the previous result list. Searching runs in a background
worker over snapshots, without changing document text or dirty state. If a
document changes or closes after searching, its old results cannot navigate
to potentially incorrect positions; run Find All again. Changes to other
documents do not invalidate unaffected matches. Search results are transient
and are not written to recovery files.

Find All retains at most **10,000 matches** and explicitly reports truncation.
It accepts up to **128 MiB per document / 256 MiB combined**. Match collection
and replacement phases retain a two-second budget per 16 MiB of input (up to
16 seconds); Find All allows ten seconds per 64 MiB of combined input (up to
40 seconds). Budgets are checked between matches, not hard execution deadlines.
Cancellation is checked between matches/documents, so a running regex evaluation
may finish before cancellation is acknowledged. A processing failure is shown
in the panel, not reported as zero matches or a silently partial success.

### Find in Files

Choose **Search > Find in files...** (**Ctrl+Shift+F**) or its Find-panel button.
Enter the folder, optionally use **Browse...**, and choose filename filters
such as `*.rs;*.toml;!generated*`. `*` matches any filename characters and `?`
matches one; spaces or semicolons separate patterns and `!` excludes matches.
Filters apply to basenames, not directory paths, and are case-insensitive on
Windows. Empty filters include all filenames. **Subfolders** is initially
enabled; **Hidden files** is initially disabled.

**Search folder** reuses the Find expression, mode, case and whole-word settings.
It searches disk contents, not unsaved buffers. Results use the same bottom panel,
navigation and cancellation controls as Find All. Activating a result verifies
the file contents before opening/selecting it; changed files or differing
unsaved buffers are rejected without replacing the buffer.

Binary, oversized, unreadable and linked files are skipped with visible
warnings. Limits are 128 MiB per file (including decoded UTF-8), 512 MiB total, 10,000 searched files,
100,000 directory entries, 30 seconds between checks and 10,000 retained matches.
Regex evaluation or a filesystem read already in progress may delay cancellation.
This is search only: folder-wide replacement is not implemented.

## Language definitions and completion

The Language menu starts with **Plain text**, followed by fixed **A-C, D-F,
G-I, J-L, M-O, P-R, S-U and V-Z** groups. Languages are alphabetized within
each group, with additional columns for longer lists. Imported definitions
have their own **User-defined** group; import/removal commands are at the bottom.

`assets\language-coverage.psv` records the 94 source-language entries and their
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
.\dist\rstpd-1.4.0\rstpd.exe --import-language .\language.xml .\example.rstlang
.\dist\rstpd-1.4.0\rstpd.exe --completion-api .\functions.xml .\example.rs
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
| Ctrl+Alt+Enter / Ctrl+Shift+Enter | Find All in current document / all open documents |
| F4 / Shift+F4 | Next / previous Find All result |
| Ctrl+Alt+R | Toggle search-results panel |
| Ctrl+D / Ctrl+Shift+L | Select next / all occurrences |
| Ctrl+U / Ctrl+Shift+U | Lowercase / uppercase selections |
| Ctrl+Space / Ctrl+Shift+Space | Completion suggestions / function parameter hint |
| Ctrl+Alt+Right / F6 | Toggle split / focus other pane |
| F7 / Shift+F7 | Next / previous difference |
| Ctrl+Alt+J / Ctrl+Alt+T | Format JSON / toggle JSON tree |
| Ctrl+mouse wheel | Editor zoom |
| Escape | Close search bar, or hide search results when that panel has focus |

Extended search/replacement recognizes `\n`, `\r`, `\t`, `\\`, `\0`,
`\xHH` and `\uHHHH`. Invalid escapes are reported, not silently interpreted.
Regex uses the Rust `fancy-regex` engine, with multiline/CRLF-aware anchors and Unicode
word boundaries. Replacement captures use `$1` or `${name}`; `$$` is a
literal dollar. Look-around and pattern backreferences are supported, for
example `(?<=prefix:)(\w+)\s+\1(?!x)`. This is not complete Boost/PCRE dialect
compatibility. Backtracking is limited to 500,000 steps, bulk operations check
input-scaled processing budgets (two seconds per 16 MiB), and oversized expansions fail before replacing
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

- Maximum document size: 256 MiB, including decoded UTF-8 text.
- Find/replace and directory search: 128 MiB per document/file; Find All: 256 MiB combined.
- Line operations and JSON tools: 16 MiB. Compare: 16 MiB combined.
- Search/replacement expressions: 32 KiB; capture expansion is size-bounded.
- Recovery file: 512 MiB; at most 256 tabs. Failed backups are visible in the status bar.
- Larger files require additional memory for decoding, editor storage, undo and search/recovery snapshots; limits are not a memory-availability guarantee. Older releases may reject recovery files above their former 256 MiB limit.
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
creates `dist\rstpd-1.4.0-windows-x64.zip` with a SHA-256 sidecar. Release
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

`tests\ui_selection.ps1` exercises native pane groups, keyboard navigation,
move/clone actions, comparison, and recovery with an isolated synthetic session.
Pass `-ChromeScreenshotDirectory .\target\ui-chrome` to also check native menus
and theme boundaries. Both UI scripts accept `-Executable` and default to
`target\release\rstpd.exe`.

`tests\ui_visual.ps1` captures desktop pixels from its own synthetic window and
rejects missing text or incorrect editor backgrounds. It requires an available
interactive desktop. On a locked/disconnected desktop, `-NativeCapture
-GdiDiagnostic` provides a clearly named software-rendering diagnostic only;
it does not verify normal DirectWrite desktop presentation. Plain PrintWindow
output is not sufficient visual evidence for DirectWrite views. Captures default
to `target\ui-visual`, or the supplied `-OutputDirectory`.

## Source layout and trust boundary

- `src\ui.rs`: native Windows UI, event queue and command wiring.
- `src\controls.rs`: visible button/input surfaces and DPI-aware search layout.
- `src\toolbar.rs`: theme-aware Lucide vector icons and native tooltip ownership.
- `src\editor.rs`: editor/lexer FFI boundary and view configuration.
- `src\native_bridge.cxx`: minimal adapter releasing owned native document references.
- `src\completion.rs`: contextual suggestions, declarations, signatures and API data import.
- `src\udl.rs`: non-executable language definitions and Rust container highlighting.
- `src\markdown.rs`: Markdown source spans, semantic styles and folding.
- `src\syntax.rs`: shared semantic token roles and font-attribute selection.
- `src\json_tools.rs`: bounded JSON/JSON5 validation and lossless formatting.
- `src\core.rs`: encoding, bounded search, text transforms, diff and JSON spans.
- `src\search_results.rs`: bounded multi-document matches, Unicode previews and result-row mappings.
- `src\symbols.rs`: persistent symbol-display flags and non-printing character names.
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
