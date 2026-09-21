use crate::{
    completion::{self, Api},
    core::{
        self, CaseOp, Difference, EditorFont, Encoding, Eol, JsonNode, LineOp, Result, Search,
        SearchMode,
    },
    editor::{self, DocumentHandle, Editor, Palette, sci::*},
    languages::{self, Language},
    session::{self, DocumentSnapshot, RecoveryWorker, Session},
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
    sync::mpsc,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::{
        LibraryLoader::*,
        Ole::RevokeDragDrop,
        Registry::*,
        SystemServices::{MK_LBUTTON, SS_CENTERIMAGE},
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
        tooltip: "Compare active tab with next tab",
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

thread_local! {
    static EVENTS: RefCell<VecDeque<Event>> = const { RefCell::new(VecDeque::new()) };
    static COLORS: Cell<Palette> = Cell::new(Palette::new(false));
    static PANEL_BRUSH: Cell<HBRUSH> = const { Cell::new(null_mut()) };
    static UI_FONT: Cell<HFONT> = const { Cell::new(null_mut()) };
    static MENU_LABELS: RefCell<Vec<(String, bool)>> = const { RefCell::new(Vec::new()) };
    static MENUS: RefCell<Vec<HMENU>> = const { RefCell::new(Vec::new()) };
    static TREE_UPDATING: Cell<bool> = const {Cell::new(false)};
    static HOT_TOOL: Cell<HWND> = const {Cell::new(null_mut())};
    static ICON_ERROR_REPORTED: Cell<bool> = const {Cell::new(false)};
}

enum Event {
    RenderError(String),
    Command(usize),
    Resize,
    Close,
    Tick,
    Theme,
    Dpi,
    Tab,
    CloseTab(usize),
    Tree(usize),
    Focus(usize),
    Changed(usize),
    Style(usize),
    Updated(usize),
    Character(usize, i32),
    Drop(Vec<PathBuf>),
    Map(i32),
    SplitDrag(i32),
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

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match message {
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
                info.ptMinTrackSize = POINT { x: 780, y: 480 };
                return 0;
            }
            WM_NOTIFY => {
                let header = &*(l as *const NMHDR);
                match header.idFrom {
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
                    TAB_ID if header.code == TCN_SELCHANGE => queue(Event::Tab),
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
                return 1;
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => {
                let palette = COLORS.with(Cell::get);
                SetTextColor(w as HDC, palette.text);
                SetBkColor(w as HDC, palette.panel);
                return PANEL_BRUSH.with(Cell::get) as isize;
            }
            WM_DRAWITEM => {
                let item = &*(l as *const DRAWITEMSTRUCT);
                if item.CtlType == ODT_BUTTON
                    && let Some(button) = TOOLBAR
                        .iter()
                        .find(|button| button.command == item.CtlID as usize)
                {
                    match toolbar::draw(
                        item,
                        button.icon,
                        COLORS.with(Cell::get),
                        HOT_TOOL.with(Cell::get) == item.hwndItem,
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
                    if let Some((title, _)) = label {
                        paint_label(
                            item.hDC,
                            &item.rcItem,
                            &title,
                            item.itemState & (ODS_SELECTED | ODS_HOTLIGHT) != 0,
                            item.itemState & ODS_DISABLED != 0,
                        );
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
                SetCapture(hwnd);
                queue(Event::SplitDrag((l as i16) as i32));
                return 0;
            }
            WM_MOUSEMOVE if GetCapture() == hwnd => {
                queue(Event::SplitDrag((l as i16) as i32));
                return 0;
            }
            WM_LBUTTONUP if GetCapture() == hwnd => {
                ReleaseCapture();
                return 0;
            }
            _ => {}
        }
        DefWindowProcW(hwnd, message, w, l)
    }
}

unsafe extern "system" fn toolbar_proc(
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
                let previous = HOT_TOOL.with(|hot| hot.replace(hwnd));
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
                if HOT_TOOL.with(Cell::get) == hwnd {
                    HOT_TOOL.with(|hot| hot.set(null_mut()));
                    InvalidateRect(hwnd, null(), 0);
                }
                if message == WM_NCDESTROY {
                    RemoveWindowSubclass(hwnd, Some(toolbar_proc), 3);
                }
            }
            _ => {}
        }
        DefSubclassProc(hwnd, message, w, l)
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

unsafe extern "system" fn tab_proc(
    hwnd: HWND,
    message: u32,
    w: WPARAM,
    l: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    unsafe {
        match message {
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
                    if index == selected {
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
                        queue(Event::CloseTab(index as usize));
                        return 0;
                    }
                }
            }
            _ => {}
        }
        DefSubclassProc(hwnd, message, w, l)
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
    query: HWND,
    replace: HWND,
    mode: HWND,
    case: HWND,
    word: HWND,
    buttons: Vec<HWND>,
    visible: bool,
}

struct CompareResult {
    left: u64,
    right: u64,
    left_rev: u64,
    right_rev: u64,
    result: Result<Vec<Difference>>,
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
    status: HWND,
    tools: Vec<HWND>,
    tooltips: Option<Tooltips>,
    search: SearchBar,
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
        self.control(
            "BUTTON",
            title,
            WS_VISIBLE | WS_TABSTOP | BS_OWNERDRAW as u32,
            id,
        )
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
        unsafe {
            SetWindowSubclass(self.tabs, Some(tab_proc), 2, 0);
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
                if SetWindowSubclass(control, Some(toolbar_proc), 3, 0) == 0 {
                    return Err("Could not initialize toolbar interaction.".into());
                }
                tooltips.add(control, button.tooltip)?;
            }
            self.tools.push(control);
        }
        self.tooltips = Some(tooltips);
        self.search.query = self.control("EDIT", "", WS_TABSTOP | ES_AUTOHSCROLL as u32, 401)?;
        self.search.replace = self.control("EDIT", "", WS_TABSTOP | ES_AUTOHSCROLL as u32, 402)?;
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
        ]
        .into_iter()
        .map(|(label, id)| self.button(label, id))
        .collect::<Result<_>>()?;
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
                    ],
                ),
                (
                    "&View",
                    &[
                        (SPLIT, "Toggle split view\tCtrl+Alt+Right"),
                        (MAP, "Document map"),
                        (WRAP, "Word wrap"),
                        (EDITOR_FONT, "Editor &font..."),
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
                        (COMPARE, "Compare active tab with next tab"),
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
        Ok(())
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
        }
        self.apply_editor_styles();
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
                self.tree,
                self.search.query,
                self.search.replace,
                self.search.mode,
                self.search.case,
                self.search.word,
            ];
            controls.extend(&self.tools);
            if let Some(tooltips) = &self.tooltips {
                controls.push(tooltips.hwnd);
            }
            controls.extend(&self.search.buttons);
            for control in controls {
                unsafe {
                    SendMessageW(control, WM_SETFONT, font as usize, 1);
                }
            }
        }
        if !old.is_null() {
            unsafe {
                DeleteObject(old);
            }
        }
    }
    fn layout(&self) {
        unsafe {
            let mut rect: RECT = zeroed();
            GetClientRect(self.hwnd, &mut rect);
            let (width, height) = (rect.right, rect.bottom);
            let toolbar_h = self.scale(42);
            let tab_h = self.scale(37);
            let search_h = if self.search.visible {
                self.scale(82)
            } else {
                0
            };
            let status_h = self.scale(28);
            let top = toolbar_h + tab_h + search_h;
            let body_h = (height - top - status_h).max(1);
            let tree_w = if self.tree_visible {
                self.scale(280).min(width / 3)
            } else {
                0
            };
            let map_w = if self.map_visible { self.scale(115) } else { 0 };
            let content_w = (width - tree_w - map_w).max(1);
            let gap = self.scale(6);
            let left_w = if self.secondary.is_some() {
                (content_w as f32 * self.ratio) as i32
            } else {
                content_w
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
            MoveWindow(self.tabs, 0, toolbar_h, width, tab_h, 1);
            MoveWindow(
                self.status,
                self.scale(10),
                height - status_h,
                width - self.scale(20),
                status_h,
                1,
            );
            MoveWindow(self.tree, 0, top, tree_w, body_h, 1);
            ShowWindow(self.tree, if self.tree_visible { SW_SHOW } else { SW_HIDE });
            MoveWindow(self.editors[0].hwnd, tree_w, top, left_w, body_h, 1);
            ShowWindow(
                self.editors[1].hwnd,
                if self.secondary.is_some() {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
            if self.secondary.is_some() {
                MoveWindow(
                    self.editors[1].hwnd,
                    tree_w + left_w + gap,
                    top,
                    (content_w - left_w - gap).max(1),
                    body_h,
                    1,
                );
            }
            MoveWindow(self.map.hwnd, width - map_w, top, map_w, body_h, 1);
            ShowWindow(
                self.map.hwnd,
                if self.map_visible { SW_SHOWNA } else { SW_HIDE },
            );
            let y = toolbar_h + tab_h + self.scale(6);
            let input_w = (width / 3).max(self.scale(180));
            MoveWindow(
                self.search.query,
                self.scale(12),
                y,
                input_w,
                self.scale(28),
                1,
            );
            MoveWindow(
                self.search.replace,
                self.scale(12),
                y + self.scale(35),
                input_w,
                self.scale(28),
                1,
            );
            let mode_x = input_w + self.scale(24);
            MoveWindow(
                self.search.mode,
                mode_x,
                y,
                self.scale(180),
                self.scale(180),
                1,
            );
            MoveWindow(
                self.search.case,
                mode_x,
                y + self.scale(35),
                self.scale(108),
                self.scale(28),
                1,
            );
            MoveWindow(
                self.search.word,
                mode_x + self.scale(110),
                y + self.scale(35),
                self.scale(116),
                self.scale(28),
                1,
            );
            for (i, button) in self.search.buttons.iter().enumerate() {
                let (x, row) = match i {
                    0 => (mode_x + self.scale(190), 0),
                    1 => (mode_x + self.scale(270), 0),
                    2 => (mode_x + self.scale(240), 1),
                    3 => (mode_x + self.scale(320), 1),
                    _ => (width - self.scale(70), 0),
                };
                MoveWindow(
                    *button,
                    x,
                    y + row * self.scale(35),
                    self.scale(if i == 3 { 96 } else { 76 }),
                    self.scale(28),
                    1,
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
        self.switch(self.documents.len() - 1)?;
        self.touch();
        Ok(())
    }
    fn new_document(&mut self) -> Result<()> {
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
        })
    }
    fn switch(&mut self, index: usize) -> Result<()> {
        if index >= self.documents.len() {
            return Ok(());
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
                self.editors[pane].send(SCI_SETWRAPMODE, self.wrap as usize, 0);
            }
        }
        self.configure_map();
        self.layout();
        Ok(())
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
        unsafe {
            SendMessageW(self.tabs, WM_SETREDRAW, 0, 0);
            SendMessageW(self.tabs, TCM_DELETEALLITEMS, 0, 0);
            for (i, doc) in self.documents.iter().enumerate() {
                let mut label = wide(&format!(
                    "{}{}{}",
                    if self.secondary == Some(i) {
                        "[R] "
                    } else {
                        ""
                    },
                    doc.snapshot.title,
                    if doc.snapshot.dirty { " *" } else { "" }
                ));
                let mut item: TCITEMW = zeroed();
                item.mask = TCIF_TEXT;
                item.pszText = label.as_mut_ptr();
                SendMessageW(
                    self.tabs,
                    TCM_INSERTITEMW,
                    i,
                    (&item as *const TCITEMW) as isize,
                );
            }
            SendMessageW(self.tabs, TCM_SETCURSEL, self.index(), 0);
            SendMessageW(self.tabs, WM_SETREDRAW, 1, 0);
            InvalidateRect(self.tabs, null(), 1);
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
    fn update_status(&self) {
        if self.documents.is_empty() {
            return;
        }
        let doc = &self.documents[self.index()];
        let editor = self.editor();
        let line = editor.send(SCI_LINEFROMPOSITION, editor.position(), 0) + 1;
        let column = editor.send(SCI_GETCOLUMN, editor.position(), 0) + 1;
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
                "Ln {line}, Col {column}   |   {} selections   |   {}   |   {}   {}   |   {state}   {}",
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
        self.documents.remove(index);
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
            custom_languages: self
                .languages
                .iter()
                .filter_map(|language| language.custom.as_deref().cloned())
                .collect(),
            completion_api: self.completion_api.clone(),
        })
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
    fn find(&mut self, previous: bool) -> Result<()> {
        let search = self.search_settings()?;
        let editor = self.editor();
        let text = self.tool_text(editor)?;
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
    fn replace(&mut self, all: bool) -> Result<()> {
        let search = self.search_settings()?;
        let editor = self.editor();
        let text = self.tool_text(editor)?;
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
    }
    fn start_compare(&mut self) -> Result<()> {
        if self.documents.len() < 2 {
            return Err("Open two documents in separate tabs to compare.".into());
        }
        self.clear_compare();
        self.primary = self.index();
        self.focused = 0;
        self.secondary = Some((self.primary + 1) % self.documents.len());
        self.refresh_views()?;
        self.update_tabs();
        self.comparing = true;
        self.compare_jump = true;
        self.launch_compare()
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
        let left_text = self.tool_text(self.editors[0])?;
        let right_text = self.tool_text(self.editors[1])?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = core::compare(&left_text, &right_text);
            let _ = tx.send(CompareResult {
                left: left_id,
                right: right_id,
                left_rev,
                right_rev,
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
        {
            if self.comparing {
                self.compare_due = Some(Instant::now() + Duration::from_millis(300));
            }
            return Ok(());
        }
        for editor in self.editors {
            editor.clear_diff();
        }
        self.differences = result.result?;
        self.comparing = true;
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
        self.note(format!(
            "{} difference groups. Red: left changes; green: right changes. F7 to navigate.",
            self.differences.len()
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
        let editor = self.editor();
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
            FIND => self.show_search()?,
            SEARCH_CLOSE => {
                self.search.visible = false;
                self.layout();
                editor.focus();
            }
            FIND_NEXT => self.find(false)?,
            FIND_PREVIOUS => self.find(true)?,
            REPLACE => self.replace(false)?,
            REPLACE_ALL => self.replace(true)?,
            SPLIT => {
                self.clear_compare();
                self.secondary = if self.secondary.is_some() {
                    None
                } else {
                    Some(self.primary)
                };
                if self.secondary.is_none() {
                    self.focused = 0;
                }
                self.refresh_views()?;
                self.update_tabs();
                self.note(
                    "Click a pane, then a tab, to choose that pane's document. F6 switches panes.",
                );
            }
            MAP => {
                self.map_visible = !self.map_visible;
                self.configure_map();
                self.layout();
            }
            WRAP => {
                self.wrap = !self.wrap;
                for ed in self.editors {
                    ed.send(SCI_SETWRAPMODE, self.wrap as usize, 0);
                }
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
                        "\nRust application with statically linked Scintilla + Lexilla.\nNo plugin loader, script execution, network service, or automatic updater.\n\nAlt+drag: rectangular selection\nCtrl+click: multiple carets\nCtrl+D / Ctrl+Shift+L: next / all occurrences\nF6: focus other pane; Ctrl+Tab: next tab\nCtrl+Space: contextual completion\nCtrl+Shift+Space: function parameter hint\nCtrl+mouse wheel: zoom\nCtrl+Alt+J: format JSON/JSON5; Ctrl+Alt+T: live JSON tree\nF7 / Shift+F7: next / previous difference\n\nCompare and JSON refresh automatically after edits.\nLanguage menu: import data-only language/API XML.\nUnsaved tabs recover when the app reopens. Close the app to keep them.\nRecovery is local plaintext; existing recovery folders are preserved.\nSee README for current and legacy recovery locations.\nRegex supports look-around/backreferences ($1 or ${name} replacements).\nFiles: 128 MiB; search, line and JSON tools: 16 MiB.\n\nIndependent application; not affiliated with Notepad++."
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
        let command = match (control, shift, alt, key) {
            (true, false, false, 0x4e) => Some(NEW),
            (true, false, false, 0x4f) => Some(OPEN),
            (true, false, false, 0x53) => Some(SAVE),
            (true, true, false, 0x53) => Some(SAVE_AS),
            (true, false, false, 0x57) => Some(CLOSE),
            (true, false, false, 0x46 | 0x48) => Some(FIND),
            (true, false, false, 0x44) => Some(ADD_NEXT),
            (true, true, false, 0x4c) => Some(SELECT_MATCHES),
            (true, true, false, 0x55) => Some(UPPER),
            (true, false, false, 0x55) => Some(LOWER),
            (true, false, true, 0x4a) => Some(JSON_FORMAT),
            (true, false, true, 0x54) => Some(JSON_TREE),
            (true, false, true, VK_RIGHT) => Some(SPLIT),
            (false, false, false, VK_F3) => Some(FIND_NEXT),
            (false, true, false, VK_F3) => Some(FIND_PREVIOUS),
            (false, false, false, VK_F7) => Some(DIFF_NEXT),
            (false, true, false, VK_F7) => Some(DIFF_PREVIOUS),
            (false, false, false, VK_ESCAPE) if self.search.visible => Some(SEARCH_CLOSE),
            (false, false, false, VK_RETURN)
                if self.search.visible && unsafe { GetFocus() } == self.search.query =>
            {
                Some(FIND_NEXT)
            }
            _ => None,
        };
        if let Some(command) = command {
            self.command(command)?;
            return Ok(true);
        }
        if control && key == VK_TAB {
            let len = self.documents.len();
            let index = (self.index() + if shift { len - 1 } else { 1 }) % len;
            self.switch(index)?;
            return Ok(true);
        }
        if key == VK_F6 && self.secondary.is_some() {
            self.focused = 1 - self.focused;
            self.editor().focus();
            self.configure_map();
            self.update_tabs();
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
            Event::Tab => {
                let selected = unsafe { SendMessageW(self.tabs, TCM_GETCURSEL, 0, 0) };
                if selected >= 0 {
                    if self.comparing {
                        self.clear_compare();
                    }
                    self.switch(selected as usize)?;
                }
            }
            Event::Focus(pane) => {
                if pane == 0 || self.secondary.is_some() {
                    let changed = self.focused != pane;
                    self.focused = pane;
                    self.configure_map();
                    self.update_tabs();
                    self.update_status();
                    if changed && self.tree_visible {
                        self.schedule_json();
                    }
                }
            }
            Event::CloseTab(index) => {
                self.switch(index)?;
                self.close_document()?;
            }
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
                    self.update_status();
                }
            }
            Event::Changed(pane) => {
                self.last_zero_match = None;
                if self.editors[pane].length() > core::MAX_DOCUMENT_BYTES {
                    self.editors[pane].send(SCI_UNDO, 0, 0);
                    return Err(
                        "The edit was undone because it exceeded the 128 MiB document limit."
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
                if self.json_document == Some(doc.snapshot.id) {
                    self.json_document = None;
                }
                if self.tree_visible && index == self.index() {
                    self.schedule_json();
                }
                if self.comparing {
                    for editor in self.editors {
                        editor.clear_diff();
                    }
                    self.differences.clear();
                    self.compare_due = Some(Instant::now() + Duration::from_millis(350));
                    self.note = "Updating comparison...".into();
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
            status: null_mut(),
            tools: Vec::new(),
            tooltips: None,
            tree: null_mut(),
            search: SearchBar {
                query: null_mut(),
                replace: null_mut(),
                mode: null_mut(),
                case: null_mut(),
                word: null_mut(),
                buttons: Vec::new(),
                visible: false,
            },
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
        for snapshot in session.documents {
            app.add_document(snapshot)?;
        }
        if !app.documents.is_empty() {
            app.switch(active.min(app.documents.len() - 1))?;
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
        Ok(())
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
