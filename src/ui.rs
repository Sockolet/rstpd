use crate::{
    comparison::{self, CompareOptions, Comparison},
    completion::{self, Api},
    controls::{self, SearchLayout},
    core::{
        self, CaseOp, Difference, EditorFont, Encoding, Eol, JsonNode, LineOp, Result, Search,
        SearchMode,
    },
    editor::{self, DocumentHandle, Editor, Palette, sci::*},
    folder_search::{self, FolderOptions},
    languages::{self, Language},
    monitor::{self, Monitor},
    search_results::{self, Input as SearchInput, Link as ResultLink, Results as SearchResults},
    session::{self, DocumentSnapshot, RecoveryWorker, Session},
    symbols::ShowSymbols,
    tabs,
    toolbar::{self, Icon, Tooltips},
    udl::{self, Highlight},
};
use std::{
    cell::{Cell, RefCell},
    collections::{HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    mem::{size_of, zeroed},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::{
        Com::CoTaskMemFree,
        DataExchange::*,
        LibraryLoader::*,
        Memory::*,
        Ole::RevokeDragDrop,
        Registry::*,
        SystemServices::{MK_LBUTTON, SS_CENTERIMAGE, SS_NOTIFY},
    },
    UI::{
        Controls::{Dialogs::*, *},
        HiDpi::*,
        Input::KeyboardAndMouse::*,
        Shell::*,
        WindowsAndMessaging::*,
    },
};

const NEW: usize = 1001;
const OPEN: usize = 1002;
const SAVE: usize = 1003;
const SAVE_AS: usize = 1004;
const CLOSE: usize = 1005;
const EXIT: usize = 1006;
const UNDO: usize = 1010;
const REDO: usize = 1011;
const CUT: usize = 1012;
const COPY: usize = 1013;
const PASTE: usize = 1014;
const SELECT_ALL: usize = 1015;
const UPPER: usize = 1020;
const LOWER: usize = 1021;
const SORT: usize = 1022;
const SORT_DESC: usize = 1023;
const UNIQUE: usize = 1024;
const TRIM: usize = 1025;
const REMOVE_EMPTY: usize = 1026;
const DUPLICATE: usize = 1027;
const DELETE_LINE: usize = 1028;
const ADD_NEXT: usize = 1029;
const SELECT_MATCHES: usize = 1030;
const FIND: usize = 1040;
const FIND_NEXT: usize = 1041;
const FIND_PREVIOUS: usize = 1042;
const REPLACE: usize = 1043;
const REPLACE_ALL: usize = 1044;
const SPLIT: usize = 1050;
const MAP: usize = 1051;
const WRAP: usize = 1052;
const ZOOM_RESET: usize = 1053;
const EDITOR_FONT: usize = 1054;
const THEME_SYSTEM: usize = 1060;
const THEME_LIGHT: usize = 1061;
const THEME_DARK: usize = 1062;
const COMPARE: usize = 1070;
const DIFF_NEXT: usize = 1071;
const DIFF_PREVIOUS: usize = 1072;
const COMPARE_CLEAR: usize = 1073;
const JSON_FORMAT: usize = 1080;
const JSON_COMPACT: usize = 1081;
const JSON_TREE: usize = 1082;
const JSON_REFRESH: usize = 1083;
const EOL_CRLF: usize = 1090;
const EOL_LF: usize = 1091;
const EOL_CR: usize = 1092;
const ENCODING_BASE: usize = 5000;
const REOPEN_BASE: usize = 5200;
const ABOUT: usize = 1150;
const SEARCH_CLOSE: usize = 1160;
const FIND_ALL_CURRENT: usize = 1170;
const FIND_ALL_OPEN: usize = 1171;
const RESULTS_TOGGLE: usize = 1172;
const RESULTS_NEXT: usize = 1173;
const RESULTS_PREVIOUS: usize = 1174;
const RESULTS_CLEAR: usize = 1175;
const RESULTS_CLOSE: usize = 1176;
const RESULTS_CANCEL: usize = 1177;
const FIND_FILES: usize = 1180;
const FIND_FILES_RUN: usize = 1181;
const FIND_FILES_BROWSE: usize = 1182;
const TAB_PIN: usize = 1500;
const TAB_LEFT: usize = 1501;
const TAB_RIGHT: usize = 1502;
const MONITOR_FILES: usize = 1503;
const TAB_SPLIT: usize = 1504;
const TAB_COMPARE: usize = 1505;
const TAB_CLONE: usize = 1506;
const COMPARE_SELECTION: usize = 1510;
const COMPARE_CLIPBOARD: usize = 1511;
const COMPARE_SAVED: usize = 1512;
const COMPARE_IGNORE_SPACE: usize = 1520;
const COMPARE_IGNORE_CASE: usize = 1521;
const COMPARE_IGNORE_EMPTY: usize = 1522;
const COMPARE_MOVES: usize = 1523;
const COMPARE_ALIGN: usize = 1524;
const COMPARE_REGEX: usize = 1525;
const COMPARE_REGEX_CLEAR: usize = 1526;
const SYMBOL_SPACE: usize = 1300;
const SYMBOL_EOL: usize = 1301;
const SYMBOL_NONPRINTING: usize = 1302;
const SYMBOL_CONTROLS: usize = 1303;
const SYMBOL_ALL: usize = 1304;
const SYMBOL_INDENT: usize = 1305;
const SYMBOL_WRAP: usize = 1306;
const TITLE_CASE: usize = 1200;
const SENTENCE_CASE: usize = 1201;
const INVERT_CASE: usize = 1202;
const SORT_IGNORE_CASE: usize = 1210;
const SORT_DESC_IGNORE_CASE: usize = 1211;
const SORT_NATURAL: usize = 1212;
const SORT_NUMERIC: usize = 1213;
const SORT_NUMERIC_DESC: usize = 1214;
const SORT_NUMERIC_COMMA: usize = 1215;
const REVERSE_LINES: usize = 1216;
const UNIQUE_ADJACENT: usize = 1217;
const TRIM_START: usize = 1218;
const TRIM_BOTH: usize = 1219;
const REMOVE_EMPTY_ONLY: usize = 1220;
const JOIN_LINES: usize = 1221;
const IMPORT_LANGUAGE: usize = 1400;
const IMPORT_API: usize = 1401;
const REMOVE_LANGUAGE: usize = 1402;
const PARAMETER_HINT: usize = 1403;
const LANGUAGE_BASE: usize = 2000;
const TREE_ID: usize = 301;
const TAB_ID: usize = 302;
const RESULTS_EDITOR_ID: usize = 105;
const RESULTS_DIVIDER_ID: usize = 304;
const TOOLBAR: [toolbar::Button; 8] = [
    toolbar::Button {
        command: NEW,
        name: "New",
        tooltip: "New document (Ctrl+N)",
        icon: Icon::New,
    },
    toolbar::Button {
        command: OPEN,
        name: "Open",
        tooltip: "Open file (Ctrl+O)",
        icon: Icon::Open,
    },
    toolbar::Button {
        command: SAVE,
        name: "Save",
        tooltip: "Save document (Ctrl+S)",
        icon: Icon::Save,
    },
    toolbar::Button {
        command: FIND,
        name: "Find",
        tooltip: "Find and replace (Ctrl+F)",
        icon: Icon::Find,
    },
    toolbar::Button {
        command: SPLIT,
        name: "Split",
        tooltip: "Toggle split view (Ctrl+Alt+Right)",
        icon: Icon::Split,
    },
    toolbar::Button {
        command: COMPARE,
        name: "Compare",
        tooltip: "Compare selected panes (or next tab)",
        icon: Icon::Compare,
    },
    toolbar::Button {
        command: JSON_TREE,
        name: "JSON tree",
        tooltip: "Toggle JSON tree (Ctrl+Alt+T)",
        icon: Icon::Json,
    },
    toolbar::Button {
        command: MAP,
        name: "Document map",
        tooltip: "Toggle document map",
        icon: Icon::Map,
    },
];

#[derive(Clone, Copy)]
struct TabDrag {
    id: u64,
    x: i32,
    y: i32,
    target: usize,
    dragging: bool,
}

thread_local! {
    static EVENTS: RefCell<VecDeque<Event>> = const { RefCell::new(VecDeque::new()) };
    static COLORS: Cell<Palette> = Cell::new(Palette::new(false));
    static PANEL_BRUSH: Cell<HBRUSH> = const { Cell::new(null_mut()) };
    static FIELD_BRUSH: Cell<HBRUSH> = const { Cell::new(null_mut()) };
    static UI_FONT: Cell<HFONT> = const { Cell::new(null_mut()) };
    static MENU_LABELS: RefCell<Vec<(String, bool)>> = const { RefCell::new(Vec::new()) };
    static MENUS: RefCell<Vec<HMENU>> = const { RefCell::new(Vec::new()) };
    static TREE_UPDATING: Cell<bool> = const {Cell::new(false)};
    static HOT_BUTTON: Cell<HWND> = const {Cell::new(null_mut())};
    static ICON_ERROR_REPORTED: Cell<bool> = const {Cell::new(false)};
    static TAB_DRAG: Cell<Option<TabDrag>> = const {Cell::new(None)};
    static SPLIT_BOUNDS: Cell<Option<RECT>> = const { Cell::new(None) };
    static SPLIT_DRAG_OFFSET: Cell<Option<i32>> = const { Cell::new(None) };
    static PANEL_BOUNDS: RefCell<Vec<RECT>> = const { RefCell::new(Vec::new()) };
    static PANE_STRIPS: Cell<[Option<RECT>; 2]> = const { Cell::new([None, None]) };
    static ACTIVE_PANE: Cell<usize> = const { Cell::new(0) };
}

fn select_pane(hwnd: HWND, pane: usize) {
    let previous = ACTIVE_PANE.with(|active| active.replace(pane));
    if previous != pane {
        for rect in PANE_STRIPS.with(Cell::get).into_iter().flatten() {
            unsafe {
                InvalidateRect(hwnd, &rect, 1);
            }
        }
    }
}

fn panel_edges(rect: RECT, split: Option<RECT>) -> Vec<RECT> {
    let mut edges = vec![
        RECT {
            bottom: rect.top + 1,
            ..rect
        },
        RECT {
            top: rect.bottom - 1,
            ..rect
        },
    ];
    if !split.is_some_and(|split| rect.left == split.right) {
        edges.push(RECT {
            right: rect.left + 1,
            ..rect
        });
    }
    if !split.is_some_and(|split| rect.right == split.left) {
        edges.push(RECT {
            left: rect.right - 1,
            ..rect
        });
    }
    edges
}

fn split_hit(x: i32, y: i32) -> Option<i32> {
    SPLIT_BOUNDS.with(Cell::get).and_then(|rect| {
        (x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom)
            .then_some(x - rect.left)
    })
}

enum Event {
    RenderError(String),
    Command(usize),
    Resize,
    Close,
    Tick,
    Theme,
    Dpi,
    Tab(usize),
    TabFocus(usize),
    CloseTab(usize, u64),
    Tree(usize),
    Focus(usize),
    Changed(usize),
    Style(usize),
    Updated(usize),
    Character(usize, i32),
    Drop(Vec<PathBuf>),
    Map(i32),
    SplitDrag(i32),
    ResultActivate,
    ResultsResize(i32),
    MoveTab(u64, usize),
    PinTab(u64),
    SplitTab(u64),
    CloneTab(u64),
    CompareTabs(u64, u64),
}
fn queue(event: Event) {
    EVENTS.with(|q| q.borrow_mut().push_back(event));
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

const EDITOR_FONT_FLAGS: CHOOSEFONT_FLAGS = CF_SCREENFONTS
    | CF_SCALABLEONLY
    | CF_FORCEFONTEXIST
    | CF_INITTOLOGFONTSTRUCT
    | CF_LIMITSIZE
    | CF_ENABLEHOOK
    | CF_NOSCRIPTSEL;
const FONT_STYLE_SUBCLASS: usize = 4;

fn editor_logfont(font: &EditorFont, dpi: u32) -> Result<LOGFONTW> {
    let family: Vec<u16> = font.family().encode_utf16().collect();
    let mut logfont = LOGFONTW::default();
    if family.len() >= logfont.lfFaceName.len() {
        return Err(
            "The Windows font dialog supports at most 31 UTF-16 units in a font family; the name has not been truncated."
                .into(),
        );
    }
    let height = i32::try_from((u64::from(font.size_hundredths()) * u64::from(dpi) + 3600) / 7200)
        .map_err(|_| "The window DPI is too large for the Windows font dialog.")?;
    if height == 0 {
        return Err("Could not determine a valid font height for the window DPI.".into());
    }
    logfont.lfHeight = -height;
    logfont.lfWeight = FW_NORMAL as i32;
    logfont.lfCharSet = DEFAULT_CHARSET;
    logfont.lfFaceName[..family.len()].copy_from_slice(&family);
    Ok(logfont)
}

fn font_size_label(size_hundredths: u32) -> String {
    let points = size_hundredths / 100;
    let fraction = size_hundredths % 100;
    if fraction == 0 {
        points.to_string()
    } else {
        format!("{points}.{fraction:02}")
            .trim_end_matches('0')
            .to_owned()
    }
}

fn font_from_dialog(logfont: &LOGFONTW, size_tenths: i32) -> Result<EditorFont> {
    let size_hundredths = u32::try_from(size_tenths)
        .ok()
        .and_then(|size| size.checked_mul(10))
        .ok_or("The font dialog returned an invalid point size.")?;
    let end = logfont
        .lfFaceName
        .iter()
        .position(|unit| *unit == 0)
        .ok_or("The font dialog returned an unterminated font family.")?;
    let family = String::from_utf16(&logfont.lfFaceName[..end])
        .map_err(|error| format!("The font dialog returned an invalid font family: {error}"))?;
    EditorFont::new(family, size_hundredths)
}

struct FontDialogState {
    size_text: Vec<u16>,
    initialization_failed: bool,
}

unsafe extern "system" fn font_style_proc(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    unsafe {
        if message == WM_ENABLE && w != 0 {
            EnableWindow(hwnd, 0);
            return 0;
        }
        if message == WM_NCDESTROY {
            RemoveWindowSubclass(hwnd, Some(font_style_proc), id);
        }
        DefSubclassProc(hwnd, message, w, l)
    }
}

unsafe extern "system" fn editor_font_hook(
    hwnd: HWND,
    message: u32,
    _w: WPARAM,
    l: LPARAM,
) -> usize {
    if message == WM_INITDIALOG {
        unsafe {
            let dialog = &*(l as *const CHOOSEFONTW);
            let state = &mut *(dialog.lCustData as *mut FontDialogState);
            let style = GetDlgItem(hwnd, cmb2 as i32);
            let label = GetDlgItem(hwnd, stc2 as i32);
            // CF_NOSTYLESEL only clears the initial selection. Disable the SDK style control,
            // including attempts by the common dialog to re-enable it after a family change.
            if style.is_null()
                || label.is_null()
                || SetWindowSubclass(style, Some(font_style_proc), FONT_STYLE_SUBCLASS, 0) == 0
            {
                state.initialization_failed = true;
            } else {
                EnableWindow(style, 0);
                EnableWindow(label, 0);
                state.initialization_failed = IsWindowEnabled(style) != 0
                    || SetDlgItemTextW(hwnd, cmb3 as i32, state.size_text.as_ptr()) == 0;
            }
            if state.initialization_failed {
                PostMessageW(hwnd, WM_COMMAND, IDCANCEL as usize, 0);
            }
        }
    }
    0
}

fn choose_editor_font(owner: HWND, dpi: u32, current: &EditorFont) -> Result<Option<EditorFont>> {
    let mut logfont = editor_logfont(current, dpi)?;
    let mut state = FontDialogState {
        size_text: wide(&font_size_label(current.size_hundredths())),
        initialization_failed: false,
    };
    let mut dialog = CHOOSEFONTW {
        lStructSize: size_of::<CHOOSEFONTW>() as u32,
        hwndOwner: owner,
        lpLogFont: &mut logfont,
        Flags: EDITOR_FONT_FLAGS,
        lCustData: (&mut state as *mut FontDialogState) as isize,
        lpfnHook: Some(editor_font_hook),
        nSizeMin: (EditorFont::MIN_SIZE_HUNDREDTHS / 100) as i32,
        nSizeMax: (EditorFont::MAX_SIZE_HUNDREDTHS / 100) as i32,
        ..CHOOSEFONTW::default()
    };
    let accepted = unsafe { ChooseFontW(&mut dialog) } != 0;
    let error = if accepted {
        0
    } else {
        unsafe { CommDlgExtendedError() }
    };
    if state.initialization_failed {
        return Err("Could not initialize the family-and-size-only font dialog.".into());
    }
    if !accepted {
        return if error == 0 {
            Ok(None)
        } else {
            Err(format!("Font dialog failed: {error:#x}"))
        };
    }
    font_from_dialog(&logfont, dialog.iPointSize).map(Some)
}

unsafe fn new_menu(popup: bool) -> HMENU {
    let menu = unsafe {
        if popup {
            CreatePopupMenu()
        } else {
            CreateMenu()
        }
    };
    MENUS.with(|menus| menus.borrow_mut().push(menu));
    menu
}

unsafe fn menu_item(menu: HMENU, id: usize, label: &str, popup: bool, top: bool) {
    if id == 0 {
        unsafe {
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
        }
        return;
    }
    let data = MENU_LABELS.with(|labels| {
        let mut labels = labels.borrow_mut();
        labels.push((label.into(), top));
        labels.len()
    });
    unsafe {
        AppendMenuW(
            menu,
            MF_OWNERDRAW | if popup { MF_POPUP } else { 0 },
            id,
            data as *const u16,
        );
    }
}

unsafe fn paint_label(dc: HDC, rect: &RECT, title: &str, selected: bool, disabled: bool) {
    unsafe {
        let palette = COLORS.with(Cell::get);
        let brush = CreateSolidBrush(if selected {
            palette.selection
        } else {
            palette.panel
        });
        FillRect(dc, rect, brush);
        DeleteObject(brush);
        let old = SelectObject(dc, UI_FONT.with(Cell::get));
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(
            dc,
            if disabled {
                palette.muted
            } else {
                palette.text
            },
        );
        let (title, shortcut) = title.split_once('\t').unwrap_or((title, ""));
        let mut r = *rect;
        r.left += 12;
        r.right -= 12;
        let text = wide(title);
        DrawTextW(
            dc,
            text.as_ptr(),
            (text.len() - 1) as i32,
            &mut r,
            DT_SINGLELINE | DT_VCENTER,
        );
        if !shortcut.is_empty() {
            let shortcut = wide(shortcut);
            SetTextColor(dc, palette.muted);
            DrawTextW(
                dc,
                shortcut.as_ptr(),
                (shortcut.len() - 1) as i32,
                &mut r,
                DT_SINGLELINE | DT_VCENTER | DT_RIGHT | DT_NOPREFIX,
            );
        }
        SelectObject(dc, old);
    }
}
fn window_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut text = vec![0u16; len + 1];
        let used = GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32);
        String::from_utf16_lossy(&text[..used.max(0) as usize])
    }
}
fn set_text(hwnd: HWND, text: &str) {
    unsafe {
        SetWindowTextW(hwnd, wide(text).as_ptr());
    }
}
pub fn show_error(error: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(error).as_ptr(),
            wide("rstpd").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
fn ask(hwnd: HWND, text: &str, flags: u32) -> i32 {
    unsafe { MessageBoxW(hwnd, wide(text).as_ptr(), wide("rstpd").as_ptr(), flags) }
}

#[repr(C)]
struct Notification {
    header: NMHDR,
    position: isize,
    ch: i32,
    modifiers: i32,
    modification: i32,
}

// Native separators lie outside owner-draw menu items. Paint after Windows'
// nonclient lifecycle, retaining the native menu, keyboard and accessibility.
unsafe fn paint_menu_rule(hwnd: HWND, supplied: HDC) {
    unsafe {
        if !COLORS.with(Cell::get).dark || GetMenu(hwnd).is_null() {
            return;
        }
        let mut window: RECT = zeroed();
        let mut client: RECT = zeroed();
        let mut origin = POINT { x: 0, y: 0 };
        if GetWindowRect(hwnd, &mut window) == 0
            || GetClientRect(hwnd, &mut client) == 0
            || ClientToScreen(hwnd, &mut origin) == 0
        {
            return;
        }
        let dc = if supplied.is_null() {
            GetWindowDC(hwnd)
        } else {
            supplied
        };
        if dc.is_null() {
            return;
        }
        let rect = RECT {
            left: origin.x - window.left,
            right: origin.x - window.left + client.right,
            top: origin.y - window.top - 1,
            bottom: origin.y - window.top,
        };
        let brush = CreateSolidBrush(COLORS.with(Cell::get).panel);
        FillRect(dc, &rect, brush);
        DeleteObject(brush);
        if supplied.is_null() {
            ReleaseDC(hwnd, dc);
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match message {
            0x8037 => {
                paint_menu_rule(hwnd, null_mut());
                return 0;
            }
            WM_NCPAINT | WM_NCACTIVATE | WM_SETTEXT | WM_WINDOWPOSCHANGED | WM_EXITMENULOOP
            | WM_PRINT => {
                let result = DefWindowProcW(hwnd, message, w, l);
                paint_menu_rule(
                    hwnd,
                    if message == WM_PRINT {
                        w as HDC
                    } else {
                        null_mut()
                    },
                );
                return result;
            }
            WM_CLOSE => {
                queue(Event::Close);
                return 0;
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                return 0;
            }
            WM_SIZE => {
                queue(Event::Resize);
                return 0;
            }
            WM_DPICHANGED => {
                let r = &*(l as *const RECT);
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                queue(Event::Dpi);
                return 0;
            }
            WM_SETTINGCHANGE => queue(Event::Theme),
            WM_MENUCHAR => {
                let menu = l as HMENU;
                let key = char::from_u32((w & 0xffff) as u32)
                    .unwrap_or('\0')
                    .to_ascii_lowercase();
                for index in 0..GetMenuItemCount(menu) {
                    let mut item: MENUITEMINFOW = zeroed();
                    item.cbSize = size_of::<MENUITEMINFOW>() as u32;
                    item.fMask = MIIM_DATA;
                    if GetMenuItemInfoW(menu, index as u32, 1, &mut item) != 0 {
                        let matches = MENU_LABELS.with(|labels| {
                            labels
                                .borrow()
                                .get(item.dwItemData.wrapping_sub(1))
                                .is_some_and(|(label, _)| {
                                    label
                                        .split_once('&')
                                        .and_then(|(_, tail)| tail.chars().next())
                                        .is_some_and(|ch| ch.to_ascii_lowercase() == key)
                                })
                        });
                        if matches {
                            return index as isize | ((MNC_EXECUTE as isize) << 16);
                        }
                    }
                }
                return 0;
            }
            WM_TIMER => {
                queue(Event::Tick);
                return 0;
            }
            WM_COMMAND => {
                let code = (w >> 16) as u16;
                if code == 0 {
                    queue(Event::Command(w & 0xffff));
                }
                return 0;
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(l as *mut MINMAXINFO);
                let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                info.ptMinTrackSize = POINT {
                    x: 780 * dpi / 96,
                    y: 480 * dpi / 96,
                };
                return 0;
            }
            WM_NOTIFY => {
                let header = &*(l as *const NMHDR);
                match header.idFrom {
                    RESULTS_EDITOR_ID if header.code == SCN_DOUBLECLICK => {
                        queue(Event::ResultActivate)
                    }
                    101 | 102 => {
                        let pane = header.idFrom - 101;
                        match header.code {
                            SCN_STYLENEEDED => queue(Event::Style(pane)),
                            SCN_FOCUSIN => queue(Event::Focus(pane)),
                            SCN_UPDATEUI | SCN_ZOOM => {
                                EVENTS.with(|q| {
                                    let mut q = q.borrow_mut();
                                    if !q.iter().any(
                                        |event| matches!(event,Event::Updated(p) if *p == pane),
                                    ) {
                                        q.push_back(Event::Updated(pane));
                                    }
                                });
                            }
                            SCN_MODIFIED => {
                                let n = &*(l as *const Notification);
                                if n.modification & 3 != 0 {
                                    queue(Event::Changed(pane));
                                }
                            }
                            SCN_CHARADDED => {
                                queue(Event::Character(pane, (*(l as *const Notification)).ch))
                            }
                            _ => {}
                        }
                    }
                    TAB_ID | 304 if header.code == TCN_SELCHANGE => {
                        queue(Event::Tab(usize::from(header.idFrom == 304)))
                    }
                    TREE_ID if header.code == TVN_SELCHANGEDW && !TREE_UPDATING.with(Cell::get) => {
                        let n = &*(l as *const NMTREEVIEWW);
                        queue(Event::Tree(n.itemNew.lParam as usize));
                    }
                    _ => {}
                }
                return 0;
            }
            WM_DROPFILES => {
                let drop = w as HDROP;
                let count = DragQueryFileW(drop, u32::MAX, null_mut(), 0);
                let mut paths = Vec::new();
                for i in 0..count {
                    let len = DragQueryFileW(drop, i, null_mut(), 0);
                    let mut path = vec![0u16; len as usize + 1];
                    DragQueryFileW(drop, i, path.as_mut_ptr(), len + 1);
                    use std::os::windows::ffi::OsStringExt;
                    paths.push(PathBuf::from(std::ffi::OsString::from_wide(
                        &path[..len as usize],
                    )));
                }
                DragFinish(drop);
                queue(Event::Drop(paths));
                return 0;
            }
            WM_ERASEBKGND => {
                let mut rect: RECT = zeroed();
                GetClientRect(hwnd, &mut rect);
                PANEL_BRUSH.with(|b| {
                    FillRect(w as HDC, &rect, b.get());
                });
                let palette = COLORS.with(Cell::get);
                let border = CreateSolidBrush(palette.border);
                if palette.dark {
                    PANEL_BOUNDS.with(|bounds| {
                        for rect in bounds.borrow().iter() {
                            for edge in panel_edges(*rect, SPLIT_BOUNDS.with(Cell::get)) {
                                FillRect(w as HDC, &edge, border);
                            }
                        }
                    });
                }
                if let Some(mut divider) = SPLIT_BOUNDS.with(Cell::get) {
                    divider.left += (divider.right - divider.left) / 2;
                    divider.right = divider.left + (GetDpiForWindow(hwnd) as i32 / 96).max(1);
                    FillRect(w as HDC, &divider, border);
                }
                DeleteObject(border);
                return 1;
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => {
                let palette = COLORS.with(Cell::get);
                let field = matches!(message, WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX)
                    || matches!(GetDlgCtrlID(l as HWND), 401 | 402);
                SetTextColor(
                    w as HDC,
                    if IsWindowEnabled(l as HWND) != 0 {
                        palette.text
                    } else {
                        palette.muted
                    },
                );
                SetBkColor(
                    w as HDC,
                    if field {
                        controls::Colors::new(palette).field
                    } else {
                        palette.panel
                    },
                );
                return if field {
                    FIELD_BRUSH.with(Cell::get)
                } else {
                    PANEL_BRUSH.with(Cell::get)
                } as isize;
            }
            WM_DRAWITEM => {
                let item = &*(l as *const DRAWITEMSTRUCT);
                if item.CtlType == ODT_MENU {
                    PostMessageW(hwnd, 0x8037, 0, 0);
                }
                if item.CtlType == ODT_BUTTON
                    && let Some(button) = TOOLBAR
                        .iter()
                        .find(|button| button.command == item.CtlID as usize)
                {
                    match toolbar::draw(
                        item,
                        button.icon,
                        COLORS.with(Cell::get),
                        HOT_BUTTON.with(Cell::get) == item.hwndItem,
                    ) {
                        Ok(()) => return 1,
                        Err(error) => {
                            if !ICON_ERROR_REPORTED.with(|reported| reported.replace(true)) {
                                queue(Event::RenderError(error));
                            }
                        }
                    }
                }
                if item.CtlType == ODT_BUTTON {
                    match controls::draw_button(
                        item,
                        &window_text(item.hwndItem),
                        UI_FONT.with(Cell::get),
                        COLORS.with(Cell::get),
                        HOT_BUTTON.with(Cell::get) == item.hwndItem,
                    ) {
                        Ok(()) => return 1,
                        Err(error) => {
                            if !ICON_ERROR_REPORTED.with(|reported| reported.replace(true)) {
                                queue(Event::RenderError(error));
                            }
                        }
                    }
                }
                if item.CtlType == ODT_MENU {
                    let label = MENU_LABELS
                        .with(|labels| labels.borrow().get(item.itemData.wrapping_sub(1)).cloned());
                    if let Some((title, top)) = label {
                        let palette = COLORS.with(Cell::get);
                        let selected = item.itemState & (ODS_SELECTED | ODS_HOTLIGHT) != 0;
                        let brush = CreateSolidBrush(if selected {
                            palette.selection
                        } else {
                            palette.panel
                        });
                        FillRect(item.hDC, &item.rcItem, brush);
                        DeleteObject(brush);
                        let mut label_rect = item.rcItem;
                        if !top {
                            label_rect.left += (18 * GetDpiForWindow(hwnd) / 96) as i32;
                        }
                        paint_label(
                            item.hDC,
                            &label_rect,
                            &title,
                            item.itemState & (ODS_SELECTED | ODS_HOTLIGHT) != 0,
                            item.itemState & ODS_DISABLED != 0,
                        );
                        if item.itemState & ODS_CHECKED != 0 {
                            let scale = GetDpiForWindow(hwnd) as i32;
                            let x = item.rcItem.left + 5 * scale / 96;
                            let y = (item.rcItem.top + item.rcItem.bottom) / 2;
                            let pen = CreatePen(PS_SOLID, (2 * scale / 96).max(1), palette.text);
                            let old = SelectObject(item.hDC, pen);
                            MoveToEx(item.hDC, x, y, null_mut());
                            LineTo(item.hDC, x + 3 * scale / 96, y + 3 * scale / 96);
                            LineTo(item.hDC, x + 9 * scale / 96, y - 4 * scale / 96);
                            SelectObject(item.hDC, old);
                            DeleteObject(pen);
                        }
                    }
                    return 1;
                }
                let palette = COLORS.with(Cell::get);
                let selected = item.itemState & ODS_SELECTED != 0;
                let back = if selected {
                    palette.selection
                } else {
                    palette.panel
                };
                let brush = CreateSolidBrush(back);
                FillRect(item.hDC, &item.rcItem, brush);
                DeleteObject(brush);
                let title = if item.CtlID as usize == TAB_ID {
                    let mut buffer = [0u16; 512];
                    let mut tab: TCITEMW = zeroed();
                    tab.mask = TCIF_TEXT;
                    tab.pszText = buffer.as_mut_ptr();
                    tab.cchTextMax = buffer.len() as i32;
                    SendMessageW(
                        item.hwndItem,
                        TCM_GETITEMW,
                        item.itemID as usize,
                        (&mut tab as *mut TCITEMW) as isize,
                    );
                    String::from_utf16_lossy(
                        &buffer[..buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len())],
                    )
                } else {
                    window_text(item.hwndItem)
                };
                let old = SelectObject(item.hDC, UI_FONT.with(Cell::get));
                SetBkMode(item.hDC, TRANSPARENT as i32);
                SetTextColor(
                    item.hDC,
                    if selected {
                        palette.accent
                    } else {
                        palette.text
                    },
                );
                let mut rect = item.rcItem;
                rect.left += 10;
                rect.right -= 10;
                let title = wide(&title);
                DrawTextW(
                    item.hDC,
                    title.as_ptr(),
                    (title.len() - 1) as i32,
                    &mut rect,
                    DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
                if item.itemState & ODS_FOCUS != 0 {
                    DrawFocusRect(item.hDC, &rect);
                }
                SelectObject(item.hDC, old);
                if selected {
                    let stripe = RECT {
                        left: item.rcItem.left + 5,
                        top: item.rcItem.bottom - 3,
                        right: item.rcItem.right - 5,
                        bottom: item.rcItem.bottom,
                    };
                    let brush = CreateSolidBrush(palette.accent);
                    FillRect(item.hDC, &stripe, brush);
                    DeleteObject(brush);
                }
                return 1;
            }
            WM_MEASUREITEM => {
                let item = &mut *(l as *mut MEASUREITEMSTRUCT);
                if item.CtlType == ODT_MENU {
                    let label = MENU_LABELS
                        .with(|labels| labels.borrow().get(item.itemData.wrapping_sub(1)).cloned());
                    if let Some((label, top)) = label {
                        let dc = GetDC(hwnd);
                        let old = SelectObject(dc, UI_FONT.with(Cell::get));
                        let label = wide(&label.replace('&', "").replace('\t', "    "));
                        let mut size: SIZE = zeroed();
                        GetTextExtentPoint32W(
                            dc,
                            label.as_ptr(),
                            (label.len() - 1) as i32,
                            &mut size,
                        );
                        SelectObject(dc, old);
                        ReleaseDC(hwnd, dc);
                        let scale = GetDpiForWindow(hwnd) as i32;
                        item.itemWidth = (size.cx + if top { 24 } else { 50 } * scale / 96) as u32;
                        item.itemHeight = (if top { 24 } else { 28 } * scale / 96) as u32;
                    }
                    return 1;
                }
            }
            WM_LBUTTONDOWN => {
                if let Some(offset) = split_hit(l as i16 as i32, (l >> 16) as i16 as i32) {
                    SPLIT_DRAG_OFFSET.with(|drag| drag.set(Some(offset)));
                    SetCapture(hwnd);
                    SetCursor(LoadCursorW(null_mut(), IDC_SIZEWE));
                    return 0;
                }
            }
            WM_SETCURSOR if w == hwnd as usize && l as u16 == HTCLIENT as u16 => {
                let mut point: POINT = zeroed();
                GetCursorPos(&mut point);
                ScreenToClient(hwnd, &mut point);
                if split_hit(point.x, point.y).is_some() {
                    SetCursor(LoadCursorW(null_mut(), IDC_SIZEWE));
                    return 1;
                }
            }
            WM_MOUSEMOVE if GetCapture() == hwnd => {
                if let Some(offset) = SPLIT_DRAG_OFFSET.with(Cell::get) {
                    SetCursor(LoadCursorW(null_mut(), IDC_SIZEWE));
                    queue(Event::SplitDrag(l as i16 as i32 - offset));
                }
                return 0;
            }
            WM_LBUTTONUP if GetCapture() == hwnd => {
                if let Some(offset) = SPLIT_DRAG_OFFSET.with(|drag| drag.take()) {
                    queue(Event::SplitDrag(l as i16 as i32 - offset));
                }
                ReleaseCapture();
                return 0;
            }
            WM_CAPTURECHANGED => {
                SPLIT_DRAG_OFFSET.with(|drag| drag.set(None));
            }
            WM_CANCELMODE => {
                SPLIT_DRAG_OFFSET.with(|drag| drag.set(None));
                if GetCapture() == hwnd {
                    ReleaseCapture();
                }
            }
            _ => {}
        }
        DefWindowProcW(hwnd, message, w, l)
    }
}

unsafe extern "system" fn button_proc(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match message {
            WM_MOUSEMOVE => {
                let previous = HOT_BUTTON.with(|hot| hot.replace(hwnd));
                if previous != hwnd {
                    if !previous.is_null() {
                        InvalidateRect(previous, null(), 0);
                    }
                    InvalidateRect(hwnd, null(), 0);
                    let mut tracking = TRACKMOUSEEVENT {
                        cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut tracking);
                }
            }
            WM_MOUSELEAVE | WM_NCDESTROY => {
                if HOT_BUTTON.with(Cell::get) == hwnd {
                    HOT_BUTTON.with(|hot| hot.set(null_mut()));
                    InvalidateRect(hwnd, null(), 0);
                }
                if message == WM_NCDESTROY {
                    RemoveWindowSubclass(hwnd, Some(button_proc), 3);
                }
            }
            WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE => {
                InvalidateRect(hwnd, null(), 0);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, message, w, l)
    }
}

unsafe extern "system" fn field_proc(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        let result = DefSubclassProc(hwnd, message, w, l);
        match message {
            WM_NCPAINT | WM_PAINT => {
                if let Err(error) = controls::draw_field_border(hwnd, COLORS.with(Cell::get))
                    && !ICON_ERROR_REPORTED.with(|reported| reported.replace(true))
                {
                    queue(Event::RenderError(error));
                }
            }
            WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE | WM_THEMECHANGED => {
                RedrawWindow(hwnd, null(), null_mut(), RDW_INVALIDATE | RDW_FRAME);
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(field_proc), 6);
            }
            _ => {}
        }
        result
    }
}

unsafe extern "system" fn map_proc(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    match message {
        WM_LBUTTONDOWN | WM_MOUSEMOVE
            if message == WM_LBUTTONDOWN || w & MK_LBUTTON as usize != 0 =>
        {
            queue(Event::Map(((l >> 16) as i16) as i32));
            0
        }
        WM_CHAR | WM_KEYDOWN | WM_PASTE | WM_CUT | WM_CLEAR | WM_SETFOCUS | WM_RBUTTONDOWN
        | WM_LBUTTONDBLCLK => 0,
        _ => unsafe { DefSubclassProc(hwnd, message, w, l) },
    }
}

unsafe extern "system" fn results_divider_proc(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match message {
            WM_SETCURSOR => {
                SetCursor(LoadCursorW(null_mut(), IDC_SIZENS));
                return 1;
            }
            WM_LBUTTONDOWN => {
                SetCapture(hwnd);
                return 0;
            }
            WM_MOUSEMOVE if GetCapture() == hwnd => {
                let mut point = POINT {
                    x: l as i16 as i32,
                    y: (l >> 16) as i16 as i32,
                };
                MapWindowPoints(hwnd, GetParent(hwnd), &mut point, 1);
                queue(Event::ResultsResize(point.y));
                return 0;
            }
            WM_LBUTTONUP if GetCapture() == hwnd => {
                ReleaseCapture();
                return 0;
            }
            WM_NCDESTROY => {
                RemoveWindowSubclass(hwnd, Some(results_divider_proc), 5);
            }
            _ => {}
        }
        DefSubclassProc(hwnd, message, w, l)
    }
}
unsafe extern "system" fn tab_proc(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        let pane = usize::from(GetDlgCtrlID(hwnd) == 304);
        match message {
            WM_NCHITTEST => {
                let mut point = POINT {
                    x: l as i16 as i32,
                    y: (l >> 16) as i16 as i32,
                };
                if ScreenToClient(hwnd, &mut point) != 0 && empty_tab_space(hwnd, point.x, point.y)
                {
                    // Native tabs otherwise let real mouse input pass through the empty strip.
                    return HTCLIENT as isize;
                }
            }
            WM_LBUTTONDBLCLK if empty_tab_space(hwnd, l as i16 as i32, (l >> 16) as i16 as i32) => {
                TAB_DRAG.with(|drag| drag.set(None));
                queue(Event::TabFocus(pane));
                queue(Event::Command(NEW));
                return 0;
            }
            WM_RBUTTONDOWN => {
                return 0;
            }
            WM_RBUTTONUP => {
                let mut hit = TCHITTESTINFO {
                    pt: POINT {
                        x: l as i16 as i32,
                        y: (l >> 16) as i16 as i32,
                    },
                    flags: 0,
                };
                let index = SendMessageW(
                    hwnd,
                    TCM_HITTEST,
                    0,
                    (&mut hit as *mut TCHITTESTINFO) as isize,
                );
                if index >= 0 {
                    let active_tabs = GetDlgItem(
                        GetParent(hwnd),
                        if ACTIVE_PANE.with(Cell::get) == 0 {
                            TAB_ID as i32
                        } else {
                            304
                        },
                    );
                    let current = tab_document_id(
                        active_tabs,
                        SendMessageW(active_tabs, TCM_GETCURSEL, 0, 0) as usize,
                    );
                    queue(Event::TabFocus(pane));
                    let id = tab_document_id(hwnd, index as usize);
                    let menu = CreatePopupMenu();
                    AppendMenuW(menu, MF_STRING, TAB_PIN, wide("Pin / unpin tab").as_ptr());
                    AppendMenuW(menu, MF_STRING, TAB_LEFT, wide("Move tab left").as_ptr());
                    AppendMenuW(menu, MF_STRING, TAB_RIGHT, wide("Move tab right").as_ptr());
                    AppendMenuW(menu, MF_SEPARATOR, 0, null());
                    AppendMenuW(
                        menu,
                        MF_STRING,
                        TAB_SPLIT,
                        wide("Open in split view").as_ptr(),
                    );
                    AppendMenuW(
                        menu,
                        MF_STRING,
                        TAB_CLONE,
                        wide("Clone to other pane").as_ptr(),
                    );
                    AppendMenuW(
                        menu,
                        MF_STRING | if current == id { MF_GRAYED } else { 0 },
                        TAB_COMPARE,
                        wide("Compare with current view").as_ptr(),
                    );
                    ClientToScreen(hwnd, &mut hit.pt);
                    let action = TrackPopupMenu(
                        menu,
                        TPM_RETURNCMD | TPM_RIGHTBUTTON,
                        hit.pt.x,
                        hit.pt.y,
                        0,
                        GetParent(hwnd),
                        null(),
                    );
                    DestroyMenu(menu);
                    if action as usize == TAB_PIN {
                        queue(Event::PinTab(id));
                    } else if action as usize == TAB_LEFT {
                        queue(Event::MoveTab(id, (index as usize).saturating_sub(1)));
                    } else if action as usize == TAB_RIGHT {
                        queue(Event::MoveTab(
                            id,
                            (index as usize + 1)
                                .min(SendMessageW(hwnd, TCM_GETITEMCOUNT, 0, 0) as usize - 1),
                        ));
                    } else if action as usize == TAB_SPLIT {
                        queue(Event::SplitTab(id));
                    } else if action as usize == TAB_CLONE {
                        queue(Event::CloneTab(id));
                    } else if action as usize == TAB_COMPARE {
                        queue(Event::CompareTabs(current, id));
                    }
                    return 0;
                }
            }
            WM_MOUSEMOVE if GetCapture() == hwnd => {
                if let Some(TabDrag {
                    id,
                    x,
                    y,
                    target,
                    dragging,
                }) = TAB_DRAG.with(Cell::get)
                {
                    let point = POINT {
                        x: l as i16 as i32,
                        y: (l >> 16) as i16 as i32,
                    };
                    let dragging = dragging || (point.x - x).abs() > 4 || (point.y - y).abs() > 4;
                    let mut hit = TCHITTESTINFO {
                        pt: point,
                        flags: 0,
                    };
                    let index = SendMessageW(
                        hwnd,
                        TCM_HITTEST,
                        0,
                        (&mut hit as *mut TCHITTESTINFO) as isize,
                    );
                    let target = if index >= 0 { index as usize } else { target };
                    TAB_DRAG.with(|drag| {
                        drag.set(Some(TabDrag {
                            id,
                            x,
                            y,
                            target,
                            dragging,
                        }))
                    });
                    if dragging {
                        SetCursor(LoadCursorW(null_mut(), IDC_SIZEWE));
                        return 0;
                    }
                }
            }
            WM_LBUTTONUP => {
                if let Some(TabDrag {
                    id,
                    target,
                    dragging: true,
                    ..
                }) = TAB_DRAG.with(|drag| drag.replace(None))
                {
                    ReleaseCapture();
                    queue(Event::MoveTab(id, target));
                    return 0;
                }
                if GetCapture() == hwnd {
                    ReleaseCapture();
                }
            }
            WM_CAPTURECHANGED | WM_CANCELMODE => {
                TAB_DRAG.with(|drag| drag.set(None));
            }
            WM_ERASEBKGND => return 1,
            WM_PAINT => {
                let mut paint: PAINTSTRUCT = zeroed();
                let dc = BeginPaint(hwnd, &mut paint);
                let mut client: RECT = zeroed();
                GetClientRect(hwnd, &mut client);
                PANEL_BRUSH.with(|brush| {
                    FillRect(dc, &client, brush.get());
                });
                let count = SendMessageW(hwnd, TCM_GETITEMCOUNT, 0, 0);
                let selected = SendMessageW(hwnd, TCM_GETCURSEL, 0, 0);
                for index in 0..count {
                    let mut rect: RECT = zeroed();
                    SendMessageW(
                        hwnd,
                        TCM_GETITEMRECT,
                        index as usize,
                        (&mut rect as *mut RECT) as isize,
                    );
                    let mut text = [0u16; 512];
                    let mut item: TCITEMW = zeroed();
                    item.mask = TCIF_TEXT;
                    item.pszText = text.as_mut_ptr();
                    item.cchTextMax = 512;
                    SendMessageW(
                        hwnd,
                        TCM_GETITEMW,
                        index as usize,
                        (&mut item as *mut TCITEMW) as isize,
                    );
                    let end = text.iter().position(|c| *c == 0).unwrap_or(text.len());
                    let mut label = rect;
                    label.right -= 14;
                    paint_label(dc, &rect, "", index == selected, false);
                    let old = SelectObject(dc, UI_FONT.with(Cell::get));
                    label.left += 12;
                    label.right -= 12;
                    DrawTextW(
                        dc,
                        text.as_ptr(),
                        end as i32,
                        &mut label,
                        DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
                    );
                    let mut close = rect;
                    close.left = close.right - 23;
                    let cross = wide("x");
                    DrawTextW(
                        dc,
                        cross.as_ptr(),
                        1,
                        &mut close,
                        DT_SINGLELINE | DT_VCENTER | DT_CENTER,
                    );
                    SelectObject(dc, old);
                    if index == selected && pane == ACTIVE_PANE.with(Cell::get) {
                        let stripe = RECT {
                            left: rect.left,
                            top: rect.bottom - 2,
                            right: rect.right,
                            bottom: rect.bottom,
                        };
                        let brush = CreateSolidBrush(COLORS.with(Cell::get).accent);
                        FillRect(dc, &stripe, brush);
                        DeleteObject(brush);
                    }
                }
                EndPaint(hwnd, &paint);
                return 0;
            }
            WM_LBUTTONDOWN | WM_MBUTTONUP => {
                let x = (l as i16) as i32;
                let mut hit = TCHITTESTINFO {
                    pt: POINT {
                        x,
                        y: ((l >> 16) as i16) as i32,
                    },
                    flags: 0,
                };
                let index = SendMessageW(
                    hwnd,
                    TCM_HITTEST,
                    0,
                    (&mut hit as *mut TCHITTESTINFO) as isize,
                );
                if index >= 0 {
                    let mut rect: RECT = zeroed();
                    SendMessageW(
                        hwnd,
                        TCM_GETITEMRECT,
                        index as usize,
                        (&mut rect as *mut RECT) as isize,
                    );
                    if message == WM_MBUTTONUP || x >= rect.right - 23 {
                        queue(Event::CloseTab(pane, tab_document_id(hwnd, index as usize)));
                        return 0;
                    }
                    if message == WM_LBUTTONDOWN {
                        queue(Event::TabFocus(pane));
                        TAB_DRAG.with(|drag| {
                            drag.set(Some(TabDrag {
                                id: tab_document_id(hwnd, index as usize),
                                x,
                                y: hit.pt.y,
                                target: index as usize,
                                dragging: false,
                            }))
                        });
                        SetCapture(hwnd);
                    }
                }
            }
            _ => {}
        }
        DefSubclassProc(hwnd, message, w, l)
    }
}
unsafe fn empty_tab_space(hwnd: HWND, x: i32, y: i32) -> bool {
    unsafe {
        let mut client: RECT = zeroed();
        if GetClientRect(hwnd, &mut client) == 0
            || x < 0
            || x >= client.right
            || y < 0
            || y >= client.bottom
        {
            return false;
        }
        let count = SendMessageW(hwnd, TCM_GETITEMCOUNT, 0, 0);
        let mut last: RECT = zeroed();
        if count > 0
            && SendMessageW(
                hwnd,
                TCM_GETITEMRECT,
                (count - 1) as usize,
                (&mut last as *mut RECT) as isize,
            ) == 0
        {
            return false;
        }
        x >= last.right
    }
}
unsafe fn tab_document_id(hwnd: HWND, index: usize) -> u64 {
    unsafe {
        let mut item: TCITEMW = zeroed();
        item.mask = TCIF_PARAM;
        SendMessageW(
            hwnd,
            TCM_GETITEMW,
            index,
            (&mut item as *mut TCITEMW) as isize,
        );
        item.lParam as u64
    }
}

fn system_dark() -> bool {
    let mut value: u32 = 1;
    let mut size = 4u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize").as_ptr(),
            wide("AppsUseLightTheme").as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        );
    }
    value == 0
}

fn encoding_options() -> Vec<Encoding> {
    core::encoding_options()
}

/// `fs::canonicalize` fails when the target does not exist yet, so fall back to
/// canonicalizing the parent. Save-as must not be able to alias a path that is
/// already open in another tab.
fn canonical_target(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => fs::canonicalize(parent)
            .map(|parent| parent.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

fn clipboard_text(owner: HWND) -> Result<String> {
    unsafe {
        if OpenClipboard(owner) == 0 {
            return Err("Could not open the clipboard.".into());
        }
        let result = (|| {
            let handle = GetClipboardData(13);
            if handle.is_null() {
                return Err("The clipboard does not contain Unicode text.".into());
            }
            let bytes = GlobalSize(handle);
            if !(2..=core::MAX_TOOL_BYTES * 2 + 2).contains(&bytes) {
                return Err("Clipboard text exceeds the comparison limit or is invalid.".into());
            }
            let pointer = GlobalLock(handle).cast::<u16>();
            if pointer.is_null() {
                return Err("Could not read clipboard text.".into());
            }
            let data = std::slice::from_raw_parts(pointer, bytes / 2);
            let text = match data.iter().position(|&ch| ch == 0) {
                Some(length) => String::from_utf16(&data[..length]).map_err(|e| e.to_string()),
                None => Err("Clipboard text is not terminated.".into()),
            };
            GlobalUnlock(handle);
            text
        })();
        CloseClipboard();
        result
    }
}
struct Document {
    handle: DocumentHandle,
    snapshot: DocumentSnapshot,
    language: usize,
    base_dirty: bool,
    metadata_dirty: bool,
    revision: u64,
    styled_revision: Option<u64>,
    last_edit: Instant,
}

struct SearchBar {
    labels: [HWND; 3],
    query: HWND,
    replace: HWND,
    mode: HWND,
    case: HWND,
    word: HWND,
    buttons: Vec<HWND>,
    visible: bool,
    directory: HWND,
    filters: HWND,
    recursive: HWND,
    hidden: HWND,
    folder_labels: [HWND; 2],
    folder_buttons: Vec<HWND>,
    folder_visible: bool,
}

struct SearchTask {
    rx: mpsc::Receiver<Result<SearchResults>>,
    cancelled: Arc<AtomicBool>,
    discard: bool,
}
impl Drop for SearchTask {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
struct ResultPanel {
    editor: Editor,
    title: HWND,
    divider: HWND,
    buttons: Vec<HWND>,
    visible: bool,
    height: i32,
    data: Option<SearchResults>,
    links: Vec<ResultLink>,
    current: Option<usize>,
    job: Option<SearchTask>,
}
impl ResultPanel {
    fn new(editor: Editor) -> Self {
        Self {
            editor,
            title: null_mut(),
            divider: null_mut(),
            buttons: Vec::new(),
            visible: false,
            height: 230,
            data: None,
            links: Vec::new(),
            current: None,
            job: None,
        }
    }
}

struct CompareResult {
    left: u64,
    right: u64,
    left_rev: u64,
    right_rev: u64,
    generation: u64,
    result: Result<Comparison>,
}
struct JsonResult {
    document: u64,
    revision: u64,
    result: Result<Vec<JsonNode>>,
}
struct HighlightResult {
    document: u64,
    revision: u64,
    language: String,
    result: Result<Highlight>,
}

struct App {
    hwnd: HWND,
    instance: HINSTANCE,
    editors: [Editor; 2],
    scratch: Editor,
    map: Editor,
    tabs: HWND,
    right_tabs: HWND,
    groups: [Vec<u64>; 2],
    status: HWND,
    tools: Vec<HWND>,
    tooltips: Option<Tooltips>,
    search: SearchBar,
    results: ResultPanel,
    tree: HWND,
    documents: Vec<Document>,
    languages: Vec<Language>,
    completion_api: Vec<Api>,
    primary: usize,
    secondary: Option<usize>,
    focused: usize,
    next_id: u64,
    palette: Palette,
    theme: String,
    editor_font: EditorFont,
    show_symbols: ShowSymbols,
    monitor_files: bool,
    monitor: Monitor,
    monitor_busy: bool,
    monitor_last: Instant,
    compare_options: CompareOptions,
    compare_generation: u64,
    compare_ranges: Option<[(std::ops::Range<usize>, usize); 2]>,
    compare_leading: [usize; 2],
    compare_top: [usize; 2],
    font: HFONT,
    dpi: u32,
    map_visible: bool,
    tree_visible: bool,
    wrap: bool,
    ratio: f32,
    json_nodes: Vec<JsonNode>,
    json_document: Option<u64>,
    json_handles: Vec<HTREEITEM>,
    json_due: Option<Instant>,
    json_rx: Option<mpsc::Receiver<JsonResult>>,
    highlight_rx: Option<mpsc::Receiver<HighlightResult>>,
    differences: Vec<Difference>,
    difference: usize,
    comparing: bool,
    compare_rx: Option<mpsc::Receiver<CompareResult>>,
    compare_due: Option<Instant>,
    compare_jump: bool,
    revision: u64,
    recovered_revision: u64,
    recovery_busy: bool,
    recovery: RecoveryWorker,
    last_autosave: Instant,
    recovery_error: Option<String>,
    note: String,
    last_zero_match: Option<(u64, usize, String)>,
    exiting: bool,
    _lock: File,
}

impl App {
    fn control(&self, class: &str, title: &str, style: u32, id: usize) -> Result<HWND> {
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(title).as_ptr(),
                WS_CHILD | style,
                0,
                0,
                1,
                1,
                self.hwnd,
                id as _,
                self.instance,
                null_mut(),
            )
        };
        if hwnd.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        unsafe {
            SendMessageW(hwnd, WM_SETFONT, self.font as usize, 1);
        }
        Ok(hwnd)
    }
    fn button(&self, title: &str, id: usize) -> Result<HWND> {
        let button = self.control(
            "BUTTON",
            title,
            WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            id,
        )?;
        if unsafe { SetWindowSubclass(button, Some(button_proc), 3, 0) } == 0 {
            return Err("Could not initialize button interaction.".into());
        }
        Ok(button)
    }
    fn search_field(&self, id: usize) -> Result<HWND> {
        let field = self.control(
            "EDIT",
            "",
            WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
            id,
        )?;
        if unsafe { SetWindowSubclass(field, Some(field_proc), 6, 0) } == 0 {
            return Err("Could not initialize the search-field border.".into());
        }
        Ok(field)
    }
    fn index(&self) -> usize {
        if self.focused == 1 {
            self.secondary.unwrap_or(self.primary)
        } else {
            self.primary
        }
    }
    fn editor(&self) -> Editor {
        self.editors[self.focused]
    }
    fn scale(&self, value: i32) -> i32 {
        value * self.dpi as i32 / 96
    }
    fn touch(&mut self) {
        self.revision += 1;
    }
    fn note(&mut self, text: impl Into<String>) {
        self.note = text.into();
        self.update_status();
    }
    fn create_controls(&mut self) -> Result<()> {
        self.tabs = self.control(
            "SysTabControl32",
            "",
            WS_VISIBLE | TCS_OWNERDRAWFIXED | TCS_FIXEDWIDTH | TCS_FOCUSNEVER,
            TAB_ID,
        )?;
        self.right_tabs = self.control(
            "SysTabControl32",
            "",
            TCS_OWNERDRAWFIXED | TCS_FIXEDWIDTH | TCS_FOCUSNEVER,
            304,
        )?;
        unsafe {
            SetWindowSubclass(self.tabs, Some(tab_proc), 2, 0);
            SetWindowSubclass(self.right_tabs, Some(tab_proc), 2, 0);
            SendMessageW(
                self.right_tabs,
                TCM_SETITEMSIZE,
                0,
                ((self.scale(34) as isize) << 16) | self.scale(190) as isize,
            );
        }
        unsafe {
            SendMessageW(
                self.tabs,
                TCM_SETITEMSIZE,
                0,
                ((self.scale(34) as isize) << 16) | self.scale(190) as isize,
            );
        }
        self.status = self.control("STATIC", "", WS_VISIBLE | SS_CENTERIMAGE, 303)?;
        self.tree = self.control(
            "SysTreeView32",
            "",
            WS_TABSTOP | TVS_HASLINES | TVS_LINESATROOT | TVS_HASBUTTONS | TVS_SHOWSELALWAYS,
            TREE_ID,
        )?;
        let mut tooltips = unsafe { Tooltips::new(self.hwnd)? };
        for button in &TOOLBAR {
            let control = self.button(button.name, button.command)?;
            unsafe {
                tooltips.add(control, button.tooltip)?;
            }
            self.tools.push(control);
        }
        self.tooltips = Some(tooltips);
        self.search.labels[0] = self.control("STATIC", "Find:", SS_CENTERIMAGE, 406)?;
        self.search.query = self.search_field(401)?;
        self.search.labels[1] = self.control("STATIC", "Replace:", SS_CENTERIMAGE, 407)?;
        self.search.replace = self.search_field(402)?;
        self.search.labels[2] = self.control("STATIC", "Mode:", SS_CENTERIMAGE, 408)?;
        self.search.mode =
            self.control("COMBOBOX", "", WS_TABSTOP | CBS_DROPDOWNLIST as u32, 403)?;
        for name in ["Normal", "Extended (\\n, \\t)", "Regex ($1 captures)"] {
            unsafe {
                SendMessageW(
                    self.search.mode,
                    CB_ADDSTRING,
                    0,
                    wide(name).as_ptr() as isize,
                );
            }
        }
        unsafe {
            SendMessageW(self.search.mode, CB_SETCURSEL, 0, 0);
            SendMessageW(
                self.search.query,
                EM_SETCUEBANNER,
                0,
                wide("Find text or expression").as_ptr() as isize,
            );
            SendMessageW(
                self.search.replace,
                EM_SETCUEBANNER,
                0,
                wide("Replace with").as_ptr() as isize,
            );
            SendMessageW(self.search.query, EM_SETLIMITTEXT, 32768, 0);
            SendMessageW(self.search.replace, EM_SETLIMITTEXT, 32768, 0);
        }
        self.set_search_margins();
        self.search.case = self.control(
            "BUTTON",
            "Match case",
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
            404,
        )?;
        self.search.word = self.control(
            "BUTTON",
            "Whole word",
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
            405,
        )?;
        self.search.buttons = [
            ("Next", FIND_NEXT),
            ("Previous", FIND_PREVIOUS),
            ("Replace", REPLACE),
            ("Replace all", REPLACE_ALL),
            ("Close", SEARCH_CLOSE),
            ("Find all: current document", FIND_ALL_CURRENT),
            ("Find all: all open documents", FIND_ALL_OPEN),
            ("Find in files...", FIND_FILES),
        ]
        .into_iter()
        .map(|(label, id)| self.button(label, id))
        .collect::<Result<_>>()?;
        self.search.directory = self.search_field(410)?;
        self.search.filters = self.search_field(411)?;
        set_text(self.search.filters, "*");
        self.search.recursive = self.control(
            "BUTTON",
            "Subfolders",
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
            412,
        )?;
        self.search.hidden = self.control(
            "BUTTON",
            "Hidden files",
            BS_AUTOCHECKBOX as u32 | WS_TABSTOP,
            413,
        )?;
        unsafe {
            SendMessageW(self.search.recursive, BM_SETCHECK, BST_CHECKED as usize, 0);
            SendMessageW(self.search.directory, EM_SETLIMITTEXT, 32768, 0);
            SendMessageW(self.search.filters, EM_SETLIMITTEXT, 8192, 0);
        }
        self.search.folder_labels = [
            self.control("STATIC", "Folder:", SS_CENTERIMAGE, 414)?,
            self.control("STATIC", "Filters:", SS_CENTERIMAGE, 415)?,
        ];
        self.search.folder_buttons = vec![
            self.button("Browse...", FIND_FILES_BROWSE)?,
            self.button("Search folder", FIND_FILES_RUN)?,
        ];
        self.results.title =
            self.control("STATIC", "Search results", WS_VISIBLE | SS_CENTERIMAGE, 305)?;
        self.results.divider =
            self.control("STATIC", "", WS_VISIBLE | SS_NOTIFY, RESULTS_DIVIDER_ID)?;
        unsafe {
            if SetWindowSubclass(self.results.divider, Some(results_divider_proc), 5, 0) == 0 {
                return Err("Could not create the search-results resize divider.".into());
            }
        }
        self.results.buttons = [
            ("Previous", RESULTS_PREVIOUS),
            ("Next", RESULTS_NEXT),
            ("Cancel", RESULTS_CANCEL),
            ("Clear", RESULTS_CLEAR),
            ("Close", RESULTS_CLOSE),
        ]
        .into_iter()
        .map(|(label, id)| self.button(label, id))
        .collect::<Result<_>>()?;
        self.results.editor.set_read_only_text("Use Find All to list matches in the current document or all open tabs.\nDouble-click a result or press Enter to navigate. F4 / Shift+F4: next / previous match.")?;
        unsafe {
            EnableWindow(self.results.buttons[2], 0);
        }
        self.make_menu()?;
        Ok(())
    }
    fn make_menu(&self) -> Result<()> {
        unsafe {
            let previous = GetMenu(self.hwnd);
            if !previous.is_null() {
                SetMenu(self.hwnd, null_mut());
                DestroyMenu(previous);
            }
            MENUS.with(|menus| menus.borrow_mut().clear());
            MENU_LABELS.with(|labels| labels.borrow_mut().clear());
            let bar = new_menu(false);
            if bar.is_null() {
                return Err("Could not create application menu.".into());
            }
            let menus: &[(&str, &[(usize, &str)])] = &[
                (
                    "&File",
                    &[
                        (NEW, "&New\tCtrl+N"),
                        (OPEN, "&Open...\tCtrl+O"),
                        (SAVE, "&Save\tCtrl+S"),
                        (SAVE_AS, "Save &as...\tCtrl+Shift+S"),
                        (CLOSE, "&Close tab\tCtrl+W"),
                        (TAB_PIN, "Pin / unpin current tab"),
                        (TAB_LEFT, "Move tab left"),
                        (TAB_RIGHT, "Move tab right"),
                        (0, ""),
                        (EXIT, "E&xit (keep session)"),
                    ],
                ),
                (
                    "&Edit",
                    &[
                        (UNDO, "&Undo\tCtrl+Z"),
                        (REDO, "&Redo\tCtrl+Y"),
                        (0, ""),
                        (CUT, "Cut\tCtrl+X"),
                        (COPY, "Copy\tCtrl+C"),
                        (PASTE, "Paste\tCtrl+V"),
                        (SELECT_ALL, "Select all\tCtrl+A"),
                        (0, ""),
                        (ADD_NEXT, "Add next occurrence\tCtrl+D"),
                        (SELECT_MATCHES, "Select all occurrences\tCtrl+Shift+L"),
                        (PARAMETER_HINT, "Function parameters\tCtrl+Shift+Space"),
                    ],
                ),
                (
                    "&Search",
                    &[
                        (FIND, "Find / replace\tCtrl+F / Ctrl+H"),
                        (FIND_NEXT, "Find next\tF3"),
                        (FIND_PREVIOUS, "Find previous\tShift+F3"),
                        (REPLACE_ALL, "Replace all"),
                        (
                            FIND_ALL_CURRENT,
                            "Find all in current document\tCtrl+Alt+Enter",
                        ),
                        (
                            FIND_ALL_OPEN,
                            "Find all in all open documents\tCtrl+Shift+Enter",
                        ),
                        (FIND_FILES, "Find in files...\tCtrl+Shift+F"),
                        (0, ""),
                        (RESULTS_TOGGLE, "Search results panel\tCtrl+Alt+R"),
                        (RESULTS_NEXT, "Next search result\tF4"),
                        (RESULTS_PREVIOUS, "Previous search result\tShift+F4"),
                    ],
                ),
                (
                    "&View",
                    &[
                        (SPLIT, "Toggle split view\tCtrl+Alt+Right"),
                        (MAP, "Document map"),
                        (WRAP, "Word wrap"),
                        (EDITOR_FONT, "Editor &font..."),
                        (MONITOR_FILES, "Automatically reload external changes"),
                        (ZOOM_RESET, "Reset zoom"),
                        (0, ""),
                        (THEME_SYSTEM, "Theme: Windows default"),
                        (THEME_LIGHT, "Theme: light"),
                        (THEME_DARK, "Theme: dark"),
                    ],
                ),
                (
                    "&Tools",
                    &[
                        (COMPARE, "Compare selected panes (or next tab)"),
                        (COMPARE_SELECTION, "Compare selected lines in both panes"),
                        (COMPARE_CLIPBOARD, "Compare with clipboard"),
                        (COMPARE_SAVED, "Compare with last saved file"),
                        (DIFF_NEXT, "Next difference\tF7"),
                        (DIFF_PREVIOUS, "Previous difference\tShift+F7"),
                        (COMPARE_CLEAR, "Clear compare"),
                        (0, ""),
                        (JSON_FORMAT, "Format JSON / JSON5\tCtrl+Alt+J"),
                        (JSON_COMPACT, "Minify JSON / JSON5"),
                        (JSON_TREE, "Toggle live JSON tree\tCtrl+Alt+T"),
                        (JSON_REFRESH, "Refresh JSON tree"),
                    ],
                ),
            ];
            for (label, items) in menus {
                let menu = new_menu(true);
                for (id, text) in *items {
                    menu_item(menu, *id, text, false, false);
                }
                menu_item(bar, menu as usize, label, true, true);
            }
            let view = GetSubMenu(bar, 3);
            let compare_menu = new_menu(true);
            for (id, label) in [
                (COMPARE_IGNORE_SPACE, "Ignore whitespace"),
                (COMPARE_IGNORE_CASE, "Ignore case"),
                (COMPARE_IGNORE_EMPTY, "Ignore empty lines"),
                (COMPARE_MOVES, "Detect moved lines"),
                (COMPARE_ALIGN, "Align panes"),
                (COMPARE_REGEX, "Use current Find expression as ignore regex"),
                (COMPARE_REGEX_CLEAR, "Clear ignore regex"),
            ] {
                menu_item(compare_menu, id, label, false, false);
            }
            menu_item(
                GetSubMenu(bar, 4),
                compare_menu as usize,
                "Comparison options",
                true,
                false,
            );
            let symbols = new_menu(true);
            for (id, label) in [
                (SYMBOL_SPACE, "Show space and tab"),
                (SYMBOL_EOL, "Show end of line"),
                (SYMBOL_NONPRINTING, "Show non-printing characters"),
                (SYMBOL_CONTROLS, "Show control characters && Unicode EOL"),
                (SYMBOL_ALL, "Show all characters"),
                (0, ""),
                (SYMBOL_INDENT, "Show indent guide"),
                (SYMBOL_WRAP, "Show wrap symbol"),
            ] {
                menu_item(symbols, id, label, false, false);
            }
            menu_item(view, symbols as usize, "Show &symbols", true, false);
            let edit = GetSubMenu(bar, 1);
            let cases = new_menu(true);
            for (id, label) in [
                (UPPER, "UPPERCASE\tCtrl+Shift+U"),
                (LOWER, "lowercase\tCtrl+U"),
                (TITLE_CASE, "Title Case"),
                (SENTENCE_CASE, "Sentence case"),
                (INVERT_CASE, "Invert case"),
            ] {
                menu_item(cases, id, label, false, false);
            }
            menu_item(edit, cases as usize, "Case conversion", true, false);
            let lines = new_menu(true);
            for (id, label) in [
                (DUPLICATE, "Duplicate selection / line"),
                (DELETE_LINE, "Delete line"),
                (SORT, "Sort ascending"),
                (SORT_DESC, "Sort descending"),
                (SORT_IGNORE_CASE, "Sort ascending, ignore case"),
                (SORT_DESC_IGNORE_CASE, "Sort descending, ignore case"),
                (SORT_NATURAL, "Natural sort (file2 before file10)"),
                (SORT_NUMERIC, "Numeric sort, decimal point"),
                (SORT_NUMERIC_DESC, "Numeric sort descending"),
                (SORT_NUMERIC_COMMA, "Numeric sort, decimal comma"),
                (REVERSE_LINES, "Reverse line order"),
                (UNIQUE, "Remove duplicate lines"),
                (UNIQUE_ADJACENT, "Remove consecutive duplicate lines"),
                (TRIM, "Trim trailing whitespace"),
                (TRIM_START, "Trim leading whitespace"),
                (TRIM_BOTH, "Trim leading and trailing whitespace"),
                (REMOVE_EMPTY, "Remove empty / whitespace-only lines"),
                (REMOVE_EMPTY_ONLY, "Remove empty lines only"),
                (JOIN_LINES, "Join lines"),
            ] {
                menu_item(lines, id, label, false, false);
            }
            menu_item(edit, lines as usize, "Line operations", true, false);
            let languages = new_menu(true);
            if let Some(index) = self
                .languages
                .iter()
                .position(|language| language.name == "Plain text" && language.custom.is_none())
            {
                menu_item(languages, LANGUAGE_BASE + index, "Plain text", false, false);
            }
            menu_item(languages, 0, "", false, false);
            for group in languages::menu_groups(&self.languages) {
                let menu = new_menu(true);
                for (position, index) in group.indices.into_iter().enumerate() {
                    let language = &self.languages[index];
                    menu_item(
                        menu,
                        LANGUAGE_BASE + index,
                        &language.name.replace('&', "&&"),
                        false,
                        false,
                    );
                    if position > 0 && position.is_multiple_of(22) {
                        let info = MENUITEMINFOW {
                            cbSize: size_of::<MENUITEMINFOW>() as u32,
                            fMask: MIIM_FTYPE,
                            fType: MFT_OWNERDRAW | MFT_MENUBARBREAK,
                            ..zeroed()
                        };
                        if SetMenuItemInfoW(menu, position as u32, 1, &info) == 0 {
                            return Err("Could not lay out the language menu.".into());
                        }
                    }
                }
                menu_item(languages, menu as usize, group.label, true, false);
            }
            menu_item(languages, 0, "", false, false);
            menu_item(
                languages,
                IMPORT_LANGUAGE,
                "Import user-defined language XML...",
                false,
                false,
            );
            menu_item(
                languages,
                IMPORT_API,
                "Import completion API for current language...",
                false,
                false,
            );
            menu_item(
                languages,
                REMOVE_LANGUAGE,
                "Remove current user-defined language",
                false,
                false,
            );
            menu_item(bar, languages as usize, "&Language", true, true);
            let encoding = new_menu(true);
            let reopen = new_menu(true);
            for (group, encodings) in encoding_options().chunks(12).enumerate() {
                let convert_group = new_menu(true);
                let reopen_group = new_menu(true);
                for (offset, encoding) in encodings.iter().enumerate() {
                    menu_item(
                        convert_group,
                        ENCODING_BASE + group * 12 + offset,
                        encoding.label(),
                        false,
                        false,
                    );
                    menu_item(
                        reopen_group,
                        REOPEN_BASE + group * 12 + offset,
                        encoding.label(),
                        false,
                        false,
                    );
                }
                let label = format!(
                    "{} - {}",
                    encodings.first().unwrap().label(),
                    encodings.last().unwrap().label()
                );
                menu_item(
                    encoding,
                    convert_group as usize,
                    &format!("Convert: {label}"),
                    true,
                    false,
                );
                menu_item(reopen, reopen_group as usize, &label, true, false);
            }
            AppendMenuW(encoding, MF_SEPARATOR, 0, null());
            menu_item(
                encoding,
                reopen as usize,
                "Reopen using encoding (discard edits)...",
                true,
                false,
            );
            AppendMenuW(encoding, MF_SEPARATOR, 0, null());
            for (id, label) in [
                (EOL_CRLF, "Line endings: Windows (CRLF)"),
                (EOL_LF, "Line endings: Unix (LF)"),
                (EOL_CR, "Line endings: Macintosh (CR)"),
            ] {
                menu_item(encoding, id, label, false, false);
            }
            menu_item(bar, encoding as usize, "E&ncoding", true, true);
            let help = new_menu(true);
            menu_item(help, ABOUT, "&About / keyboard help", false, false);
            menu_item(bar, help as usize, "&Help", true, true);
            SetMenu(self.hwnd, bar);
        }
        self.update_symbol_checks();
        Ok(())
    }
    fn update_symbol_checks(&self) {
        unsafe {
            let menu = GetMenu(self.hwnd);
            for (id, checked) in [
                (SYMBOL_SPACE, self.show_symbols.whitespace),
                (SYMBOL_EOL, self.show_symbols.eol),
                (SYMBOL_NONPRINTING, self.show_symbols.non_printing),
                (SYMBOL_CONTROLS, self.show_symbols.controls),
                (SYMBOL_ALL, self.show_symbols.all_characters()),
                (SYMBOL_INDENT, self.show_symbols.indent_guides),
                (SYMBOL_WRAP, self.show_symbols.wrap_markers),
                (MONITOR_FILES, self.monitor_files),
                (COMPARE_IGNORE_SPACE, self.compare_options.ignore_whitespace),
                (COMPARE_IGNORE_CASE, self.compare_options.ignore_case),
                (
                    COMPARE_IGNORE_EMPTY,
                    self.compare_options.ignore_empty_lines,
                ),
                (COMPARE_MOVES, self.compare_options.detect_moves),
                (COMPARE_ALIGN, self.compare_options.align),
            ] {
                CheckMenuItem(
                    menu,
                    id as u32,
                    MF_BYCOMMAND | if checked { MF_CHECKED } else { MF_UNCHECKED },
                );
            }
            DrawMenuBar(self.hwnd);
        }
    }
    fn apply_symbols(&self) {
        for editor in self.editors {
            editor.show_symbols(self.show_symbols, self.palette);
        }
        self.update_symbol_checks();
    }
    fn apply_theme(&mut self) {
        self.palette = Palette::new(match self.theme.as_str() {
            "dark" => true,
            "light" => false,
            _ => system_dark(),
        });
        COLORS.with(|c| c.set(self.palette));
        unsafe {
            PANEL_BRUSH.with(|b| {
                let previous = b.replace(CreateSolidBrush(self.palette.panel));
                if !previous.is_null() {
                    DeleteObject(previous);
                }
            });
            FIELD_BRUSH.with(|brush| {
                let previous =
                    brush.replace(CreateSolidBrush(controls::Colors::new(self.palette).field));
                if !previous.is_null() {
                    DeleteObject(previous);
                }
            });
            let info = MENUINFO {
                cbSize: size_of::<MENUINFO>() as u32,
                fMask: MIM_BACKGROUND,
                hbrBack: PANEL_BRUSH.with(Cell::get),
                ..zeroed()
            };
            MENUS.with(|menus| {
                for menu in menus.borrow().iter() {
                    SetMenuInfo(*menu, &info);
                }
            });
            DrawMenuBar(self.hwnd);
            let dark = self.palette.dark as i32;
            DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                (&dark as *const i32).cast(),
                4,
            );
            let theme = wide(if self.palette.dark {
                "DarkMode_Explorer"
            } else {
                "Explorer"
            });
            for control in [
                self.tree,
                self.search.query,
                self.search.replace,
                self.search.mode,
                self.search.case,
                self.search.word,
            ] {
                SetWindowTheme(control, theme.as_ptr(), null());
            }
            if let Some(tooltips) = &self.tooltips {
                SetWindowTheme(tooltips.hwnd, theme.as_ptr(), null());
                SendMessageW(
                    tooltips.hwnd,
                    TTM_SETTIPBKCOLOR,
                    self.palette.panel as usize,
                    0,
                );
                SendMessageW(
                    tooltips.hwnd,
                    TTM_SETTIPTEXTCOLOR,
                    self.palette.text as usize,
                    0,
                );
            }
            SendMessageW(
                self.tree,
                TVM_SETBKCOLOR,
                0,
                self.palette.background as isize,
            );
            SendMessageW(self.tree, TVM_SETTEXTCOLOR, 0, self.palette.text as isize);
            InvalidateRect(self.hwnd, null(), 1);
            RedrawWindow(
                self.hwnd,
                null(),
                null_mut(),
                RDW_INVALIDATE | RDW_ALLCHILDREN,
            );
            for field in [self.search.query, self.search.replace] {
                RedrawWindow(field, null(), null_mut(), RDW_INVALIDATE | RDW_FRAME);
            }
        }
        self.apply_editor_styles();
        self.theme_results();
        self.layout();
    }
    fn apply_editor_styles(&self) {
        if self.documents.is_empty() {
            return;
        }
        for (pane, index) in [
            (0, self.primary),
            (1, self.secondary.unwrap_or(self.primary)),
        ] {
            self.editors[pane].theme_with_font(
                &self.languages[self.documents[index].language],
                self.palette,
                &self.editor_font,
            );
        }
        self.apply_symbols();
        self.configure_map();
    }
    fn set_editor_font(&mut self, selection: Option<EditorFont>) {
        let Some(font) = selection else {
            return;
        };
        self.editor_font = font;
        for editor in self.editors {
            editor.send(SCI_SETZOOM, 0, 0);
        }
        self.apply_editor_styles();
        self.touch();
    }
    fn set_font(&mut self) {
        let font = unsafe {
            CreateFontW(
                -self.scale(14),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                0,
                0,
                CLEARTYPE_QUALITY as u32,
                0,
                wide("Segoe UI").as_ptr(),
            )
        };
        if font.is_null() {
            return;
        }
        let old = self.font;
        self.font = font;
        UI_FONT.with(|f| f.set(font));
        if !self.tabs.is_null() {
            let mut controls = vec![
                self.tabs,
                self.status,
                self.right_tabs,
                self.tree,
                self.search.query,
                self.search.replace,
                self.search.mode,
                self.search.case,
                self.search.word,
                self.results.title,
            ];
            controls.extend(&self.tools);
            controls.extend(self.search.labels);
            controls.extend([
                self.search.directory,
                self.search.filters,
                self.search.recursive,
                self.search.hidden,
            ]);
            controls.extend(self.search.folder_labels);
            controls.extend(&self.search.folder_buttons);
            if let Some(tooltips) = &self.tooltips {
                controls.push(tooltips.hwnd);
            }
            controls.extend(&self.search.buttons);
            controls.extend(&self.results.buttons);
            for control in controls {
                unsafe {
                    SendMessageW(control, WM_SETFONT, font as usize, 1);
                }
                self.set_search_margins();
            }
        }
        if !old.is_null() {
            unsafe {
                DeleteObject(old);
            }
        }
    }
    fn set_search_margins(&self) {
        let margin = self.scale(8).max(1) as usize;
        for field in [self.search.query, self.search.replace] {
            unsafe {
                SendMessageW(
                    field,
                    EM_SETMARGINS,
                    (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
                    (margin | (margin << 16)) as isize,
                );
            }
        }
    }
    fn layout(&self) {
        unsafe {
            let mut rect: RECT = zeroed();
            GetClientRect(self.hwnd, &mut rect);
            let (width, height) = (rect.right, rect.bottom);
            // Separate the icons from the tabs with breathing room, not another rule.
            let toolbar_h = self.scale(46);
            let tab_h = self.scale(37);
            let search_layout = SearchLayout::new(width, self.dpi);
            let search_h = if self.search.visible {
                search_layout.height
                    + self.scale(40)
                    + if self.search.folder_visible {
                        self.scale(80)
                    } else {
                        0
                    }
            } else {
                0
            };
            let status_h = self.scale(28);
            let top = toolbar_h + tab_h + search_h;
            let available_h = (height - top - status_h).max(1);
            let results_h = if self.results.visible {
                self.scale(self.results.height)
                    .min((available_h - self.scale(100)).max(0))
                    .max(0)
            } else {
                0
            };
            let body_h = (available_h - results_h).max(1);
            let tree_w = if self.tree_visible {
                self.scale(280).min(width / 3)
            } else {
                0
            };
            let map_w = if self.map_visible { self.scale(115) } else { 0 };
            let content_w = (width - tree_w - map_w).max(1);
            let gap = self.scale(10).max(1);
            let left_w = if self.secondary.is_some() {
                (content_w as f32 * self.ratio) as i32
            } else {
                content_w
            };
            SPLIT_BOUNDS.with(|bounds| {
                bounds.set(self.secondary.map(|_| RECT {
                    left: tree_w + left_w,
                    top,
                    right: tree_w + left_w + gap,
                    bottom: top + body_h,
                }));
            });
            if self.secondary.is_none() {
                SPLIT_DRAG_OFFSET.with(|drag| drag.set(None));
                if GetCapture() == self.hwnd {
                    ReleaseCapture();
                }
            }
            PANEL_BOUNDS.with(|bounds| bounds.borrow_mut().clear());
            PANE_STRIPS.with(|strips| strips.set([None, None]));
            let place_panel = |hwnd, x, y, width: i32, height: i32, pane: Option<usize>| {
                let inset = i32::from(self.palette.dark && width > 2 && height > 2);
                let top_inset = if let Some(pane) = pane {
                    let strip_h = self.scale(3).max(1).min(height.max(1));
                    PANE_STRIPS.with(|strips| {
                        let mut bounds = strips.get();
                        bounds[pane] = Some(RECT {
                            left: x,
                            top: y,
                            right: x + width,
                            bottom: y + strip_h,
                        });
                        strips.set(bounds);
                    });
                    strip_h
                } else {
                    inset
                };
                if inset != 0 {
                    PANEL_BOUNDS.with(|bounds| {
                        bounds.borrow_mut().push(RECT {
                            left: x,
                            top: y,
                            right: x + width,
                            bottom: y + height,
                        });
                    });
                }
                MoveWindow(
                    hwnd,
                    x + inset,
                    y + top_inset,
                    (width - 2 * inset).max(0),
                    (height - top_inset - inset).max(0),
                    1,
                );
            };
            for (i, button) in self.tools.iter().enumerate() {
                MoveWindow(
                    *button,
                    self.scale(
                        8 + i as i32 * 36 + if i >= 3 { 8 } else { 0 } + if i >= 4 { 8 } else { 0 },
                    ),
                    self.scale(5),
                    self.scale(32),
                    self.scale(32),
                    1,
                );
            }
            MoveWindow(self.tabs, tree_w, toolbar_h, left_w, tab_h, 1);
            MoveWindow(
                self.right_tabs,
                tree_w + left_w + gap,
                toolbar_h,
                (content_w - left_w - gap).max(1),
                tab_h,
                1,
            );
            ShowWindow(
                self.right_tabs,
                if self.secondary.is_some() {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
            MoveWindow(
                self.status,
                self.scale(10),
                height - status_h,
                width - self.scale(20),
                status_h,
                1,
            );
            place_panel(self.tree, 0, top, tree_w, body_h, None);
            ShowWindow(self.tree, if self.tree_visible { SW_SHOW } else { SW_HIDE });
            let left_top = (self.compare_top[0] as i32
                * self.editors[0].send(SCI_TEXTHEIGHT, 0, 0).max(1) as i32)
                .min(body_h - 1)
                .max(0);
            place_panel(
                self.editors[0].hwnd,
                tree_w,
                top + left_top,
                left_w,
                body_h - left_top,
                Some(0),
            );
            ShowWindow(
                self.editors[1].hwnd,
                if self.secondary.is_some() {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
            if self.secondary.is_some() {
                let right_top = (self.compare_top[1] as i32
                    * self.editors[1].send(SCI_TEXTHEIGHT, 0, 0).max(1) as i32)
                    .min(body_h - 1)
                    .max(0);
                place_panel(
                    self.editors[1].hwnd,
                    tree_w + left_w + gap,
                    top + right_top,
                    (content_w - left_w - gap).max(1),
                    body_h - right_top,
                    Some(1),
                );
            }
            place_panel(self.map.hwnd, width - map_w, top, map_w, body_h, None);
            ShowWindow(
                self.map.hwnd,
                if self.map_visible { SW_SHOWNA } else { SW_HIDE },
            );
            let panel_top = top + body_h;
            let divider_h = self.scale(5);
            let header_h = self.scale(32);
            MoveWindow(self.results.divider, 0, panel_top, width, divider_h, 1);
            MoveWindow(
                self.results.title,
                self.scale(10),
                panel_top + divider_h,
                (width - self.scale(420)).max(1),
                header_h,
                1,
            );
            for (index, button) in self.results.buttons.iter().enumerate() {
                MoveWindow(
                    *button,
                    width - self.scale(400 - index as i32 * 78),
                    panel_top + divider_h + self.scale(2),
                    self.scale(74),
                    self.scale(28),
                    1,
                );
            }
            MoveWindow(
                self.results.editor.hwnd,
                0,
                panel_top + divider_h + header_h,
                width,
                (results_h - divider_h - header_h).max(1),
                1,
            );
            for control in [
                self.results.divider,
                self.results.title,
                self.results.editor.hwnd,
            ]
            .into_iter()
            .chain(self.results.buttons.iter().copied())
            {
                ShowWindow(
                    control,
                    if self.results.visible {
                        SW_SHOWNA
                    } else {
                        SW_HIDE
                    },
                );
            }
            let y = toolbar_h + tab_h;
            let place = |hwnd, bounds: controls::Bounds| {
                MoveWindow(hwnd, bounds.x, y + bounds.y, bounds.width, bounds.height, 1);
            };
            for (hwnd, bounds) in self.search.labels.into_iter().zip(search_layout.labels) {
                place(hwnd, bounds);
            }
            for (index, (hwnd, mut bounds)) in
                [self.search.query, self.search.replace, self.search.mode]
                    .into_iter()
                    .zip(search_layout.fields)
                    .enumerate()
            {
                if index == 2 {
                    bounds.height = self.scale(180);
                }
                place(hwnd, bounds);
            }
            for (hwnd, bounds) in [self.search.case, self.search.word]
                .into_iter()
                .zip(search_layout.checks)
            {
                place(hwnd, bounds);
            }
            for (hwnd, bounds) in self.search.buttons.iter().zip(search_layout.buttons) {
                place(*hwnd, bounds);
            }
            MoveWindow(
                self.search.buttons[7],
                self.scale(76),
                y + search_layout.height,
                self.scale(180),
                self.scale(32),
                1,
            );
            let folder_y = y + search_layout.height + self.scale(40);
            for (row, field) in [self.search.directory, self.search.filters]
                .into_iter()
                .enumerate()
            {
                MoveWindow(
                    self.search.folder_labels[row],
                    self.scale(12),
                    folder_y + self.scale(row as i32 * 40),
                    self.scale(56),
                    self.scale(32),
                    1,
                );
                MoveWindow(
                    field,
                    self.scale(76),
                    folder_y + self.scale(row as i32 * 40),
                    (width - self.scale(420)).max(1),
                    self.scale(32),
                    1,
                );
            }
            MoveWindow(
                self.search.folder_buttons[0],
                width - self.scale(330),
                folder_y,
                self.scale(110),
                self.scale(32),
                1,
            );
            MoveWindow(
                self.search.folder_buttons[1],
                width - self.scale(210),
                folder_y,
                self.scale(192),
                self.scale(32),
                1,
            );
            MoveWindow(
                self.search.recursive,
                width - self.scale(330),
                folder_y + self.scale(40),
                self.scale(125),
                self.scale(32),
                1,
            );
            MoveWindow(
                self.search.hidden,
                width - self.scale(195),
                folder_y + self.scale(40),
                self.scale(175),
                self.scale(32),
                1,
            );
            for control in [
                self.search.directory,
                self.search.filters,
                self.search.recursive,
                self.search.hidden,
            ]
            .into_iter()
            .chain(self.search.folder_labels)
            .chain(self.search.folder_buttons.iter().copied())
            {
                ShowWindow(
                    control,
                    if self.search.visible && self.search.folder_visible {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
            for control in [
                self.search.query,
                self.search.replace,
                self.search.mode,
                self.search.case,
                self.search.word,
            ]
            .into_iter()
            .chain(self.search.labels)
            .chain(self.search.buttons.iter().copied())
            {
                ShowWindow(
                    control,
                    if self.search.visible {
                        SW_SHOW
                    } else {
                        SW_HIDE
                    },
                );
            }
            select_pane(self.hwnd, self.focused);
            InvalidateRect(self.hwnd, null(), 1);
        }
    }
    fn add_document(&mut self, mut snapshot: DocumentSnapshot) -> Result<()> {
        if self.documents.len() >= 256 {
            return Err("At most 256 tabs can be open.".into());
        }
        let handle = self.scratch.create_document()?;
        let language = self
            .languages
            .iter()
            .position(|l| l.name == snapshot.language)
            .unwrap_or(0);
        self.scratch.attach(&handle);
        self.scratch.set_text(&snapshot.text)?;
        self.scratch
            .send(SCI_SETEOLMODE, snapshot.eol.scintilla(), 0);
        self.scratch.language_with_font(
            &self.languages[language],
            self.palette,
            &self.editor_font,
        )?;
        snapshot.id = self.next_id;
        self.next_id += 1;
        snapshot.text = String::new();
        let dirty = snapshot.dirty;
        self.groups[self.focused].push(snapshot.id);
        self.documents.push(Document {
            handle,
            snapshot,
            language,
            base_dirty: dirty,
            metadata_dirty: false,
            revision: 0,
            styled_revision: None,
            last_edit: Instant::now(),
        });
        if self.documents.len() == 1 {
            self.refresh_views()?;
            self.editor()
                .send(SCI_GOTOPOS, self.documents[0].snapshot.caret, 0);
        }
        self.switch(self.documents.len() - 1)?;
        self.touch();
        Ok(())
    }
    fn new_document(&mut self) -> Result<()> {
        if self.comparing {
            self.clear_compare();
        }
        let title = format!("Untitled {}", self.next_id);
        self.add_document(DocumentSnapshot {
            id: 0,
            title,
            path: None,
            text: String::new(),
            encoding: Encoding::Utf8,
            eol: Eol::CrLf,
            language: "Plain text".into(),
            dirty: false,
            disk_hash: None,
            caret: 0,
            pinned: false,
        })
    }
    fn switch(&mut self, index: usize) -> Result<()> {
        if index >= self.documents.len() {
            return Ok(());
        }
        let id = self.documents[index].snapshot.id;
        if !self.groups[self.focused].contains(&id) {
            self.focused = 1 - self.focused;
        }
        if self.index() == index {
            self.focus_pane(self.focused);
            self.editor().focus();
            return Ok(());
        }
        if self.comparing {
            self.clear_compare();
        }
        if self.focused == 1 && self.secondary.is_some() {
            self.secondary = Some(index);
        } else {
            self.primary = index;
            self.focused = 0;
        }
        self.refresh_views()?;
        self.editor()
            .send(SCI_GOTOPOS, self.documents[index].snapshot.caret, 0);
        self.update_tabs();
        self.update_status();
        self.editor().focus();
        self.touch();
        if self.tree_visible {
            self.schedule_json();
        }
        Ok(())
    }
    fn focus_pane(&mut self, pane: usize) {
        if pane > 1 || (pane == 1 && self.secondary.is_none()) {
            return;
        }
        let changed = self.focused != pane;
        self.focused = pane;
        self.configure_map();
        self.update_tabs();
        self.update_status();
        if changed {
            self.touch();
            if self.tree_visible {
                self.schedule_json();
            }
        }
    }
    fn refresh_views(&mut self) -> Result<()> {
        for (pane, index) in [(0, Some(self.primary)), (1, self.secondary)] {
            if let Some(index) = index {
                let doc = &self.documents[index];
                self.editors[pane].attach(&doc.handle);
                self.editors[pane].language_with_font(
                    &self.languages[doc.language],
                    self.palette,
                    &self.editor_font,
                )?;
                self.editors[pane].send(SCI_SETEOLMODE, doc.snapshot.eol.scintilla(), 0);
                self.editors[pane].send(
                    SCI_SETWRAPMODE,
                    (self.wrap && !(self.comparing && self.compare_options.align)) as usize,
                    0,
                );
            }
        }
        self.apply_symbols();
        self.configure_map();
        self.layout();
        Ok(())
    }
    fn split_tab(&mut self, id: u64) -> Result<()> {
        self.transfer_tab(id, false)
    }
    fn transfer_tab(&mut self, id: u64, clone: bool) -> Result<()> {
        let index = self
            .documents
            .iter()
            .position(|doc| doc.snapshot.id == id)
            .ok_or("This tab is no longer open.")?;
        self.clear_compare();
        let source = self.focused;
        let other = 1 - source;
        if !clone && self.groups[source].len() == 1 {
            self.new_document()?;
        }
        if !self.groups[other].contains(&id) {
            self.groups[other].push(id);
            self.groups[other].sort_by_key(|id| {
                !self
                    .documents
                    .iter()
                    .find(|doc| doc.snapshot.id == *id)
                    .unwrap()
                    .snapshot
                    .pinned
            });
        }
        if !clone {
            self.groups[source].retain(|entry| *entry != id);
            if self.index() == index {
                let replacement = self
                    .documents
                    .iter()
                    .position(|doc| self.groups[source].contains(&doc.snapshot.id))
                    .unwrap();
                if source == 0 {
                    self.primary = replacement;
                } else {
                    self.secondary = Some(replacement);
                }
            }
        }
        if other == 1 {
            self.secondary = Some(index);
        } else {
            self.primary = index;
        }
        self.focused = other;
        self.refresh_views()?;
        self.editors[other].send(SCI_GOTOPOS, self.documents[index].snapshot.caret, 0);
        self.update_tabs();
        self.update_status();
        self.editor().focus();
        self.touch();
        Ok(())
    }
    fn compare_tabs(&mut self, current: u64, target: u64) -> Result<()> {
        let left = self
            .documents
            .iter()
            .position(|doc| doc.snapshot.id == current)
            .ok_or("The current document is no longer open.")?;
        let right = self
            .documents
            .iter()
            .position(|doc| doc.snapshot.id == target)
            .ok_or("This tab is no longer open.")?;
        self.begin_compare(left, right)
    }
    fn configure_map(&self) {
        if self.documents.is_empty() {
            return;
        }
        let doc = &self.documents[self.index()];
        self.map.attach(&doc.handle);
        self.map.theme_with_font(
            &self.languages[doc.language],
            self.palette,
            &self.editor_font,
        );
        for style in 0..256 {
            self.map.send(SCI_STYLESETSIZEFRACTIONAL, style, 200);
        }
        for margin in 0..5 {
            self.map.send(SCI_SETMARGINWIDTHN, margin, 0);
        }
        self.map.send(SCI_SETCARETWIDTH, 0, 0);
        self.map.send(SCI_SETCARETLINEVISIBLE, 0, 0);
        self.map.send(SCI_SETHSCROLLBAR, 0, 0);
        self.map.send(SCI_SETVSCROLLBAR, 0, 0);
        self.map.send(SCI_SETWRAPMODE, 0, 0);
        self.map.send(SCI_SETFIRSTVISIBLELINE, 0, 0);
        self.map.send(SCI_SETSEL, 0, 0);
        self.update_map_view();
    }
    fn update_map_view(&self) {
        if !self.map_visible {
            return;
        }
        let editor = self.editor();
        let first = editor.send(SCI_GETFIRSTVISIBLELINE, 0, 0) as usize;
        let count = editor.send(SCI_LINESONSCREEN, 0, 0).max(1) as usize;
        let first_line = editor.send(SCI_DOCLINEFROMVISIBLE, first, 0).max(0) as usize;
        let last_line = editor.send(SCI_DOCLINEFROMVISIBLE, first + count, 0);
        let start = editor.send(SCI_POSITIONFROMLINE, first_line, 0).max(0) as usize;
        let end = if last_line < 0 {
            editor.length()
        } else {
            let pos = editor.send(SCI_POSITIONFROMLINE, last_line as usize, 0);
            if pos < 0 {
                editor.length()
            } else {
                pos as usize
            }
        };
        self.map.send(SCI_SETSEL, start, end as isize);
        self.map.send(SCI_SCROLLCARET, 0, 0);
    }
    fn update_tabs(&self) {
        select_pane(self.hwnd, self.focused);
        unsafe {
            for (pane, tab) in [self.tabs, self.right_tabs].into_iter().enumerate() {
                SendMessageW(tab, WM_SETREDRAW, 0, 0);
                SendMessageW(tab, TCM_DELETEALLITEMS, 0, 0);
                let visible = if pane == 0 {
                    Some(self.primary)
                } else {
                    self.secondary
                };
                let mut selected = 0;
                for (i, id) in self.groups[pane].iter().enumerate() {
                    let (index, doc) = self
                        .documents
                        .iter()
                        .enumerate()
                        .find(|(_, doc)| doc.snapshot.id == *id)
                        .unwrap();
                    if Some(index) == visible {
                        selected = i;
                    }
                    let mut label = wide(&format!(
                        "{}{}{}",
                        if doc.snapshot.pinned { "[P] " } else { "" },
                        doc.snapshot.title,
                        if doc.snapshot.dirty { " *" } else { "" }
                    ));
                    let mut item: TCITEMW = zeroed();
                    item.mask = TCIF_TEXT | TCIF_PARAM;
                    item.pszText = label.as_mut_ptr();
                    item.lParam = doc.snapshot.id as isize;
                    SendMessageW(tab, TCM_INSERTITEMW, i, (&item as *const TCITEMW) as isize);
                }
                SendMessageW(tab, TCM_SETCURSEL, selected, 0);
                SendMessageW(tab, WM_SETREDRAW, 1, 0);
                InvalidateRect(tab, null(), 1);
            }
        }
        let doc = &self.documents[self.index()];
        set_text(
            self.hwnd,
            &format!(
                "{}{} - rstpd",
                doc.snapshot.title,
                if doc.snapshot.dirty { " *" } else { "" }
            ),
        );
    }
    fn reorder_tabs(&mut self, id: u64, requested: Option<usize>) -> Result<()> {
        let from = self
            .documents
            .iter()
            .position(|doc| doc.snapshot.id == id)
            .ok_or("This tab is no longer open.")?;
        let primary = self.documents[self.primary].snapshot.id;
        let secondary = self
            .secondary
            .map(|index| self.documents[index].snapshot.id);
        if let Some(requested) = requested {
            let group = &mut self.groups[self.focused];
            let local = group
                .iter()
                .position(|entry| *entry == id)
                .ok_or("Tab is outside this pane.")?;
            let pins: Vec<_> = group
                .iter()
                .map(|id| {
                    self.documents
                        .iter()
                        .find(|doc| doc.snapshot.id == *id)
                        .unwrap()
                        .snapshot
                        .pinned
                })
                .collect();
            let target = tabs::move_target(&pins, local, requested)?;
            let id = group.remove(local);
            group.insert(target, id);
        } else {
            self.documents[from].snapshot.pinned = !self.documents[from].snapshot.pinned;
            for group in &mut self.groups {
                group.sort_by_key(|id| {
                    !self
                        .documents
                        .iter()
                        .find(|doc| doc.snapshot.id == *id)
                        .unwrap()
                        .snapshot
                        .pinned
                });
            }
        }
        self.primary = self
            .documents
            .iter()
            .position(|doc| doc.snapshot.id == primary)
            .expect("existing primary");
        self.secondary =
            secondary.and_then(|id| self.documents.iter().position(|doc| doc.snapshot.id == id));
        self.update_tabs();
        self.touch();
        Ok(())
    }
    fn poll_monitor(&mut self) -> Result<()> {
        match self.monitor.rx.try_recv() {
            Ok(changes) => {
                self.monitor_busy = false;
                if self.monitor_files {
                    for change in changes {
                        self.apply_external_change(change)?;
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.monitor_files = false;
                self.monitor_busy = false;
                self.update_symbol_checks();
                return Err(
                    "File monitoring worker stopped. Automatic reload has been disabled.".into(),
                );
            }
        }
        if self.monitor_files
            && !self.monitor_busy
            && self.monitor_last.elapsed() >= Duration::from_secs(1)
        {
            let requests = self
                .documents
                .iter()
                .filter_map(|doc| {
                    doc.snapshot.path.clone().map(|path| monitor::Request {
                        id: doc.snapshot.id,
                        path,
                        known_hash: doc.snapshot.disk_hash,
                        encoding: doc.snapshot.encoding.clone(),
                    })
                })
                .collect();
            self.monitor.submit(requests)?;
            self.monitor_busy = true;
            self.monitor_last = Instant::now();
        }
        Ok(())
    }
    fn apply_external_change(&mut self, change: monitor::Change) -> Result<()> {
        let Some(index) = self.documents.iter().position(|doc| {
            doc.snapshot.id == change.id
                && doc.snapshot.path.as_ref() == Some(&change.path)
                && doc.snapshot.disk_hash == change.baseline_hash
        }) else {
            return Ok(());
        };
        let snapshot = match change.result {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.note(format!("External file monitoring: {error}"));
                return Ok(());
            }
        };
        self.scratch.attach(&self.documents[index].handle);
        let dirty = self.documents[index].base_dirty
            || self.documents[index].metadata_dirty
            || self.scratch.send(SCI_GETMODIFY, 0, 0) != 0;
        if dirty
            && ask(
                self.hwnd,
                &format!(
                    "{} was changed by another program.\n\nReload from disk and discard this tab's unsaved edits?\nChoose No to keep your edits; saving will still check for a disk conflict.",
                    change.path.display()
                ),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
            ) != IDYES
        {
            self.note("External change not reloaded: your unsaved edits were kept.");
            return Ok(());
        }
        let views: Vec<_> = [(0, self.primary), (1, self.secondary.unwrap_or(usize::MAX))]
            .into_iter()
            .filter(|(_, doc)| *doc == index)
            .map(|(pane, _)| {
                let editor = self.editors[pane];
                (
                    editor,
                    editor.send(SCI_GETANCHOR, 0, 0),
                    editor.position(),
                    editor.send(SCI_GETFIRSTVISIBLELINE, 0, 0),
                    editor.send(SCI_GETXOFFSET, 0, 0),
                )
            })
            .collect();
        self.scratch.set_text(&snapshot.text)?;
        self.scratch
            .send(SCI_SETEOLMODE, snapshot.eol.scintilla(), 0);
        for (editor, anchor, caret, scroll, x) in views {
            editor.send(
                SCI_SETSEL,
                (anchor.max(0) as usize).min(editor.length()),
                caret.min(editor.length()) as isize,
            );
            editor.send(SCI_SETFIRSTVISIBLELINE, scroll.max(0) as usize, 0);
            editor.send(SCI_SETXOFFSET, x.max(0) as usize, 0);
        }
        let doc = &mut self.documents[index];
        doc.snapshot.encoding = snapshot.encoding;
        doc.snapshot.eol = snapshot.eol;
        doc.snapshot.disk_hash = Some(snapshot.hash);
        doc.snapshot.dirty = false;
        doc.snapshot.caret = doc.snapshot.caret.min(snapshot.text.len());
        doc.base_dirty = false;
        doc.metadata_dirty = false;
        doc.revision += 1;
        doc.styled_revision = None;
        doc.last_edit = Instant::now() - Duration::from_secs(1);
        if self.comparing {
            self.compare_due = Some(Instant::now());
        }
        if self.tree_visible && index == self.index() {
            self.schedule_json();
        }
        self.update_tabs();
        self.touch();
        self.note(format!(
            "Reloaded external changes: {}",
            change.path.display()
        ));
        Ok(())
    }
    fn update_status(&self) {
        if self.documents.is_empty() {
            return;
        }
        let doc = &self.documents[self.index()];
        let editor = self.editor();
        let line = editor.send(SCI_LINEFROMPOSITION, editor.position(), 0) + 1;
        let column = editor.send(SCI_GETCOLUMN, editor.position(), 0) + 1;
        let characters = match editor.character_counts() {
            Ok(counts) => counts.to_string(),
            Err(error) => format!("Character count unavailable: {error}"),
        };
        let state = if self.recovery_error.is_some() {
            "RECOVERY FAILED"
        } else if self.recovery_busy {
            "Backing up..."
        } else if self.recovered_revision == self.revision {
            "Session backed up"
        } else {
            "Recovery pending"
        };
        let note = if let Some(error) = &self.recovery_error {
            error.as_str()
        } else {
            &self.note
        };
        set_text(
            self.status,
            &format!(
                "Ln {line}, Col {column}   |   {characters}   |   {} selections   |   {}   |   {}   {}   |   {state}   {}",
                editor.send(SCI_GETSELECTIONS, 0, 0),
                self.languages[doc.language].name,
                doc.snapshot.encoding.label(),
                doc.snapshot.eol.label(),
                note
            ),
        );
    }
    fn open_path(&mut self, path: &Path, encoding: Option<&Encoding>) -> Result<()> {
        let path = fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some(index) = self
            .documents
            .iter()
            .position(|doc| doc.snapshot.path.as_ref() == Some(&path))
        {
            self.switch(index)?;
            return Ok(());
        }
        let bytes = session::read_bounded(&path, core::MAX_DOCUMENT_BYTES)?;
        let oem = Encoding::Legacy("IBM437".into());
        let fallback = if encoding.is_none()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("nfo"))
            && std::str::from_utf8(&bytes).is_err()
            && !bytes.starts_with(b"\xff\xfe")
            && !bytes.starts_with(b"\xfe\xff")
            && !bytes.starts_with(b"\0\0\xfe\xff")
        {
            Some(&oem)
        } else {
            encoding
        };
        let (text, encoding) = core::decode(&bytes, fallback)?;
        let eol = Eol::detect(&text);
        let language = languages::detect(&path, &self.languages);
        let title = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let fallback = matches!(&encoding,Encoding::Legacy(s) if s == "windows-1252");
        self.add_document(DocumentSnapshot {
            id: 0,
            title,
            path: Some(path),
            text,
            encoding,
            eol,
            language: self.languages[language].name.clone(),
            dirty: false,
            disk_hash: Some(session::fingerprint(&bytes)),
            caret: 0,
            pinned: false,
        })?;
        if fallback {
            self.note("No Unicode BOM / valid UTF-8: opened as Windows-1252; Encoding > Reopen can change this.");
        }
        Ok(())
    }
    fn dialog(&self, save: bool) -> Result<Option<PathBuf>> {
        unsafe {
            let mut filename = vec![0u16; 32768];
            if save {
                let initial = self.documents[self.index()]
                    .snapshot
                    .path
                    .as_ref()
                    .map(|p| p.as_os_str().encode_wide().collect::<Vec<_>>())
                    .unwrap_or_else(|| wide(&self.documents[self.index()].snapshot.title));
                let count = initial.len().min(filename.len() - 1);
                filename[..count].copy_from_slice(&initial[..count]);
            }
            let filter = wide("All files (*.*)\0*.*\0Text files (*.txt)\0*.txt\0");
            let mut dialog: OPENFILENAMEW = zeroed();
            dialog.lStructSize = size_of::<OPENFILENAMEW>() as u32;
            dialog.hwndOwner = self.hwnd;
            dialog.lpstrFilter = filter.as_ptr();
            dialog.lpstrFile = filename.as_mut_ptr();
            dialog.nMaxFile = filename.len() as u32;
            dialog.Flags = OFN_EXPLORER
                | OFN_NOCHANGEDIR
                | OFN_PATHMUSTEXIST
                | if save {
                    OFN_OVERWRITEPROMPT
                } else {
                    OFN_FILEMUSTEXIST
                };
            let result = if save {
                GetSaveFileNameW(&mut dialog)
            } else {
                GetOpenFileNameW(&mut dialog)
            };
            if result == 0 {
                let error = CommDlgExtendedError();
                return if error == 0 {
                    Ok(None)
                } else {
                    Err(format!("File dialog failed: {error:#x}"))
                };
            }
            use std::os::windows::ffi::OsStringExt;
            let end = filename
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(filename.len());
            Ok(Some(PathBuf::from(std::ffi::OsString::from_wide(
                &filename[..end],
            ))))
        }
    }
    fn save_document(&mut self, save_as: bool) -> Result<bool> {
        let index = self.index();
        let path = if save_as || self.documents[index].snapshot.path.is_none() {
            let Some(path) = self.dialog(true)? else {
                return Ok(false);
            };
            path
        } else {
            self.documents[index].snapshot.path.clone().unwrap()
        };
        let canonical = canonical_target(&path);
        if self
            .documents
            .iter()
            .enumerate()
            .any(|(i, doc)| i != index && doc.snapshot.path.as_ref() == Some(&canonical))
        {
            return Err(
                "This path is already open in another tab. Save to a different path.".into(),
            );
        }
        let same = self.documents[index].snapshot.path.as_ref() == Some(&canonical);
        if same {
            let current = if path.exists() {
                Some(session::fingerprint(&session::read_bounded(
                    &path,
                    core::MAX_DOCUMENT_BYTES,
                )?))
            } else {
                None
            };
            if current != self.documents[index].snapshot.disk_hash
                && ask(
                    self.hwnd,
                    "The file changed or was deleted outside rstpd. Overwrite the external version?",
                    MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
                ) != IDYES
            {
                return Ok(false);
            }
        }
        let text = self.editor().text()?;
        let bytes = self.documents[index].snapshot.encoding.encode(&text)?;
        session::atomic_write(&path, &bytes)?;
        let doc = &mut self.documents[index];
        let was_unnamed = doc.snapshot.path.is_none();
        doc.snapshot.path = Some(fs::canonicalize(&path).map_err(|e| e.to_string())?);
        doc.snapshot.title = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        doc.snapshot.disk_hash = Some(session::fingerprint(&bytes));
        doc.snapshot.dirty = false;
        doc.base_dirty = false;
        doc.metadata_dirty = false;
        if was_unnamed || save_as {
            doc.language = languages::detect(&path, &self.languages);
            doc.snapshot.language = self.languages[doc.language].name.clone();
        }
        self.editor().send(SCI_SETSAVEPOINT, 0, 0);
        self.touch();
        self.refresh_views()?;
        self.update_tabs();
        self.note("Saved.");
        Ok(true)
    }
    fn close_document(&mut self) -> Result<()> {
        let index = self.index();
        let id = self.documents[index].snapshot.id;
        if self.groups[1 - self.focused].contains(&id) {
            self.clear_compare();
            self.groups[self.focused].retain(|entry| *entry != id);
            self.normalize_groups();
            self.refresh_views()?;
            self.update_tabs();
            self.update_status();
            self.editor().focus();
            self.touch();
            if self.tree_visible {
                self.schedule_json();
            }
            return Ok(());
        }
        if self.documents[index].snapshot.dirty {
            match ask(
                self.hwnd,
                "Save this document before closing its tab?\n\nNo discards its edits and removes its recovery copy. To keep all tabs without choosing filenames, close the app instead.",
                MB_YESNOCANCEL | MB_ICONWARNING,
            ) {
                IDYES => {
                    if !self.save_document(false)? {
                        return Ok(());
                    }
                }
                IDNO => {}
                _ => return Ok(()),
            }
        }
        self.clear_compare();
        if self.documents.len() == 1 {
            self.new_document()?;
        }
        let closed = self.documents.remove(index);
        for group in &mut self.groups {
            group.retain(|entry| *entry != id);
        }
        if self.results.data.as_ref().is_some_and(|results| {
            results
                .files
                .iter()
                .any(|file| file.id == closed.snapshot.id)
        }) {
            set_text(
                self.results.title,
                "A result document was closed - rerun Find All to refresh.",
            );
        }
        self.primary = if self.primary > index {
            self.primary - 1
        } else {
            self.primary.min(self.documents.len() - 1)
        };
        self.secondary = self.secondary.and_then(|i| {
            if i == index {
                None
            } else if i > index {
                Some(i - 1)
            } else {
                Some(i)
            }
        });
        self.normalize_groups();
        if self.secondary.is_none() {
            self.focused = 0;
            self.editors[1].attach(&self.documents[self.primary].handle);
        }
        self.refresh_views()?;
        self.json_document = None;
        self.json_nodes.clear();
        self.json_handles.clear();
        unsafe {
            SendMessageW(self.tree, TVM_DELETEITEM, 0, TVI_ROOT);
        }
        self.touch();
        self.update_tabs();
        self.update_status();
        self.editor().focus();
        if self.tree_visible {
            self.schedule_json();
        }
        Ok(())
    }
    fn snapshot(&mut self) -> Result<Session> {
        let documents = self
            .documents
            .iter()
            .map(|doc| {
                self.scratch.attach(&doc.handle);
                let mut snapshot = doc.snapshot.clone();
                snapshot.text = self.scratch.text()?;
                Ok(snapshot)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Session {
            version: session::SESSION_VERSION,
            documents,
            active: self.index(),
            theme: self.theme.clone(),
            editor_font: self.editor_font.clone(),
            show_symbols: self.show_symbols,
            monitor_files: self.monitor_files,
            compare_options: self.compare_options.clone(),
            custom_languages: self
                .languages
                .iter()
                .filter_map(|language| language.custom.as_deref().cloned())
                .collect(),
            completion_api: self.completion_api.clone(),
            pane_documents: self.groups.clone().map(|group| {
                group
                    .iter()
                    .filter_map(|id| self.documents.iter().position(|doc| doc.snapshot.id == *id))
                    .collect()
            }),
            pane_selected: [self.primary, self.secondary.unwrap_or(0)],
            focused_pane: self.focused,
        })
    }
    fn normalize_groups(&mut self) {
        for group in &mut self.groups {
            let mut seen = HashSet::new();
            group.retain(|id| {
                seen.insert(*id) && self.documents.iter().any(|doc| doc.snapshot.id == *id)
            });
            group.sort_by_key(|id| {
                !self
                    .documents
                    .iter()
                    .find(|doc| doc.snapshot.id == *id)
                    .unwrap()
                    .snapshot
                    .pinned
            });
        }
        if self.groups[0].is_empty() {
            self.groups.swap(0, 1);
            self.primary = self.secondary.unwrap_or(0);
            self.focused = 0;
        }
        let first = |pane: usize| {
            self.documents
                .iter()
                .position(|doc| self.groups[pane].contains(&doc.snapshot.id))
        };
        if !self
            .documents
            .get(self.primary)
            .is_some_and(|doc| self.groups[0].contains(&doc.snapshot.id))
        {
            self.primary = first(0).unwrap_or(0);
        }
        if self.groups[1].is_empty() {
            self.secondary = None;
            self.focused = 0;
        } else if !self
            .secondary
            .and_then(|i| self.documents.get(i))
            .is_some_and(|doc| self.groups[1].contains(&doc.snapshot.id))
        {
            self.secondary = first(1);
        }
    }
    fn search_settings(&self) -> Result<Search> {
        let mode = unsafe { SendMessageW(self.search.mode, CB_GETCURSEL, 0, 0) };
        let case =
            unsafe { SendMessageW(self.search.case, BM_GETCHECK, 0, 0) } == BST_CHECKED as isize;
        let word =
            unsafe { SendMessageW(self.search.word, BM_GETCHECK, 0, 0) } == BST_CHECKED as isize;
        Search::new(
            &window_text(self.search.query),
            match mode {
                1 => SearchMode::Extended,
                2 => SearchMode::Regex,
                _ => SearchMode::Literal,
            },
            case,
            word,
        )
    }
    fn show_search(&mut self) -> Result<()> {
        self.search.visible = true;
        self.last_zero_match = None;
        let selection = self.editor().selection();
        if !selection.is_empty() && selection.len() < 32768 {
            let value = self.editor().range(selection)?;
            if !value.contains(['\r', '\n', '\0']) {
                set_text(self.search.query, &value);
            }
        }
        self.layout();
        unsafe {
            SetFocus(self.search.query);
            SendMessageW(self.search.query, EM_SETSEL, 0, -1);
        }
        Ok(())
    }
    fn show_folder_search(&mut self) -> Result<()> {
        self.search.folder_visible = true;
        if window_text(self.search.directory).is_empty() {
            let directory = self.documents[self.index()]
                .snapshot
                .path
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .map(Ok)
                .unwrap_or_else(std::env::current_dir)
                .map_err(|e| e.to_string())?;
            set_text(self.search.directory, &directory.display().to_string());
        }
        self.show_search()
    }
    fn browse_folder(&self) -> Result<()> {
        unsafe {
            let title = wide("Choose the folder to search");
            let mut info: BROWSEINFOW = zeroed();
            info.hwndOwner = self.hwnd;
            info.lpszTitle = title.as_ptr();
            info.ulFlags = BIF_RETURNONLYFSDIRS;
            let item = SHBrowseForFolderW(&info);
            if item.is_null() {
                return Ok(());
            }
            let mut path = vec![0u16; 32768];
            let valid = SHGetPathFromIDListW(item, path.as_mut_ptr());
            CoTaskMemFree(item.cast());
            if valid == 0 {
                return Err("The chosen folder cannot be represented as a filesystem path.".into());
            }
            let length = path.iter().position(|&ch| ch == 0).unwrap_or(path.len());
            set_text(
                self.search.directory,
                &String::from_utf16(&path[..length]).map_err(|e| e.to_string())?,
            );
        }
        Ok(())
    }
    fn start_find_files(&mut self) -> Result<()> {
        if self.results.job.is_some() {
            return Err(
                "A search is already running. Cancel it before searching another folder.".into(),
            );
        }
        let query = window_text(self.search.query);
        let search = self.search_settings()?;
        let options = FolderOptions {
            directory: PathBuf::from(window_text(self.search.directory)),
            filters: window_text(self.search.filters),
            recursive: unsafe { SendMessageW(self.search.recursive, BM_GETCHECK, 0, 0) }
                == BST_CHECKED as isize,
            hidden: unsafe { SendMessageW(self.search.hidden, BM_GETCHECK, 0, 0) }
                == BST_CHECKED as isize,
        };
        options.validate()?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let token = cancelled.clone();
        let (tx, rx) = mpsc::channel();
        self.results
            .editor
            .set_read_only_text("Searching files on disk...\n")?;
        self.results
            .editor
            .clear_indicator(search_results::MATCH_INDICATOR);
        self.results.data = None;
        self.results.links.clear();
        self.results.current = None;
        self.results.visible = true;
        std::thread::spawn(move || {
            let _ = tx.send(folder_search::find_in_files(
                &search, query, options, &token,
            ));
        });
        self.results.job = Some(SearchTask {
            rx,
            cancelled,
            discard: false,
        });
        self.result_message("Searching files on disk (unsaved tab edits are not searched)...");
        self.update_result_buttons();
        self.layout();
        Ok(())
    }
    fn find(&mut self, previous: bool) -> Result<()> {
        let search = self.search_settings()?;
        let editor = self.editor();
        let text = self.search_text(editor)?;
        let selection = editor.selection();
        let search_key = format!(
            "{}:{}:{}:{}",
            window_text(self.search.query),
            unsafe { SendMessageW(self.search.mode, CB_GETCURSEL, 0, 0) },
            unsafe { SendMessageW(self.search.case, BM_GETCHECK, 0, 0) },
            unsafe { SendMessageW(self.search.word, BM_GETCHECK, 0, 0) }
        );
        let document = self.documents[self.index()].snapshot.id;
        let range = if previous {
            let matches = search.matches(&text)?;
            matches
                .iter()
                .rev()
                .find(|r| r.start < selection.start)
                .or(matches.last())
                .cloned()
        } else {
            let mut start = selection.end;
            if selection.is_empty()
                && self.last_zero_match.as_ref() == Some(&(document, start, search_key.clone()))
            {
                start = editor.send(SCI_POSITIONAFTER, start, 0).max(0) as usize;
                if start == selection.end && start == text.len() {
                    start = 0;
                }
            }
            search.find(&text, start)?
        };
        if let Some(range) = range {
            self.last_zero_match = if range.is_empty() {
                Some((document, range.start, search_key))
            } else {
                None
            };
            editor.select(range);
            self.note("Match found.");
        } else {
            self.note("No matches.");
        }
        Ok(())
    }
    fn tool_text(&self, editor: Editor) -> Result<String> {
        if editor.length() > core::MAX_TOOL_BYTES {
            return Err("This tool is limited to 16 MiB documents.".into());
        }
        editor.text()
    }
    fn search_text(&self, editor: Editor) -> Result<String> {
        Search::validate_size(editor.length())?;
        editor.text()
    }
    fn replace(&mut self, all: bool) -> Result<()> {
        let search = self.search_settings()?;
        let editor = self.editor();
        let text = self.search_text(editor)?;
        let replacement = window_text(self.search.replace);
        if all {
            let (out, count) = search.replace_all(&text, &replacement)?;
            if count > 0 {
                editor.replace_all(&out)?;
            }
            self.note(format!("Replaced {count} matches."));
        } else {
            let range = editor.selection();
            let found = search.find(&text, range.start)?;
            if found == Some(range.clone()) {
                let output = search.replacement(&text, range.clone(), &replacement)?;
                let end = range.start + output.len();
                editor.replace(range, &output)?;
                editor.select(end..end);
            }
            self.find(false)?;
        }
        Ok(())
    }
    fn theme_results(&self) {
        let editor = self.results.editor;
        editor.theme(&self.languages[0], self.palette);
        editor.send(SCI_SETMARGINWIDTHN, 0, 0);
        editor.send(SCI_SETMARGINWIDTHN, 1, 0);
        editor.send(SCI_SETWRAPMODE, 0, 0);
        editor.send(SCI_SETCARETLINEVISIBLE, 1, 0);
        editor.send(SCI_STYLESETFORE, 1, self.palette.accent as isize);
        editor.send(SCI_STYLESETBOLD, 1, 1);
        editor.send(SCI_STYLESETBOLD, 2, 1);
        editor.send(SCI_INDICSETSTYLE, search_results::MATCH_INDICATOR, 7);
        editor.send(
            SCI_INDICSETFORE,
            search_results::MATCH_INDICATOR,
            self.palette.accent as isize,
        );
        editor.send(SCI_INDICSETALPHA, search_results::MATCH_INDICATOR, 75);
        editor.send(
            SCI_INDICSETOUTLINEALPHA,
            search_results::MATCH_INDICATOR,
            140,
        );
        editor.send(SCI_INDICSETUNDER, search_results::MATCH_INDICATOR, 1);
    }
    fn result_message(&mut self, message: &str) {
        set_text(self.results.title, message);
        self.note(message);
    }
    fn update_result_buttons(&self) {
        unsafe {
            let has_results = !self.results.links.is_empty();
            for (index, button) in self.results.buttons.iter().enumerate() {
                let enabled = match index {
                    0 | 1 => has_results,
                    2 => self.results.job.is_some(),
                    _ => true,
                };
                EnableWindow(*button, enabled as i32);
            }
            for button in self.search.buttons.iter().skip(5) {
                EnableWindow(*button, self.results.job.is_none() as i32);
            }
            EnableWindow(
                self.search.folder_buttons[1],
                self.results.job.is_none() as i32,
            );
        }
    }
    fn start_find_all(&mut self, all_open: bool) -> Result<()> {
        if self.results.job.is_some() {
            self.results.visible = true;
            self.layout();
            self.note("Find All is still running. Cancel it before starting another search.");
            return Ok(());
        }
        if window_text(self.search.query).is_empty() {
            self.show_search()?;
            if window_text(self.search.query).is_empty() {
                self.note("Enter a search expression, then choose Find All.");
                return Ok(());
            }
        }
        let query = window_text(self.search.query);
        let search = self.search_settings()?;
        let mut bytes = 0usize;
        let mut inputs = Vec::new();
        for (index, doc) in self.documents.iter().enumerate() {
            if !all_open && index != self.index() {
                continue;
            }
            self.scratch.attach(&doc.handle);
            let length = self.scratch.length();
            Search::validate_size(length).map_err(|error| {
                format!(
                    "{}: {error} No documents were searched.",
                    doc.snapshot.title
                )
            })?;
            bytes += length;
            if bytes > search_results::MAX_BATCH_BYTES {
                return Err("Find All is limited to 256 MiB across open documents. Close some tabs or search the current document.".into());
            }
            let title = doc
                .snapshot
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| doc.snapshot.title.clone());
            inputs.push(SearchInput {
                id: doc.snapshot.id,
                revision: doc.revision,
                title,
                text: self.scratch.text()?,
                tab_width: self.scratch.send(SCI_GETTABWIDTH, 0, 0).max(1) as usize,
            });
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let token = cancelled.clone();
        let (tx, rx) = mpsc::channel();
        self.results
            .editor
            .set_read_only_text("Searching open document snapshots...\n")?;
        self.results
            .editor
            .clear_indicator(search_results::MATCH_INDICATOR);
        self.results.data = None;
        self.results.links.clear();
        self.results.current = None;
        self.results.visible = true;
        std::thread::spawn(move || {
            let result = search_results::find_all(&search, query, inputs, &token);
            let _ = tx.send(result);
        });
        self.results.job = Some(SearchTask {
            rx,
            cancelled,
            discard: false,
        });
        self.result_message(if all_open {
            "Searching all open documents..."
        } else {
            "Searching current document..."
        });
        self.update_result_buttons();
        self.layout();
        Ok(())
    }
    fn apply_find_results(&mut self, results: SearchResults) -> Result<()> {
        let rendered = results.render();
        self.results.editor.set_read_only_text(&rendered.text)?;
        self.theme_results();
        self.results.editor.highlight(&rendered.highlight)?;
        self.results
            .editor
            .clear_indicator(search_results::MATCH_INDICATOR);
        for range in rendered.emphasis {
            self.results
                .editor
                .indicator(search_results::MATCH_INDICATOR, range)?;
        }
        self.results.links = rendered.links;
        self.results.current = None;
        let stale = results.files.iter().any(|file| {
            file.source.is_none()
                && !self
                    .documents
                    .iter()
                    .any(|doc| doc.snapshot.id == file.id && doc.revision == file.revision)
        });
        let summary = results.summary()
            + if stale {
                " - some documents changed; rerun to navigate those matches."
            } else {
                " - double-click or Enter to navigate."
            };
        self.results.data = Some(results);
        self.result_message(&summary);
        self.update_result_buttons();
        Ok(())
    }
    fn clear_find_results(&mut self) -> Result<()> {
        if let Some(job) = &mut self.results.job {
            job.cancelled.store(true, Ordering::Relaxed);
            job.discard = true;
        }
        self.results
            .editor
            .set_read_only_text("Search results cleared. Run Find All to search again.\n")?;
        self.results
            .editor
            .clear_indicator(search_results::MATCH_INDICATOR);
        self.results.data = None;
        self.results.links.clear();
        self.results.current = None;
        self.result_message("Search results cleared.");
        self.update_result_buttons();
        Ok(())
    }
    fn activate_result(&mut self, index: usize) -> Result<()> {
        let Some(link) = self.results.links.get(index) else {
            return Ok(());
        };
        let Some(results) = &self.results.data else {
            return Ok(());
        };
        let file = &results.files[link.file];
        let range = file.hits[link.hit].range.clone();
        let row = link.row;
        let source = file.source.clone();
        let id = file.id;
        let revision = file.revision;
        let document = if let Some(source) = source {
            folder_search::verify_source(&source)?;
            self.open_path(&source.path, None)?;
            let document = self.index();
            self.scratch.attach(&self.documents[document].handle);
            if session::fingerprint(self.scratch.text()?.as_bytes()) != source.text_hash {
                return Err("The open tab differs from the searched file. Your edits were kept; save or reload and search again.".into());
            }
            document
        } else {
            let Some(document) = self.documents.iter().position(|doc| doc.snapshot.id == id) else {
                self.result_message("This result's document was closed. Run Find All again.");
                return Ok(());
            };
            if self.documents[document].revision != revision {
                self.result_message("This document changed since the search. Run Find All again before navigating its results.");
                return Ok(());
            }
            document
        };
        // A compare binds two particular documents; do not silently change its pair.
        if self.comparing && document != self.index() {
            self.clear_compare();
        }
        if document != self.index() {
            self.switch(document)?;
        }
        self.editor().select(range);
        self.editor().focus();
        self.results.current = Some(index);
        let editor = self.results.editor;
        editor.send(SCI_ENSUREVISIBLEENFORCEPOLICY, row, 0);
        let start = editor.send(SCI_POSITIONFROMLINE, row, 0).max(0) as usize;
        let end = editor
            .send(SCI_GETLINEENDPOSITION, row, 0)
            .max(start as isize) as usize;
        editor.select(start..end);
        self.results.visible = true;
        self.layout();
        self.result_message(&format!(
            "Search result {} of {} (F4 / Shift+F4)",
            index + 1,
            self.results.links.len()
        ));
        Ok(())
    }
    fn activate_result_row(&mut self) -> Result<()> {
        let editor = self.results.editor;
        let row = editor.send(SCI_LINEFROMPOSITION, editor.position(), 0) as usize;
        if let Some(index) = self.results.links.iter().position(|link| link.row == row) {
            self.activate_result(index)?;
        } else {
            editor.send(SCI_TOGGLEFOLD, row, 0);
        }
        Ok(())
    }
    fn next_result(&mut self, previous: bool) -> Result<()> {
        let count = self.results.links.len();
        if count == 0 {
            self.results.visible = true;
            self.layout();
            self.note("No search matches are listed. Run Find All first.");
            return Ok(());
        }
        let index = match (self.results.current, previous) {
            (None, false) => 0,
            (None, true) => count - 1,
            (Some(index), false) => (index + 1) % count,
            (Some(index), true) => (index + count - 1) % count,
        };
        let valid = (0..count)
            .map(|step| {
                if previous {
                    (index + count - step) % count
                } else {
                    (index + step) % count
                }
            })
            .find(|candidate| {
                let Some(results) = &self.results.data else {
                    return false;
                };
                let file = &results.files[self.results.links[*candidate].file];
                file.source.is_some()
                    || self
                        .documents
                        .iter()
                        .any(|doc| doc.snapshot.id == file.id && doc.revision == file.revision)
            });
        self.activate_result(valid.unwrap_or(index))
    }
    fn poll_find_results(&mut self) -> Result<()> {
        let Some(job) = &self.results.job else {
            return Ok(());
        };
        let result = match job.rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Find All worker stopped unexpectedly.".into()))
            }
        };
        let Some(result) = result else {
            return Ok(());
        };
        let job = self.results.job.take().expect("active search task");
        let cancelled = job.cancelled.load(Ordering::Relaxed);
        let applied = (|| -> Result<()> {
            if !job.discard {
                match result {
                    Ok(results) if !cancelled => self.apply_find_results(results)?,
                    Ok(_) => {
                        self.results
                            .editor
                            .set_read_only_text("Search cancelled.\n")?;
                        self.result_message("Search cancelled.");
                    }
                    Err(error) => {
                        self.results
                            .editor
                            .set_read_only_text(&format!("Find All: {error}\n"))?;
                        self.result_message(&error);
                    }
                }
            }
            Ok(())
        })();
        self.update_result_buttons();
        applied
    }
    fn clear_compare(&mut self) {
        for doc in &self.documents {
            self.scratch.attach(&doc.handle);
            self.scratch.clear_diff();
        }
        self.differences.clear();
        self.comparing = false;
        self.compare_rx = None;
        self.compare_due = None;
        self.compare_jump = false;
        self.compare_ranges = None;
        self.compare_leading = [0; 2];
        self.compare_top = [0; 2];
        self.compare_generation = self.compare_generation.wrapping_add(1);
        for editor in self.editors {
            editor.send(SCI_SETWRAPMODE, self.wrap as usize, 0);
        }
        self.layout();
    }
    fn start_compare(&mut self) -> Result<()> {
        if self.documents.len() < 2 {
            return Err("Open two documents in separate tabs to compare.".into());
        }
        let left = self.index();
        if let Some(right) = self.secondary.filter(|right| *right != self.primary) {
            self.begin_compare(self.primary, right)
        } else {
            let group = &self.groups[self.focused];
            let current = group
                .iter()
                .position(|id| *id == self.documents[left].snapshot.id)
                .unwrap();
            let id = group[(current + 1) % group.len()];
            let right = self
                .documents
                .iter()
                .position(|doc| doc.snapshot.id == id)
                .unwrap();
            self.begin_compare(left, right)
        }
    }
    fn begin_compare(&mut self, left: usize, right: usize) -> Result<()> {
        if left == right || left >= self.documents.len() || right >= self.documents.len() {
            return Err("Choose two different documents to compare.".into());
        }
        self.compare_options.validate()?;
        self.clear_compare();
        let left_id = self.documents[left].snapshot.id;
        let right_id = self.documents[right].snapshot.id;
        self.groups[1].retain(|id| *id != left_id);
        self.groups[0].retain(|id| *id != right_id);
        if !self.groups[0].contains(&left_id) {
            self.groups[0].push(left_id);
        }
        if !self.groups[1].contains(&right_id) {
            self.groups[1].push(right_id);
        }
        self.primary = left;
        self.focused = 0;
        self.secondary = Some(right);
        self.normalize_groups();
        self.refresh_views()?;
        self.update_tabs();
        self.editor().focus();
        self.touch();
        self.comparing = true;
        self.compare_jump = true;
        self.launch_compare()
    }
    fn compare_selection(&mut self) -> Result<()> {
        if self.secondary.is_none() || self.secondary == Some(self.primary) {
            return Err(
                "Open two different documents in split view and select text in both panes.".into(),
            );
        }
        let mut ranges = Vec::new();
        for editor in self.editors {
            if editor.send(SCI_GETSELECTIONS, 0, 0) != 1 {
                return Err(
                    "Selection comparison requires one contiguous selection in each pane.".into(),
                );
            }
            let selected = editor.selection();
            if selected.is_empty() {
                return Err("Select text in both panes before comparing selected lines.".into());
            }
            let first = editor.send(SCI_LINEFROMPOSITION, selected.start, 0).max(0) as usize;
            let last = editor
                .send(SCI_LINEFROMPOSITION, selected.end.saturating_sub(1), 0)
                .max(0) as usize;
            let start = editor.send(SCI_POSITIONFROMLINE, first, 0).max(0) as usize;
            let end = editor.send(SCI_POSITIONFROMLINE, last + 1, 0);
            ranges.push((
                start..if end < 0 {
                    editor.length()
                } else {
                    end as usize
                },
                first,
            ));
        }
        self.clear_compare();
        self.compare_ranges = Some([ranges.remove(0), ranges.remove(0)]);
        self.comparing = true;
        self.compare_jump = true;
        self.launch_compare()
    }
    fn compare_snapshot(&mut self, saved: bool) -> Result<()> {
        let original = self.index();
        let source = self.documents[original].snapshot.clone();
        let text = if saved {
            let path = source
                .path
                .as_ref()
                .ok_or("Save this document before comparing with its on-disk version.")?;
            let bytes = session::read_bounded(path, core::MAX_TOOL_BYTES)?;
            core::decode(&bytes, Some(&source.encoding))?.0
        } else {
            clipboard_text(self.hwnd)?
        };
        self.clear_compare();
        self.add_document(DocumentSnapshot {
            id: 0,
            title: if saved {
                format!("Saved: {}", source.title)
            } else {
                "Clipboard comparison".into()
            },
            path: None,
            eol: Eol::detect(&text),
            text,
            encoding: source.encoding,
            language: source.language,
            dirty: false,
            disk_hash: None,
            caret: 0,
            pinned: false,
        })?;
        self.begin_compare(original, self.documents.len() - 1)
    }
    fn change_compare_options(&mut self, command: usize) -> Result<()> {
        let mut options = self.compare_options.clone();
        match command {
            COMPARE_IGNORE_SPACE => options.ignore_whitespace = !options.ignore_whitespace,
            COMPARE_IGNORE_CASE => options.ignore_case = !options.ignore_case,
            COMPARE_IGNORE_EMPTY => options.ignore_empty_lines = !options.ignore_empty_lines,
            COMPARE_MOVES => options.detect_moves = !options.detect_moves,
            COMPARE_ALIGN => options.align = !options.align,
            COMPARE_REGEX => {
                options.ignore_regex = window_text(self.search.query);
                if options.ignore_regex.is_empty() {
                    self.show_search()?;
                    self.note("Enter an expression in Find, then choose Tools > Comparison options > Use current Find expression as ignore regex.");
                    return Ok(());
                }
            }
            COMPARE_REGEX_CLEAR => options.ignore_regex.clear(),
            _ => unreachable!(),
        }
        options.validate()?;
        self.compare_options = options;
        self.compare_generation = self.compare_generation.wrapping_add(1);
        if self.comparing {
            for editor in self.editors {
                editor.clear_diff();
                editor.send(
                    SCI_SETWRAPMODE,
                    (self.wrap && !self.compare_options.align) as usize,
                    0,
                );
            }
            self.compare_leading = [0; 2];
            self.compare_top = [0; 2];
            self.compare_due = Some(Instant::now());
            self.layout();
        }
        self.update_symbol_checks();
        self.touch();
        Ok(())
    }
    fn launch_compare(&mut self) -> Result<()> {
        if self.compare_rx.is_some() {
            return Ok(());
        }
        let Some(right_index) = self.secondary else {
            return Ok(());
        };
        let left = &self.documents[self.primary];
        let right = &self.documents[right_index];
        let left_id = left.snapshot.id;
        let right_id = right.snapshot.id;
        let left_rev = left.revision;
        let right_rev = right.revision;
        let (left_text, right_text) = if let Some(ranges) = &self.compare_ranges {
            (
                self.editors[0].range(ranges[0].0.clone())?,
                self.editors[1].range(ranges[1].0.clone())?,
            )
        } else {
            (
                self.tool_text(self.editors[0])?,
                self.tool_text(self.editors[1])?,
            )
        };
        let ranges = self.compare_ranges.clone();
        let options = self.compare_options.clone();
        let generation = self.compare_generation;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result =
                comparison::compare(&left_text, &right_text, &options).map(|mut result| {
                    if let Some(ranges) = ranges {
                        result.offset(
                            ranges[0].0.start,
                            ranges[0].1,
                            ranges[1].0.start,
                            ranges[1].1,
                        );
                    }
                    result
                });
            let _ = tx.send(CompareResult {
                left: left_id,
                right: right_id,
                left_rev,
                right_rev,
                generation,
                result,
            });
        });
        self.compare_rx = Some(rx);
        self.compare_due = None;
        self.note("Comparing in background...");
        Ok(())
    }
    fn apply_compare(&mut self, result: CompareResult) -> Result<()> {
        let Some(right) = self.secondary else {
            return Ok(());
        };
        let left = self.primary;
        if self.documents[left].snapshot.id != result.left
            || self.documents[right].snapshot.id != result.right
            || self.documents[left].revision != result.left_rev
            || self.documents[right].revision != result.right_rev
            || self.compare_generation != result.generation
        {
            if self.comparing {
                self.compare_due = Some(Instant::now() + Duration::from_millis(300));
            }
            return Ok(());
        }
        let comparison = result.result?;
        let virtual_row = if self.compare_jump {
            0
        } else {
            let pane = self.focused;
            (self.editors[pane]
                .send(SCI_GETFIRSTVISIBLELINE, 0, 0)
                .max(0) as usize
                + self.compare_leading[pane])
                .saturating_sub(self.compare_top[pane])
        };
        for editor in self.editors {
            editor.clear_diff();
        }
        self.differences = comparison.differences;
        self.comparing = true;
        for (pane, padding) in [comparison.left_padding, comparison.right_padding]
            .iter()
            .enumerate()
        {
            self.editors[pane].send(
                SCI_SETWRAPMODE,
                (self.wrap && !self.compare_options.align) as usize,
                0,
            );
            self.compare_leading[pane] = if self.compare_options.align {
                self.editors[pane].apply_compare_padding(padding)?
            } else {
                0
            };
            self.compare_top[pane] = self.compare_leading[pane].saturating_sub(virtual_row);
            self.editors[pane].send(
                SCI_SETFIRSTVISIBLELINE,
                virtual_row.saturating_sub(self.compare_leading[pane]),
                0,
            );
        }
        for difference in &self.differences {
            for line in difference.left.clone() {
                self.editors[0].send(SCI_MARKERADD, line, 20);
            }
            for line in difference.right.clone() {
                self.editors[1].send(SCI_MARKERADD, line, 21);
            }
            for range in &difference.left_inline {
                self.editors[0].indicator(20, range.clone())?;
            }
            for range in &difference.right_inline {
                self.editors[1].indicator(21, range.clone())?;
            }
        }
        for moved in &comparison.moves {
            self.editors[0].send(SCI_MARKERADD, moved.left, 22);
            self.editors[1].send(SCI_MARKERADD, moved.right, 22);
        }
        self.layout();
        self.note(format!(
            "{} difference groups, {} moved lines. Red: left; green: right; blue: moved. F7 to navigate.",
            self.differences.len(),comparison.moves.len()
        ));
        if self.compare_jump && !self.differences.is_empty() {
            self.difference = self.differences.len() - 1;
            self.navigate_difference(false);
        }
        self.compare_jump = false;
        self.difference = self
            .difference
            .min(self.differences.len().saturating_sub(1));
        Ok(())
    }
    fn navigate_difference(&mut self, previous: bool) {
        if self.differences.is_empty() {
            return;
        }
        self.difference = if previous {
            (self.difference + self.differences.len() - 1) % self.differences.len()
        } else {
            (self.difference + 1) % self.differences.len()
        };
        let difference = &self.differences[self.difference];
        for (editor, line) in [
            (self.editors[0], difference.left.start),
            (self.editors[1], difference.right.start),
        ] {
            let line = line.min(editor.send(SCI_GETLINECOUNT, 0, 0).saturating_sub(1) as usize);
            editor.send(SCI_GOTOLINE, line, 0);
            editor.send(SCI_ENSUREVISIBLEENFORCEPOLICY, line, 0);
            editor.send(SCI_SETFIRSTVISIBLELINE, line.saturating_sub(4), 0);
        }
        self.note(format!(
            "Difference {} of {}",
            self.difference + 1,
            self.differences.len()
        ));
    }
    fn schedule_json(&mut self) {
        self.json_document = None;
        self.json_due = Some(Instant::now() + Duration::from_millis(350));
        unsafe {
            EnableWindow(self.tree, 0);
        }
    }
    fn refresh_json(&mut self) -> Result<()> {
        self.tree_visible = true;
        self.schedule_json();
        self.json_due = Some(Instant::now());
        self.layout();
        self.launch_json()
    }
    fn launch_json(&mut self) -> Result<()> {
        if self.json_rx.is_some() {
            return Ok(());
        }
        let doc = &self.documents[self.index()];
        let document = doc.snapshot.id;
        let revision = doc.revision;
        let text = self.tool_text(self.editor())?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(JsonResult {
                document,
                revision,
                result: core::json_tree(&text),
            });
        });
        self.json_rx = Some(rx);
        self.json_due = None;
        self.note("Updating JSON tree...");
        Ok(())
    }
    fn apply_json(&mut self, result: JsonResult) -> Result<()> {
        let doc = &self.documents[self.index()];
        if !self.tree_visible {
            return Ok(());
        }
        if result.document != doc.snapshot.id || result.revision != doc.revision {
            self.schedule_json();
            return Ok(());
        }
        let nodes = match result.result {
            Ok(nodes) => nodes,
            Err(error) => {
                self.json_document = None;
                self.note(format!("JSON paused: {error}"));
                return Ok(());
            }
        };
        let mut expanded = HashSet::new();
        let selected = unsafe { SendMessageW(self.tree, TVM_GETNEXTITEM, TVGN_CARET as usize, 0) };
        let mut selected_pointer = None;
        for (node, handle) in self.json_nodes.iter().zip(&self.json_handles) {
            if unsafe {
                SendMessageW(
                    self.tree,
                    TVM_GETITEMSTATE,
                    *handle as usize,
                    TVIS_EXPANDED as isize,
                )
            } & TVIS_EXPANDED as isize
                != 0
            {
                expanded.insert(node.pointer.clone());
            }
            if *handle == selected {
                selected_pointer = Some(node.pointer.clone());
            }
        }
        let mut handles = Vec::new();
        unsafe {
            TREE_UPDATING.with(|flag| flag.set(true));
            SendMessageW(self.tree, WM_SETREDRAW, 0, 0);
            SendMessageW(self.tree, TVM_DELETEITEM, 0, TVI_ROOT);
            for (i, node) in nodes.iter().enumerate() {
                let mut label = wide(&node.label);
                let mut item: TVINSERTSTRUCTW = zeroed();
                item.hParent = node.parent.map(|p| handles[p]).unwrap_or(TVI_ROOT);
                item.hInsertAfter = TVI_LAST;
                item.Anonymous.item = TVITEMW {
                    mask: TVIF_TEXT | TVIF_PARAM,
                    pszText: label.as_mut_ptr(),
                    lParam: i as isize,
                    ..zeroed()
                };
                let handle = SendMessageW(
                    self.tree,
                    TVM_INSERTITEMW,
                    0,
                    (&item as *const TVINSERTSTRUCTW) as isize,
                ) as HTREEITEM;
                if handle == 0 {
                    SendMessageW(self.tree, WM_SETREDRAW, 1, 0);
                    TREE_UPDATING.with(|flag| flag.set(false));
                    self.json_document = None;
                    return Err("Could not allocate a JSON tree item.".into());
                }
                handles.push(handle);
                if expanded.contains(&node.pointer) {
                    SendMessageW(self.tree, TVM_EXPAND, TVE_EXPAND as usize, handle);
                }
                if selected_pointer.as_ref() == Some(&node.pointer) {
                    SendMessageW(self.tree, TVM_SELECTITEM, TVGN_CARET as usize, handle);
                }
            }
            if let Some(root) = handles.first() {
                SendMessageW(self.tree, TVM_EXPAND, TVE_EXPAND as usize, *root);
            }
            SendMessageW(self.tree, WM_SETREDRAW, 1, 0);
            EnableWindow(self.tree, 1);
            TREE_UPDATING.with(|flag| flag.set(false));
            InvalidateRect(self.tree, null(), 1);
        }
        self.json_nodes = nodes;
        self.json_handles = handles;
        self.json_document = Some(result.document);
        self.tree_visible = true;
        self.layout();
        self.note("Live JSON tree: select a node to jump to its value.");
        Ok(())
    }
    fn definition_text(path: &Path) -> Result<String> {
        let bytes = session::read_bounded(path, 1024 * 1024)?;
        let (text, encoding) = core::decode(&bytes, None)?;
        if matches!(encoding, Encoding::Legacy(_)) {
            return Err("Definitions must be UTF-8 or BOM-marked Unicode.".into());
        }
        Ok(text)
    }
    fn import_language(&mut self, path: &Path) -> Result<()> {
        let definitions = udl::import(&Self::definition_text(path)?)?;
        let mut languages = self.languages.clone();
        let mut changed = Vec::new();
        for definition in definitions {
            changed.push(languages::add_custom(&mut languages, definition)?);
        }
        self.languages = languages;
        for doc in &mut self.documents {
            if doc.language == 0
                && let Some(path) = &doc.snapshot.path
            {
                doc.language = languages::detect(path, &self.languages);
                doc.snapshot.language = self.languages[doc.language].name.clone();
            }
            if changed.contains(&doc.language) {
                doc.snapshot.language = self.languages[doc.language].name.clone();
                doc.revision += 1;
                doc.styled_revision = None;
            }
        }
        self.make_menu()?;
        self.apply_theme();
        if !self.documents.is_empty() {
            self.refresh_views()?;
        }
        self.touch();
        self.note(format!(
            "Imported {} data-only language definition(s).",
            changed.len()
        ));
        Ok(())
    }
    fn import_api(&mut self, path: &Path) -> Result<()> {
        let language = &self.languages[self.documents[self.index()].language].name;
        let entries = completion::import_api(&Self::definition_text(path)?, language)?;
        let mut api = self.completion_api.clone();
        api.retain(|old| {
            !entries.iter().any(|new| {
                new.language == old.language && new.receiver == old.receiver && new.name == old.name
            })
        });
        api.extend(entries);
        if api.len() > 10_000 {
            return Err("The completion catalog is limited to 10,000 imported overloads.".into());
        }
        self.completion_api = api;
        self.touch();
        self.note("Completion signatures imported for the active language.");
        Ok(())
    }
    fn remove_language(&mut self) -> Result<()> {
        let language = self.documents[self.index()].language;
        if self.languages[language].custom.is_none() {
            return Err("Select a user-defined language first.".into());
        }
        let removed = self.languages.remove(language);
        self.completion_api
            .retain(|api| api.language != removed.name);
        for doc in &mut self.documents {
            if doc.language == language {
                doc.language = 0;
                doc.snapshot.language = self.languages[0].name.clone();
                doc.revision += 1;
                doc.styled_revision = None;
            } else if doc.language > language {
                doc.language -= 1;
            }
        }
        self.make_menu()?;
        self.refresh_views()?;
        self.apply_theme();
        self.touch();
        self.note("User-defined language removed; document text is unchanged.");
        Ok(())
    }
    fn launch_highlight(&mut self) -> Result<()> {
        if self.highlight_rx.is_some() {
            return Ok(());
        }
        let index = [Some(self.primary), self.secondary]
            .into_iter()
            .flatten()
            .find(|index| {
                let doc = &self.documents[*index];
                self.languages[doc.language].uses_container()
                    && doc.styled_revision != Some(doc.revision)
                    && doc.last_edit.elapsed() >= Duration::from_millis(200)
            });
        let Some(index) = index else { return Ok(()) };
        let doc = &self.documents[index];
        let definition = self.languages[doc.language].clone();
        let document = doc.snapshot.id;
        let revision = doc.revision;
        let language = definition.name.clone();
        self.scratch.attach(&doc.handle);
        let text = self.scratch.text()?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(HighlightResult {
                document,
                revision,
                language,
                result: definition.highlight(&text),
            });
        });
        self.highlight_rx = Some(rx);
        Ok(())
    }
    fn line_operation(&mut self, operation: LineOp) -> Result<()> {
        let editor = self.editor();
        let selection = editor.selection();
        let range = if selection.is_empty() {
            0..editor.length()
        } else {
            let first = editor.send(SCI_LINEFROMPOSITION, selection.start, 0) as usize;
            let last_position = if selection.end > selection.start {
                editor.send(SCI_POSITIONBEFORE, selection.end, 0) as usize
            } else {
                selection.end
            };
            let last = editor.send(SCI_LINEFROMPOSITION, last_position, 0) as usize;
            let start = editor.send(SCI_POSITIONFROMLINE, first, 0) as usize;
            let end = editor.send(SCI_POSITIONFROMLINE, last + 1, 0);
            start..if end < 0 {
                editor.length()
            } else {
                end as usize
            }
        };
        if range.len() > core::MAX_TOOL_BYTES {
            return Err("Line operations are limited to 16 MiB.".into());
        }
        let out = core::lines(
            &editor.range(range.clone())?,
            operation,
            self.documents[self.index()].snapshot.eol,
        )?;
        editor.replace(range.clone(), &out)?;
        editor.select(range.start..range.start + out.len());
        Ok(())
    }
    fn command(&mut self, command: usize) -> Result<()> {
        let results_focused = unsafe { GetFocus() } == self.results.editor.hwnd;
        if results_focused
            && matches!(
                command,
                UNDO | REDO
                    | CUT
                    | PASTE
                    | DUPLICATE
                    | DELETE_LINE
                    | UPPER
                    | LOWER
                    | TITLE_CASE
                    | SENTENCE_CASE
                    | INVERT_CASE
                    | SORT
                    | SORT_DESC
                    | UNIQUE
                    | TRIM
                    | REMOVE_EMPTY
                    | SORT_IGNORE_CASE
                    | SORT_DESC_IGNORE_CASE
                    | SORT_NATURAL
                    | SORT_NUMERIC
                    | SORT_NUMERIC_DESC
                    | SORT_NUMERIC_COMMA
                    | REVERSE_LINES
                    | UNIQUE_ADJACENT
                    | TRIM_START
                    | TRIM_BOTH
                    | REMOVE_EMPTY_ONLY
                    | JOIN_LINES
                    | ADD_NEXT
                    | SELECT_MATCHES
            )
        {
            self.note("Search results are read-only. Focus an editing pane to modify a document.");
            return Ok(());
        }
        let editor = if results_focused && matches!(command, COPY | SELECT_ALL) {
            self.results.editor
        } else {
            self.editor()
        };
        match command {
            NEW => self.new_document()?,
            OPEN => {
                if let Some(path) = self.dialog(false)? {
                    self.open_path(&path, None)?;
                }
            }
            SAVE => {
                self.save_document(false)?;
            }
            SAVE_AS => {
                self.save_document(true)?;
            }
            CLOSE => self.close_document()?,
            MONITOR_FILES => {
                self.monitor_files = !self.monitor_files;
                if self.monitor_files {
                    self.monitor.reset();
                }
                self.update_symbol_checks();
                self.touch();
            }
            EXIT => self.close()?,
            UNDO | REDO | CUT | COPY | PASTE | SELECT_ALL | DUPLICATE | DELETE_LINE => {
                let message = match command {
                    UNDO => SCI_UNDO,
                    REDO => SCI_REDO,
                    CUT => SCI_CUT,
                    COPY => SCI_COPY,
                    PASTE => SCI_PASTE,
                    SELECT_ALL => SCI_SELECTALL,
                    DUPLICATE => SCI_SELECTIONDUPLICATE,
                    _ => SCI_LINEDELETE,
                };
                editor.send(message, 0, 0);
            }
            UPPER | LOWER | TITLE_CASE | SENTENCE_CASE | INVERT_CASE => {
                let operation = match command {
                    UPPER => CaseOp::Upper,
                    LOWER => CaseOp::Lower,
                    TITLE_CASE => CaseOp::Title,
                    SENTENCE_CASE => CaseOp::Sentence,
                    _ => CaseOp::Invert,
                };
                editor.transform_selections(|text| core::change_case(text, operation))?;
            }
            SORT => self.line_operation(LineOp::Sort)?,
            SORT_DESC => self.line_operation(LineOp::SortDescending)?,
            UNIQUE => self.line_operation(LineOp::Unique)?,
            TRIM => self.line_operation(LineOp::Trim)?,
            REMOVE_EMPTY => self.line_operation(LineOp::RemoveEmpty)?,
            SORT_IGNORE_CASE => self.line_operation(LineOp::SortIgnoreCase)?,
            SORT_DESC_IGNORE_CASE => self.line_operation(LineOp::SortDescendingIgnoreCase)?,
            SORT_NATURAL => self.line_operation(LineOp::SortNatural)?,
            SORT_NUMERIC => self.line_operation(LineOp::SortNumeric)?,
            SORT_NUMERIC_DESC => self.line_operation(LineOp::SortNumericDescending)?,
            SORT_NUMERIC_COMMA => self.line_operation(LineOp::SortNumericComma)?,
            REVERSE_LINES => self.line_operation(LineOp::Reverse)?,
            UNIQUE_ADJACENT => self.line_operation(LineOp::UniqueAdjacent)?,
            TRIM_START => self.line_operation(LineOp::TrimStart)?,
            TRIM_BOTH => self.line_operation(LineOp::TrimBoth)?,
            REMOVE_EMPTY_ONLY => self.line_operation(LineOp::RemoveEmptyOnly)?,
            JOIN_LINES => self.line_operation(LineOp::Join)?,
            PARAMETER_HINT => editor.call_tip(
                &self.languages[self.documents[self.index()].language],
                &self.completion_api,
            )?,
            IMPORT_LANGUAGE => {
                if let Some(path) = self.dialog(false)? {
                    self.import_language(&path)?;
                }
            }
            IMPORT_API => {
                if let Some(path) = self.dialog(false)? {
                    self.import_api(&path)?;
                }
            }
            REMOVE_LANGUAGE => self.remove_language()?,
            ADD_NEXT => {
                editor.send(SCI_MULTIPLESELECTADDNEXT, 0, 0);
            }
            SELECT_MATCHES => {
                editor.send(SCI_MULTIPLESELECTADDEACH, 0, 0);
            }
            FIND => {
                self.search.folder_visible = false;
                self.show_search()?;
            }
            FIND_FILES => self.show_folder_search()?,
            FIND_FILES_BROWSE => self.browse_folder()?,
            FIND_FILES_RUN => self.start_find_files()?,
            SEARCH_CLOSE => {
                self.search.visible = false;
                self.layout();
                editor.focus();
            }
            FIND_NEXT => self.find(false)?,
            FIND_PREVIOUS => self.find(true)?,
            FIND_ALL_CURRENT => self.start_find_all(false)?,
            FIND_ALL_OPEN => self.start_find_all(true)?,
            RESULTS_TOGGLE => {
                self.results.visible = !self.results.visible;
                self.layout();
                if self.results.visible {
                    self.results.editor.focus();
                } else {
                    self.editor().focus();
                }
            }
            RESULTS_NEXT => self.next_result(false)?,
            RESULTS_PREVIOUS => self.next_result(true)?,
            RESULTS_CLEAR => self.clear_find_results()?,
            RESULTS_CLOSE => {
                self.results.visible = false;
                self.layout();
                self.editor().focus();
            }
            RESULTS_CANCEL => {
                if let Some(job) = &self.results.job {
                    job.cancelled.store(true, Ordering::Relaxed);
                    self.result_message("Cancelling search...");
                }
            }
            REPLACE => self.replace(false)?,
            REPLACE_ALL => self.replace(true)?,
            SPLIT => {
                self.clear_compare();
                if self.secondary.is_some() {
                    for id in std::mem::take(&mut self.groups[1]) {
                        if !self.groups[0].contains(&id) {
                            self.groups[0].push(id);
                        }
                    }
                    self.primary = self.index();
                    self.secondary = None;
                    self.focused = 0;
                } else {
                    self.split_tab(self.documents[self.index()].snapshot.id)?;
                }
                self.normalize_groups();
                self.refresh_views()?;
                self.update_tabs();
                self.editor().focus();
                self.touch();
                self.note(
                    "Each pane has its own tabs. Open in split moves a tab; Clone explicitly shares it. F6 switches panes.",
                );
            }
            TAB_SPLIT => self.split_tab(self.documents[self.index()].snapshot.id)?,
            TAB_CLONE => self.transfer_tab(self.documents[self.index()].snapshot.id, true)?,
            TAB_PIN => self.reorder_tabs(self.documents[self.index()].snapshot.id, None)?,
            TAB_LEFT | TAB_RIGHT => {
                let id = self.documents[self.index()].snapshot.id;
                let group = &self.groups[self.focused];
                let position = group.iter().position(|entry| *entry == id).unwrap();
                let target = if command == TAB_LEFT {
                    position.saturating_sub(1)
                } else {
                    (position + 1).min(group.len() - 1)
                };
                self.reorder_tabs(id, Some(target))?;
            }
            MAP => {
                self.map_visible = !self.map_visible;
                self.configure_map();
                self.layout();
            }
            WRAP => {
                self.wrap = !self.wrap;
                for ed in self.editors {
                    ed.send(
                        SCI_SETWRAPMODE,
                        (self.wrap && !(self.comparing && self.compare_options.align)) as usize,
                        0,
                    );
                }
            }
            SYMBOL_SPACE | SYMBOL_EOL | SYMBOL_NONPRINTING | SYMBOL_CONTROLS | SYMBOL_ALL
            | SYMBOL_INDENT | SYMBOL_WRAP => {
                match command {
                    SYMBOL_SPACE => self.show_symbols.whitespace = !self.show_symbols.whitespace,
                    SYMBOL_EOL => self.show_symbols.eol = !self.show_symbols.eol,
                    SYMBOL_NONPRINTING => {
                        self.show_symbols.non_printing = !self.show_symbols.non_printing
                    }
                    SYMBOL_CONTROLS => self.show_symbols.controls = !self.show_symbols.controls,
                    SYMBOL_ALL => self.show_symbols.toggle_all(),
                    SYMBOL_INDENT => {
                        self.show_symbols.indent_guides = !self.show_symbols.indent_guides
                    }
                    SYMBOL_WRAP => self.show_symbols.wrap_markers = !self.show_symbols.wrap_markers,
                    _ => unreachable!(),
                }
                self.apply_symbols();
                self.touch();
            }
            ZOOM_RESET => {
                for ed in self.editors {
                    ed.send(SCI_SETZOOM, 0, 0);
                }
            }
            EDITOR_FONT => {
                let selection = choose_editor_font(self.hwnd, self.dpi, &self.editor_font)?;
                self.set_editor_font(selection);
            }
            THEME_SYSTEM | THEME_LIGHT | THEME_DARK => {
                self.theme = match command {
                    THEME_LIGHT => "light",
                    THEME_DARK => "dark",
                    _ => "system",
                }
                .into();
                self.apply_theme();
                self.touch();
            }
            COMPARE => self.start_compare()?,
            COMPARE_SELECTION => self.compare_selection()?,
            COMPARE_CLIPBOARD => self.compare_snapshot(false)?,
            COMPARE_SAVED => self.compare_snapshot(true)?,
            COMPARE_IGNORE_SPACE | COMPARE_IGNORE_CASE | COMPARE_IGNORE_EMPTY | COMPARE_MOVES
            | COMPARE_ALIGN | COMPARE_REGEX | COMPARE_REGEX_CLEAR => {
                self.change_compare_options(command)?
            }
            COMPARE_CLEAR => {
                self.clear_compare();
                self.note("Compare cleared.");
            }
            DIFF_NEXT => self.navigate_difference(false),
            DIFF_PREVIOUS => self.navigate_difference(true),
            JSON_FORMAT | JSON_COMPACT => {
                let text = core::format_json(
                    &self.tool_text(editor)?,
                    command == JSON_COMPACT,
                    self.documents[self.index()].snapshot.eol,
                )?;
                editor.replace_all(&text)?;
            }
            JSON_TREE => {
                if self.tree_visible {
                    self.tree_visible = false;
                    self.json_due = None;
                    self.json_rx = None;
                    self.layout();
                } else {
                    self.refresh_json()?;
                }
            }
            JSON_REFRESH => self.refresh_json()?,
            EOL_CRLF | EOL_LF | EOL_CR => {
                let eol = match command {
                    EOL_LF => Eol::Lf,
                    EOL_CR => Eol::Cr,
                    _ => Eol::CrLf,
                };
                editor.send(SCI_CONVERTEOLS, eol.scintilla(), 0);
                editor.send(SCI_SETEOLMODE, eol.scintilla(), 0);
                let index = self.index();
                self.documents[index].snapshot.eol = eol;
                self.documents[index].metadata_dirty = true;
                self.documents[index].snapshot.dirty = true;
                self.touch();
                self.update_tabs();
                self.update_status();
            }
            ABOUT => {
                ask(
                    self.hwnd,
                    concat!(
                        "rstpd ",
                        env!("CARGO_PKG_VERSION"),
                        "\nRust application with statically linked Scintilla + Lexilla.\nNo plugin loader, script execution, network service, or automatic updater.\n\nAlt+drag: rectangular selection\nCtrl+click: multiple carets\nCtrl+D / Ctrl+Shift+L: next / all occurrences\nF6: focus other pane; Ctrl+Tab: next tab\nCtrl+Space: contextual completion\nCtrl+Shift+Space: function parameter hint\nCtrl+mouse wheel: zoom\nCtrl+Alt+J: format JSON/JSON5; Ctrl+Alt+T: live JSON tree\nF7 / Shift+F7: next / previous difference\n\nCompare and JSON refresh automatically after edits.\nLanguage menu: import data-only language/API XML.\nUnsaved tabs recover when the app reopens. Close the app to keep them.\nRecovery is local plaintext; existing recovery folders are preserved.\nSee README for current and legacy recovery locations.\nRegex supports look-around/backreferences ($1 or ${name} replacements).\nFiles: 256 MiB; search/replace: 128 MiB; line and JSON tools: 16 MiB.\n\nIndependent application; not affiliated with Notepad++."
                    ),
                    MB_OK | MB_ICONINFORMATION,
                );
            }
            id if (ENCODING_BASE..ENCODING_BASE + encoding_options().len()).contains(&id) => {
                let encoding = encoding_options()[id - ENCODING_BASE].clone();
                encoding.encode(&editor.text()?)?;
                let index = self.index();
                self.documents[index].snapshot.encoding = encoding;
                self.documents[index].metadata_dirty = true;
                self.documents[index].snapshot.dirty = true;
                self.touch();
                self.update_tabs();
                self.note("Encoding will be applied on Save.");
            }
            id if (REOPEN_BASE..REOPEN_BASE + encoding_options().len()).contains(&id) => {
                let index = self.index();
                let path = self.documents[index]
                    .snapshot
                    .path
                    .clone()
                    .ok_or("This document has no file to reopen.")?;
                if ask(
                    self.hwnd,
                    "Reload the file using this encoding? Current edits will be discarded.",
                    MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
                ) == IDYES
                {
                    let bytes = session::read_bounded(&path, core::MAX_DOCUMENT_BYTES)?;
                    let (text, encoding) =
                        core::decode(&bytes, Some(&encoding_options()[id - REOPEN_BASE]))?;
                    editor.set_text(&text)?;
                    let doc = &mut self.documents[index];
                    doc.snapshot.encoding = encoding;
                    doc.snapshot.eol = Eol::detect(&text);
                    doc.snapshot.disk_hash = Some(session::fingerprint(&bytes));
                    doc.snapshot.dirty = false;
                    doc.base_dirty = false;
                    doc.metadata_dirty = false;
                    doc.revision += 1;
                    self.touch();
                    self.refresh_views()?;
                    self.update_tabs();
                }
            }
            id if id >= LANGUAGE_BASE && id < LANGUAGE_BASE + self.languages.len() => {
                let index = self.index();
                self.documents[index].language = id - LANGUAGE_BASE;
                self.documents[index].revision += 1;
                self.documents[index].styled_revision = None;
                self.documents[index].snapshot.language =
                    self.languages[id - LANGUAGE_BASE].name.clone();
                self.refresh_views()?;
                self.touch();
                self.update_status();
            }
            _ => {}
        }
        Ok(())
    }
    fn key(&mut self, message: &MSG) -> Result<bool> {
        if message.message != WM_KEYDOWN && message.message != WM_SYSKEYDOWN {
            return Ok(false);
        }
        let control = unsafe { GetKeyState(VK_CONTROL as i32) } < 0;
        let shift = unsafe { GetKeyState(VK_SHIFT as i32) } < 0;
        let alt = unsafe { GetKeyState(VK_MENU as i32) } < 0;
        let key = message.wParam as u16;
        let result_focus = unsafe { GetFocus() } == self.results.editor.hwnd;
        if result_focus && !control && !alt {
            if key == VK_RETURN {
                self.activate_result_row()?;
                return Ok(true);
            }
            if key == VK_ESCAPE {
                self.command(RESULTS_CLOSE)?;
                return Ok(true);
            }
        }
        let command = match (control, shift, alt, key) {
            (true, false, false, 0x4e) => Some(NEW),
            (true, false, false, 0x4f) => Some(OPEN),
            (true, false, false, 0x53) => Some(SAVE),
            (true, true, false, 0x53) => Some(SAVE_AS),
            (true, false, false, 0x57) => Some(CLOSE),
            (true, false, false, 0x46 | 0x48) => Some(FIND),
            (true, true, false, 0x46) => Some(FIND_FILES),
            (true, false, false, 0x44) => Some(ADD_NEXT),
            (true, true, false, 0x4c) => Some(SELECT_MATCHES),
            (true, true, false, 0x55) => Some(UPPER),
            (true, false, false, 0x55) => Some(LOWER),
            (true, false, true, 0x4a) => Some(JSON_FORMAT),
            (true, false, true, 0x54) => Some(JSON_TREE),
            (true, false, true, VK_RIGHT) => Some(SPLIT),
            (false, false, false, VK_F3) => Some(FIND_NEXT),
            (false, true, false, VK_F3) => Some(FIND_PREVIOUS),
            (false, false, false, VK_F4) => Some(RESULTS_NEXT),
            (false, true, false, VK_F4) => Some(RESULTS_PREVIOUS),
            (true, false, true, VK_RETURN) => Some(FIND_ALL_CURRENT),
            (true, true, false, VK_RETURN) => Some(FIND_ALL_OPEN),
            (true, false, true, 0x52) => Some(RESULTS_TOGGLE),
            (false, false, false, VK_F7) => Some(DIFF_NEXT),
            (false, true, false, VK_F7) => Some(DIFF_PREVIOUS),
            (false, false, false, VK_ESCAPE) if self.search.visible => Some(SEARCH_CLOSE),
            (false, false, false, VK_RETURN)
                if self.search.visible && unsafe { GetFocus() } == self.search.query =>
            {
                Some(if self.search.folder_visible {
                    FIND_FILES_RUN
                } else {
                    FIND_NEXT
                })
            }
            _ => None,
        };
        if let Some(command) = command {
            self.command(command)?;
            return Ok(true);
        }
        if control && key == VK_TAB {
            let indices: Vec<_> = self.groups[self.focused]
                .iter()
                .map(|id| {
                    self.documents
                        .iter()
                        .position(|doc| doc.snapshot.id == *id)
                        .unwrap()
                })
                .collect();
            let current = indices.iter().position(|i| *i == self.index()).unwrap_or(0);
            let next = (current + if shift { indices.len() - 1 } else { 1 }) % indices.len();
            self.switch(indices[next])?;
            return Ok(true);
        }
        if key == VK_F6 && self.secondary.is_some() {
            self.focus_pane(1 - self.focused);
            self.editor().focus();
            return Ok(true);
        }
        if control && key == VK_SPACE {
            if shift {
                self.editor().call_tip(
                    &self.languages[self.documents[self.index()].language],
                    &self.completion_api,
                )?;
            } else {
                self.editor().complete(
                    &self.languages[self.documents[self.index()].language],
                    &self.completion_api,
                    true,
                )?;
            }
            return Ok(true);
        }
        Ok(false)
    }
    fn tick(&mut self) -> Result<()> {
        self.poll_find_results()?;
        self.poll_monitor()?;
        while let Ok((revision, result)) = self.recovery.rx.try_recv() {
            self.recovery_busy = false;
            match result {
                Ok(()) => {
                    self.recovered_revision = revision;
                    self.recovery_error = None;
                }
                Err(error) => {
                    self.recovery_error = Some(error);
                }
            }
            self.update_status();
        }
        if let Some(rx) = &self.compare_rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.compare_rx = None;
                    if let Err(error) = self.apply_compare(result) {
                        self.note(format!("Compare paused: {error}"));
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.compare_rx = None;
                    return Err("Compare worker stopped unexpectedly.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.comparing
            && self.compare_rx.is_none()
            && self.compare_due.is_some_and(|due| Instant::now() >= due)
        {
            self.compare_due = None;
            if let Err(error) = self.launch_compare() {
                self.note(format!("Compare paused: {error}"));
            }
        }
        if let Some(rx) = &self.json_rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.json_rx = None;
                    self.apply_json(result)?;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.json_rx = None;
                    self.note("JSON worker stopped unexpectedly.");
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.tree_visible
            && self.json_rx.is_none()
            && self.json_due.is_some_and(|due| Instant::now() >= due)
        {
            self.json_due = None;
            if let Err(error) = self.launch_json() {
                self.note(format!("JSON paused: {error}"));
            }
        }
        if let Some(rx) = &self.highlight_rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.highlight_rx = None;
                    if let Some(index) = self.documents.iter().position(|doc| {
                        doc.snapshot.id == result.document
                            && doc.revision == result.revision
                            && self.languages[doc.language].uses_container()
                            && self.languages[doc.language].name == result.language
                    }) {
                        self.documents[index].styled_revision = Some(result.revision);
                        match result.result {
                            Ok(highlight) => {
                                self.scratch.attach(&self.documents[index].handle);
                                self.scratch.highlight(&highlight)?;
                            }
                            Err(error) => {
                                self.scratch.attach(&self.documents[index].handle);
                                self.scratch.clear_styles();
                                self.note(format!("Highlighting paused: {error}"));
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.highlight_rx = None;
                    return Err("Highlighting worker stopped unexpectedly.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        self.launch_highlight()?;
        if self.revision != self.recovered_revision
            && !self.recovery_busy
            && self.last_autosave.elapsed() >= Duration::from_secs(3)
        {
            let snapshot = self.snapshot()?;
            self.recovery.submit(self.revision, snapshot)?;
            self.recovery_busy = true;
            self.last_autosave = Instant::now();
            self.update_status();
        }
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        // Show why the window is about to freeze: flush blocks on a full recovery write.
        self.note("Saving recovery before exit...");
        unsafe {
            UpdateWindow(self.status);
        }
        let saved = self
            .snapshot()
            .and_then(|snapshot| self.recovery.flush(self.revision + 1, snapshot));
        if let Err(error) = saved {
            self.recovery_error = Some(error.clone());
            self.update_status();
            let unsaved = self
                .documents
                .iter()
                .filter(|doc| doc.snapshot.dirty)
                .count();
            if ask(
                self.hwnd,
                &format!(
                    "Recovery could not be saved: {error}\n\n\
                     Exit anyway? {unsaved} tab(s) with unsaved edits will be lost.\n\
                     Choose No to stay open and save them with File > Save as.",
                ),
                MB_YESNO | MB_ICONERROR | MB_DEFBUTTON2,
            ) != IDYES
            {
                return Ok(());
            }
        }
        if let Some(job) = &self.results.job {
            job.cancelled.store(true, Ordering::Relaxed);
        }
        self.exiting = true;
        self.documents.clear();
        unsafe {
            DestroyWindow(self.hwnd);
        }
        Ok(())
    }
    fn event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::RenderError(error) => return Err(error),
            Event::Command(command) => self.command(command)?,
            Event::Resize => self.layout(),
            Event::Close => self.close()?,
            Event::Tick => self.tick()?,
            Event::Theme if self.theme == "system" => self.apply_theme(),
            Event::Theme => {}
            Event::Dpi => {
                self.dpi = unsafe { GetDpiForWindow(self.hwnd) };
                self.set_font();
                self.layout();
            }
            Event::TabFocus(pane) => {
                let changed = self.focused != pane;
                self.focused = pane;
                self.editor().focus();
                select_pane(self.hwnd, pane);
                self.update_status();
                if changed {
                    self.touch();
                    if self.tree_visible {
                        self.schedule_json();
                    }
                }
            }
            Event::Tab(pane) => {
                let tab = [self.tabs, self.right_tabs][pane];
                let selected = unsafe { SendMessageW(tab, TCM_GETCURSEL, 0, 0) };
                if selected >= 0 {
                    let id = unsafe { tab_document_id(tab, selected as usize) };
                    self.focused = pane;
                    if self.comparing {
                        self.clear_compare();
                    }
                    if let Some(index) = self.documents.iter().position(|doc| doc.snapshot.id == id)
                    {
                        self.switch(index)?;
                    }
                }
            }
            Event::Focus(pane) => {
                // Focus notifications are queued. Ignore a view that has already
                // lost focus to a later tab/keyboard action.
                if pane < 2 && unsafe { GetFocus() } == self.editors[pane].hwnd {
                    self.focus_pane(pane);
                }
            }
            Event::CloseTab(pane, id) => {
                self.focused = pane;
                if let Some(index) = self.documents.iter().position(|doc| doc.snapshot.id == id) {
                    self.switch(index)?;
                    self.close_document()?;
                }
            }
            Event::MoveTab(id, target) => self.reorder_tabs(id, Some(target))?,
            Event::PinTab(id) => self.reorder_tabs(id, None)?,
            Event::SplitTab(id) => self.split_tab(id)?,
            Event::CloneTab(id) => self.transfer_tab(id, true)?,
            Event::CompareTabs(current, target) => self.compare_tabs(current, target)?,
            Event::Updated(pane) => {
                self.editors[pane].update_line_number_margin();
                if pane == self.focused {
                    let index = self.index();
                    let position = self.editor().position();
                    if self.documents[index].snapshot.caret != position {
                        self.documents[index].snapshot.caret = position;
                        if self.editor().send(SCI_CALLTIPACTIVE, 0, 0) != 0 {
                            self.editor().call_tip(
                                &self.languages[self.documents[index].language],
                                &self.completion_api,
                            )?;
                        }
                    }
                    self.update_map_view();
                    if self.comparing && self.secondary.is_some() {
                        let visible = self.editor().send(SCI_GETFIRSTVISIBLELINE, 0, 0);
                        if self.compare_options.align {
                            let virtual_row = if visible <= 0 {
                                0
                            } else {
                                visible as usize + self.compare_leading[pane]
                            };
                            let mut resized = false;
                            for side in 0..2 {
                                let top = self.compare_leading[side].saturating_sub(virtual_row);
                                resized |= self.compare_top[side] != top;
                                self.compare_top[side] = top;
                                if side != pane {
                                    let target =
                                        virtual_row.saturating_sub(self.compare_leading[side]);
                                    if self.editors[side].send(SCI_GETFIRSTVISIBLELINE, 0, 0)
                                        != target as isize
                                    {
                                        self.editors[side].send(SCI_SETFIRSTVISIBLELINE, target, 0);
                                    }
                                }
                            }
                            if resized {
                                self.layout();
                            }
                        } else {
                            let source_line = self
                                .editor()
                                .send(SCI_DOCLINEFROMVISIBLE, visible as usize, 0)
                                .max(0) as usize;
                            let target_line =
                                core::corresponding_line(&self.differences, source_line, pane == 1);
                            let other = self.editors[1 - pane];
                            let line = other.send(SCI_VISIBLEFROMDOCLINE, target_line, 0);
                            if other.send(SCI_GETFIRSTVISIBLELINE, 0, 0) != line {
                                other.send(SCI_SETFIRSTVISIBLELINE, line as usize, 0);
                            }
                        }
                    }
                    self.update_status();
                }
            }
            Event::Changed(pane) => {
                self.last_zero_match = None;
                if self.editors[pane].length() > core::MAX_DOCUMENT_BYTES {
                    self.editors[pane].send(SCI_UNDO, 0, 0);
                    return Err(
                        "The edit was undone because it exceeded the 256 MiB document limit."
                            .into(),
                    );
                }
                let index = if pane == 0 {
                    self.primary
                } else {
                    self.secondary.unwrap_or(self.primary)
                };
                let doc = &mut self.documents[index];
                let dirty = doc.base_dirty
                    || doc.metadata_dirty
                    || self.editors[pane].send(SCI_GETMODIFY, 0, 0) != 0;
                let tab_changed = doc.snapshot.dirty != dirty;
                doc.snapshot.dirty = dirty;
                doc.revision += 1;
                doc.last_edit = Instant::now();
                if self.results.data.as_ref().is_some_and(|results| {
                    results.files.iter().any(|file| file.id == doc.snapshot.id)
                }) {
                    set_text(
                        self.results.title,
                        "Document changed - rerun Find All to refresh its results.",
                    );
                }
                if self.json_document == Some(doc.snapshot.id) {
                    self.json_document = None;
                }
                if self.tree_visible && index == self.index() {
                    self.schedule_json();
                }
                if self.comparing {
                    if self.compare_ranges.is_some() {
                        self.clear_compare();
                        self.note="Selected-line comparison ended after the edit; reselect lines to compare again.".into();
                    } else {
                        for editor in self.editors {
                            editor.clear_diff();
                        }
                        self.compare_leading = [0; 2];
                        self.compare_top = [0; 2];
                        self.layout();
                        self.differences.clear();
                        self.compare_due = Some(Instant::now() + Duration::from_millis(350));
                        self.note = "Updating comparison...".into();
                    }
                }
                self.touch();
                if tab_changed {
                    self.update_tabs();
                }
                self.update_status();
            }
            Event::Style(pane) => {
                let index = if pane == 0 {
                    self.primary
                } else {
                    self.secondary.unwrap_or(self.primary)
                };
                if self.languages[self.documents[index].language].uses_container()
                    && self.editors[pane].send(SCI_GETENDSTYLED, 0, 0)
                        < self.editors[pane].length() as isize
                {
                    self.documents[index].styled_revision = None;
                }
            }
            Event::Character(pane, ch) => {
                if pane == self.focused
                    && char::from_u32(ch as u32).is_some_and(|c| c.is_alphanumeric() || c == '_')
                {
                    self.editor().complete(
                        &self.languages[self.documents[self.index()].language],
                        &self.completion_api,
                        false,
                    )?;
                }
                if pane == self.focused {
                    if ch == b'.' as i32 {
                        self.editor().complete(
                            &self.languages[self.documents[self.index()].language],
                            &self.completion_api,
                            true,
                        )?;
                    }
                    self.editor().call_tip(
                        &self.languages[self.documents[self.index()].language],
                        &self.completion_api,
                    )?;
                }
                if pane == self.focused
                    && ch == b'\n' as i32
                    && self.editor().send(SCI_GETSELECTIONS, 0, 0) == 1
                {
                    let ed = self.editor();
                    let line = ed.send(SCI_LINEFROMPOSITION, ed.position(), 0) as usize;
                    if line > 0 {
                        let indent = ed.send(SCI_GETLINEINDENTATION, line - 1, 0);
                        ed.send(SCI_SETLINEINDENTATION, line, indent);
                        let position = ed.send(SCI_GETLINEINDENTPOSITION, line, 0) as usize;
                        ed.select(position..position);
                    }
                }
            }
            Event::Tree(index) => {
                if self.json_document != Some(self.documents[self.index()].snapshot.id) {
                    self.note("JSON tree is updating; navigation resumes when parsing completes.");
                } else if let Some(node) = self.json_nodes.get(index) {
                    self.editor().select(node.span.clone());
                    self.note(format!(
                        "JSON pointer: {}",
                        if node.pointer.is_empty() {
                            "/"
                        } else {
                            &node.pointer
                        }
                    ));
                }
            }
            Event::Drop(paths) => {
                let mut failures = Vec::new();
                for path in paths {
                    if let Err(error) = self.open_path(&path, None) {
                        failures.push(error);
                    }
                }
                if let Some(first) = failures.first() {
                    return Err(match failures.len() {
                        1 => first.clone(),
                        count => format!("{first} ({} more file(s) also failed.)", count - 1),
                    });
                }
            }
            Event::Map(y) => {
                let display_line = self.map.send(SCI_GETFIRSTVISIBLELINE, 0, 0);
                let line_height = self.map.send(SCI_TEXTHEIGHT, 0, 0).max(1);
                let line = (display_line + y.max(0) as isize / line_height)
                    .min(self.editor().send(SCI_GETLINECOUNT, 0, 0) - 1)
                    .max(0);
                self.editor().send(SCI_GOTOLINE, line as usize, 0);
                self.editor().focus();
            }
            Event::SplitDrag(x) => {
                if self.secondary.is_some() {
                    let mut rect: RECT = unsafe { zeroed() };
                    unsafe {
                        GetClientRect(self.hwnd, &mut rect);
                    }
                    let tree = if self.tree_visible {
                        self.scale(280).min(rect.right / 3)
                    } else {
                        0
                    };
                    let map = if self.map_visible { self.scale(115) } else { 0 };
                    self.ratio = ((x - tree) as f32 / (rect.right - tree - map).max(1) as f32)
                        .clamp(0.2, 0.8);
                    self.layout();
                }
            }
            Event::ResultActivate => self.activate_result_row()?,
            Event::ResultsResize(y) => {
                let mut rect: RECT = unsafe { zeroed() };
                unsafe {
                    GetClientRect(self.hwnd, &mut rect);
                }
                self.results.height =
                    ((rect.bottom - self.scale(28) - y) * 96 / self.dpi.max(1) as i32).max(90);
                self.layout();
            }
        }
        Ok(())
    }
}

pub fn run() -> Result<()> {
    unsafe {
        SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32);
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(null());
        editor::register(instance)?;
        let init = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TAB_CLASSES | ICC_TREEVIEW_CLASSES | ICC_STANDARD_CLASSES | ICC_BAR_CLASSES,
        };
        if InitCommonControlsEx(&init) == 0 {
            return Err("Could not initialize native controls.".into());
        }
        let mut args = std::env::args_os().skip(1);
        let mut paths = Vec::new();
        let mut language_paths = Vec::new();
        let mut api_paths = Vec::new();
        let mut session_dir = None;
        while let Some(arg) = args.next() {
            if arg == "--session-dir" {
                session_dir = Some(PathBuf::from(
                    args.next().ok_or("--session-dir requires a directory.")?,
                ));
            } else if arg == "--import-language" {
                language_paths.push(PathBuf::from(
                    args.next()
                        .ok_or("--import-language requires an XML file.")?,
                ));
            } else if arg == "--completion-api" {
                api_paths.push(PathBuf::from(
                    args.next()
                        .ok_or("--completion-api requires an XML file.")?,
                ));
            } else {
                paths.push(PathBuf::from(arg));
            }
        }
        let directory = match session_dir {
            Some(directory) => directory,
            None => session::default_directory(
                &std::env::var_os("LOCALAPPDATA")
                    .map(PathBuf::from)
                    .ok_or("LOCALAPPDATA is unavailable.")?,
            )?,
        };
        fs::create_dir_all(&directory)
            .map_err(|e| format!("Could not create recovery directory: {e}"))?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .share_mode(0)
            .open(directory.join("session.lock"))
            .map_err(|e| {
                format!(
                    "Cannot lock the session. Another rstpd instance may already be running: {e}"
                )
            })?;
        let recovery_path = directory.join("session.json");
        let session = session::load(&recovery_path)?;
        let class = wide("rstpd.Window");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hIcon: LoadIconW(instance, std::ptr::without_provenance(1)),
            lpszClassName: class.as_ptr(),
            ..zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        COLORS.with(|c| c.set(Palette::new(false)));
        PANEL_BRUSH.with(|b| b.set(CreateSolidBrush(Palette::new(false).panel)));
        FIELD_BRUSH.with(|b| {
            b.set(CreateSolidBrush(
                controls::Colors::new(Palette::new(false)).field,
            ))
        });
        let hwnd = CreateWindowExW(
            WS_EX_ACCEPTFILES,
            class.as_ptr(),
            wide("rstpd").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1280,
            840,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        if hwnd.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let primary = Editor::new(hwnd, instance, 101, true)?;
        let secondary = Editor::new(hwnd, instance, 102, false)?;
        let scratch = Editor::new(hwnd, instance, 103, false)?;
        let map = Editor::new(hwnd, instance, 104, false)?;
        let result_editor = Editor::new(hwnd, instance, RESULTS_EDITOR_ID, false)?;
        SetWindowSubclass(map.hwnd, Some(map_proc), 1, 0);
        RevokeDragDrop(map.hwnd);
        SetWindowLongPtrW(
            map.hwnd,
            GWL_STYLE,
            GetWindowLongPtrW(map.hwnd, GWL_STYLE) & !(WS_TABSTOP as isize),
        );
        let mut languages = languages::catalog(&editor::available_lexers());
        for definition in session.custom_languages {
            languages::add_custom(&mut languages, definition)?;
        }
        let mut app = App {
            hwnd,
            instance,
            editors: [primary, secondary],
            scratch,
            map,
            tabs: null_mut(),
            right_tabs: null_mut(),
            groups: Default::default(),
            status: null_mut(),
            tools: Vec::new(),
            tooltips: None,
            tree: null_mut(),
            search: SearchBar {
                labels: [null_mut(); 3],
                query: null_mut(),
                replace: null_mut(),
                mode: null_mut(),
                case: null_mut(),
                word: null_mut(),
                buttons: Vec::new(),
                visible: false,
                directory: null_mut(),
                filters: null_mut(),
                recursive: null_mut(),
                hidden: null_mut(),
                folder_labels: [null_mut(); 2],
                folder_buttons: Vec::new(),
                folder_visible: false,
            },
            results: ResultPanel::new(result_editor),
            documents: Vec::new(),
            languages,
            completion_api: session.completion_api,
            primary: 0,
            secondary: None,
            focused: 0,
            next_id: 1,
            palette: Palette::new(false),
            theme: session.theme.clone(),
            editor_font: session.editor_font,
            show_symbols: session.show_symbols,
            monitor_files: session.monitor_files,
            monitor: Monitor::new(),
            monitor_busy: false,
            monitor_last: Instant::now(),
            compare_options: session.compare_options,
            compare_generation: 0,
            compare_ranges: None,
            compare_leading: [0; 2],
            compare_top: [0; 2],
            font: null_mut(),
            dpi: GetDpiForWindow(hwnd),
            map_visible: false,
            tree_visible: false,
            wrap: false,
            ratio: 0.5,
            json_nodes: Vec::new(),
            json_document: None,
            json_handles: Vec::new(),
            json_due: None,
            json_rx: None,
            highlight_rx: None,
            differences: Vec::new(),
            difference: 0,
            comparing: false,
            compare_rx: None,
            compare_due: None,
            compare_jump: false,
            revision: 0,
            recovered_revision: 0,
            recovery_busy: false,
            recovery: RecoveryWorker::new(recovery_path),
            last_autosave: Instant::now(),
            recovery_error: None,
            note: String::new(),
            last_zero_match: None,
            exiting: false,
            _lock: lock,
        };
        app.set_font();
        app.create_controls()?;
        app.apply_theme();
        for path in language_paths {
            app.import_language(&path)?;
        }
        let active = session.active;
        let pane_documents = session.pane_documents;
        let pane_selected = session.pane_selected;
        let focused_pane = session.focused_pane.min(1);
        for snapshot in session.documents {
            app.add_document(snapshot)?;
        }
        if !app.documents.is_empty() {
            let original_ids: Vec<_> = app.documents.iter().map(|doc| doc.snapshot.id).collect();
            let active_id = app.documents[active.min(app.documents.len() - 1)]
                .snapshot
                .id;
            app.documents.sort_by_key(|doc| !doc.snapshot.pinned);
            let active = app
                .documents
                .iter()
                .position(|doc| doc.snapshot.id == active_id)
                .expect("restored active tab");
            app.switch(active.min(app.documents.len() - 1))?;
            if pane_documents.iter().any(|group| !group.is_empty()) {
                app.groups = pane_documents.map(|group| {
                    group
                        .into_iter()
                        .filter_map(|i| original_ids.get(i).copied())
                        .collect()
                });
                for id in &original_ids {
                    if !app.groups.iter().any(|group| group.contains(id)) {
                        app.groups[0].push(*id);
                    }
                }
                let selected = pane_selected.map(|i| {
                    original_ids
                        .get(i)
                        .and_then(|id| app.documents.iter().position(|doc| doc.snapshot.id == *id))
                });
                app.primary = selected[0].unwrap_or(0);
                app.secondary = selected[1];
                app.focused = focused_pane;
            }
            app.normalize_groups();
            app.refresh_views()?;
            app.update_tabs();
        }
        for path in paths {
            if let Err(error) = app.open_path(&path, None) {
                show_error(&error);
            }
        }
        if app.documents.is_empty() {
            app.new_document()?;
        }
        for path in api_paths {
            app.import_api(&path)?;
        }
        app.apply_theme();
        app.layout();
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        if SetTimer(hwnd, 1, 250, None) == 0 {
            return Err("Could not start the recovery timer.".into());
        }
        app.editor().focus();
        let mut message: MSG = zeroed();
        loop {
            let status = GetMessageW(&mut message, null_mut(), 0, 0);
            if status == 0 {
                break;
            }
            if status < 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            match app.key(&message) {
                Ok(true) => {}
                Ok(false) => {
                    if IsDialogMessageW(hwnd, &message) == 0 {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                Err(error) => {
                    show_error(&error);
                }
            }
            while !app.exiting {
                let event = EVENTS.with(|q| q.borrow_mut().pop_front());
                let Some(event) = event else { break };
                if let Err(error) = app.event(event) {
                    show_error(&error);
                }
            }
        }
        if !app.font.is_null() {
            DeleteObject(app.font);
        }
        PANEL_BRUSH.with(|b| DeleteObject(b.replace(null_mut())));
        FIELD_BRUSH.with(|b| DeleteObject(b.replace(null_mut())));
        Ok(())
    }
}

#[cfg(test)]
mod split_tests {
    use super::*;

    #[test]
    fn chrome_has_one_separator_and_no_pane_accent_when_focus_changes() {
        unsafe {
            let hwnd = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                null(),
                WS_POPUP,
                0,
                0,
                100,
                100,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!hwnd.is_null());
            let screen = GetDC(hwnd);
            let dc = CreateCompatibleDC(screen);
            let bitmap = CreateCompatibleBitmap(screen, 100, 100);
            assert!(!dc.is_null() && !bitmap.is_null());
            let old_bitmap = SelectObject(dc, bitmap);
            let old_palette = COLORS.with(Cell::get);
            let old_brush = PANEL_BRUSH.with(Cell::get);
            PANEL_BOUNDS.with(|bounds| {
                bounds.borrow_mut().push(RECT {
                    left: 5,
                    top: 5,
                    right: 40,
                    bottom: 95,
                });
                bounds.borrow_mut().push(RECT {
                    left: 50,
                    top: 5,
                    right: 95,
                    bottom: 95,
                });
            });
            PANE_STRIPS.with(|strips| {
                strips.set([
                    Some(RECT {
                        left: 5,
                        top: 5,
                        right: 40,
                        bottom: 8,
                    }),
                    Some(RECT {
                        left: 50,
                        top: 5,
                        right: 95,
                        bottom: 8,
                    }),
                ]);
            });
            SPLIT_BOUNDS.with(|bounds| {
                bounds.set(Some(RECT {
                    left: 40,
                    top: 5,
                    right: 50,
                    bottom: 95,
                }));
            });
            for dark in [true, false] {
                let palette = Palette::new(dark);
                let brush = CreateSolidBrush(palette.panel);
                COLORS.with(|colors| colors.set(palette));
                PANEL_BRUSH.with(|panel| panel.set(brush));
                // Focus still tracks the physical pane without painting a pane-wide accent.
                for active in [0, 1, 0] {
                    select_pane(hwnd, active);
                    window_proc(hwnd, WM_ERASEBKGND, dc as usize, 0);
                    assert_eq!(GetPixel(dc, 45, 20), palette.border);
                    for x in [39, 40, 42, 49, 50] {
                        assert_eq!(GetPixel(dc, x, 20), palette.panel);
                    }
                    assert_eq!(
                        GetPixel(dc, 5, 20),
                        if dark { palette.border } else { palette.panel }
                    );
                    assert_eq!(ACTIVE_PANE.with(Cell::get), active);
                    for x in [10, 60] {
                        assert_eq!(GetPixel(dc, x, 6), palette.panel);
                    }
                }
                DeleteObject(brush);
            }
            COLORS.with(|colors| colors.set(old_palette));
            PANEL_BRUSH.with(|panel| panel.set(old_brush));
            PANEL_BOUNDS.with(|bounds| bounds.borrow_mut().clear());
            SPLIT_BOUNDS.with(|bounds| bounds.set(None));
            PANE_STRIPS.with(|strips| strips.set([None, None]));
            ACTIVE_PANE.with(|active| active.set(0));
            SelectObject(dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(dc);
            ReleaseDC(hwnd, screen);
            DestroyWindow(hwnd);
        }
    }

    #[test]
    fn divider_hit_area_excludes_chrome_and_preserves_grab_offset() {
        SPLIT_BOUNDS.with(|bounds| {
            bounds.set(Some(RECT {
                left: 400,
                top: 79,
                right: 410,
                bottom: 600,
            }));
        });
        for (x, y) in [(399, 100), (410, 100), (405, 78), (405, 600)] {
            assert_eq!(split_hit(x, y), None);
        }
        assert_eq!(split_hit(400, 79), Some(0));
        assert_eq!(split_hit(409, 599), Some(9));
        SPLIT_BOUNDS.with(|bounds| bounds.set(None));
        assert_eq!(split_hit(405, 100), None);
    }

    #[test]
    fn only_divider_drags_queue_resizes_and_capture_loss_stops_them() {
        unsafe {
            let hwnd = CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                null(),
                WS_OVERLAPPED,
                0,
                0,
                800,
                600,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!hwnd.is_null());
            let point = |x: i32, y: i32| (x as u16 as isize) | ((y as isize) << 16);
            SPLIT_BOUNDS.with(|bounds| {
                bounds.set(Some(RECT {
                    left: 400,
                    top: 79,
                    right: 410,
                    bottom: 500,
                }));
            });
            EVENTS.with(|events| events.borrow_mut().clear());
            window_proc(hwnd, WM_LBUTTONDOWN, 0, point(405, 20));
            assert_ne!(GetCapture(), hwnd);
            window_proc(hwnd, WM_LBUTTONDOWN, 0, point(408, 100));
            assert_eq!(GetCapture(), hwnd);
            assert!(EVENTS.with(|events| events.borrow().is_empty()));
            window_proc(hwnd, WM_MOUSEMOVE, MK_LBUTTON as usize, point(508, 100));
            assert!(EVENTS.with(|events| matches!(
                events.borrow_mut().pop_front(),
                Some(Event::SplitDrag(500))
            )));
            window_proc(hwnd, WM_LBUTTONUP, 0, point(518, 100));
            assert_ne!(GetCapture(), hwnd);
            assert!(EVENTS.with(|events| matches!(
                events.borrow_mut().pop_front(),
                Some(Event::SplitDrag(510))
            )));
            window_proc(hwnd, WM_LBUTTONDOWN, 0, point(402, 100));
            window_proc(hwnd, WM_CANCELMODE, 0, 0);
            assert_ne!(GetCapture(), hwnd);
            assert_eq!(SPLIT_DRAG_OFFSET.with(Cell::get), None);
            window_proc(hwnd, WM_LBUTTONDOWN, 0, point(402, 100));
            ReleaseCapture();
            window_proc(hwnd, WM_CAPTURECHANGED, 0, 0);
            assert_eq!(SPLIT_DRAG_OFFSET.with(Cell::get), None);
            window_proc(hwnd, WM_MOUSEMOVE, MK_LBUTTON as usize, point(600, 100));
            assert!(EVENTS.with(|events| events.borrow().is_empty()));
            SPLIT_BOUNDS.with(|bounds| bounds.set(None));
            DestroyWindow(hwnd);
        }
    }
}

#[cfg(test)]
mod font_dialog_tests {
    use super::*;

    #[test]
    fn logfont_initialization_keeps_unicode_and_fractional_size() {
        let family = format!("{}X", "\u{1f680}".repeat(15));
        let font = EditorFont::new(&family, 1250).unwrap();
        let logfont = editor_logfont(&font, 96).unwrap();
        assert_eq!(
            String::from_utf16(&logfont.lfFaceName[..31]).unwrap(),
            family
        );
        assert_eq!(logfont.lfFaceName[31], 0);
        assert_eq!(logfont.lfHeight, -17);
        assert_eq!(logfont.lfWeight, FW_NORMAL as i32);
        assert_eq!(
            (logfont.lfItalic, logfont.lfUnderline, logfont.lfStrikeOut),
            (0, 0, 0)
        );
        assert_eq!(font_size_label(1100), "11");
        assert_eq!(font_size_label(1250), "12.5");
        assert_eq!(font_size_label(1234), "12.34");
        assert!(
            editor_logfont(&EditorFont::new("\u{1f680}".repeat(16), 1100).unwrap(), 96).is_err()
        );
        assert!(editor_logfont(&EditorFont::new("a".repeat(32), 1100).unwrap(), 96).is_err());
        assert!(editor_logfont(&font, 0).is_err());
    }

    #[test]
    fn dialog_results_are_validated_without_truncating_or_clamping() {
        let font = EditorFont::new("Consolas", 1250).unwrap();
        let mut logfont = editor_logfont(&font, 96).unwrap();
        assert_eq!(font_from_dialog(&logfont, 125).unwrap(), font);
        assert_eq!(
            font_from_dialog(&logfont, 40).unwrap().size_hundredths(),
            400
        );
        assert_eq!(
            font_from_dialog(&logfont, 720).unwrap().size_hundredths(),
            7200
        );
        for size in [-1, 0, 39, 721, i32::MAX] {
            assert!(font_from_dialog(&logfont, size).is_err(), "{size}");
        }
        logfont.lfFaceName.fill(b'a' as u16);
        assert!(font_from_dialog(&logfont, 110).is_err());
        logfont.lfFaceName.fill(0);
        logfont.lfFaceName[0] = 0xd800;
        assert!(font_from_dialog(&logfont, 110).is_err());
    }

    #[cfg(windows)]
    thread_local! {
        static DIALOG_CHECKS: Cell<u32> = const { Cell::new(0) };
    }

    #[cfg(windows)]
    unsafe extern "system" fn inspect_dialog(
        hwnd: HWND,
        message: u32,
        w: WPARAM,
        l: LPARAM,
    ) -> usize {
        unsafe {
            let result = editor_font_hook(hwnd, message, w, l);
            if message == WM_INITDIALOG {
                let style = GetDlgItem(hwnd, cmb2 as i32);
                let mut checks = 0;
                if !style.is_null() && IsWindowEnabled(style) == 0 {
                    checks |= 1;
                }
                EnableWindow(style, 1);
                if !style.is_null() && IsWindowEnabled(style) == 0 {
                    checks |= 2;
                }
                if [chx1, chx2, cmb4].into_iter().all(|id| {
                    let control = GetDlgItem(hwnd, id as i32);
                    control.is_null()
                        || GetWindowLongPtrW(control, GWL_STYLE) & WS_VISIBLE as isize == 0
                        || IsWindowEnabled(control) == 0
                }) {
                    checks |= 4;
                }
                if IsWindowEnabled(GetDlgItem(hwnd, cmb5 as i32)) == 0 {
                    checks |= 8;
                }
                if window_text(GetDlgItem(hwnd, cmb3 as i32)) == "12.5" {
                    checks |= 16;
                }
                DIALOG_CHECKS.with(|value| value.set(checks));
                PostMessageW(hwnd, WM_COMMAND, IDCANCEL as usize, 0);
            }
            result
        }
    }

    #[cfg(windows)]
    #[test]
    fn native_dialog_only_offers_family_and_size_and_can_cancel() {
        let font = EditorFont::new("Consolas", 1250).unwrap();
        let mut logfont = editor_logfont(&font, 96).unwrap();
        let mut state = FontDialogState {
            size_text: wide(&font_size_label(font.size_hundredths())),
            initialization_failed: false,
        };
        let mut dialog = CHOOSEFONTW {
            lStructSize: size_of::<CHOOSEFONTW>() as u32,
            lpLogFont: &mut logfont,
            Flags: EDITOR_FONT_FLAGS,
            lCustData: (&mut state as *mut FontDialogState) as isize,
            lpfnHook: Some(inspect_dialog),
            nSizeMin: 4,
            nSizeMax: 72,
            ..CHOOSEFONTW::default()
        };
        assert_eq!(unsafe { ChooseFontW(&mut dialog) }, 0);
        assert_eq!(unsafe { CommDlgExtendedError() }, 0);
        assert!(!state.initialization_failed);
        assert_eq!(DIALOG_CHECKS.with(Cell::get), 31);
    }
}
#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn canonical_target_resolves_parents_for_new_files() {
        let base = std::env::temp_dir();
        let nested = base.join("rstpd-canonical-test");
        std::fs::create_dir_all(&nested).unwrap();
        let target = nested.join("..").join("new-file.txt");
        let resolved = canonical_target(&target);
        assert!(resolved.ends_with("new-file.txt"));
        assert_eq!(
            resolved.parent().unwrap(),
            std::fs::canonicalize(&base).unwrap()
        );
        assert!(!resolved.to_string_lossy().contains(".."));
        std::fs::remove_dir(&nested).unwrap();
    }
}
