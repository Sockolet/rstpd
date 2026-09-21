use crate::{
    core::{EditorFont, MAX_DOCUMENT_BYTES, Result},
    languages::{self, Language},
    syntax::{self, Role},
};
use std::{
    ffi::{CString, c_void},
    marker::PhantomData,
    ops::Range,
    ptr::{NonNull, null_mut},
    rc::Rc,
};
use windows_sys::Win32::{
    Foundation::{HINSTANCE, HWND},
    UI::{Input::KeyboardAndMouse::SetFocus, WindowsAndMessaging::*},
};

#[allow(dead_code)]
pub mod sci {
    include!(concat!(env!("OUT_DIR"), "/scintilla.rs"));
}
use sci::*;

unsafe extern "C" {
    fn Scintilla_RegisterClasses(instance: HINSTANCE) -> i32;
    fn rstpd_document_release(document: *mut c_void);
}
unsafe extern "system" {
    fn CreateLexer(name: *const i8) -> *mut c_void;
    fn GetLexerCount() -> i32;
    fn GetLexerName(index: u32, name: *mut i8, len: i32);
}

/// # Safety
/// `instance` must be the current executable's valid module handle.
pub unsafe fn register(instance: HINSTANCE) -> Result<()> {
    if unsafe { Scintilla_RegisterClasses(instance) } == 0 {
        return Err("Could not initialize the built-in editor.".into());
    }
    Ok(())
}

pub fn available_lexers() -> Vec<String> {
    let count = unsafe { GetLexerCount() };
    (0..count)
        .map(|i| {
            let mut name = [0u8; 128];
            unsafe {
                GetLexerName(i as u32, name.as_mut_ptr().cast(), name.len() as i32);
            }
            let end = name.iter().position(|c| *c == 0).unwrap_or(name.len());
            String::from_utf8_lossy(&name[..end]).into_owned()
        })
        .collect()
}

pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub background: u32,
    pub panel: u32,
    pub text: u32,
    pub muted: u32,
    pub border: u32,
    pub accent: u32,
    pub selection: u32,
}
impl Palette {
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                dark,
                background: rgb(27, 29, 34),
                panel: rgb(35, 38, 44),
                text: rgb(221, 226, 235),
                muted: rgb(142, 151, 165),
                border: rgb(53, 58, 68),
                accent: rgb(113, 177, 255),
                selection: rgb(49, 72, 102),
            }
        } else {
            Self {
                dark,
                background: rgb(255, 255, 255),
                panel: rgb(244, 246, 249),
                text: rgb(32, 40, 53),
                muted: rgb(104, 115, 131),
                border: rgb(219, 225, 234),
                accent: rgb(0, 95, 184),
                selection: rgb(207, 229, 255),
            }
        }
    }
    fn syntax(self, role: &str) -> u32 {
        self.token_color(syntax::role(role).unwrap_or_default())
    }
    fn token_color(self, role: Role) -> u32 {
        match role {
            Role::Text => self.text,
            Role::Comment => {
                if self.dark {
                    rgb(123, 164, 117)
                } else {
                    rgb(51, 120, 69)
                }
            }
            Role::String => {
                if self.dark {
                    rgb(222, 166, 133)
                } else {
                    rgb(157, 57, 44)
                }
            }
            Role::Keyword | Role::Strong => {
                if self.dark {
                    rgb(156, 185, 255)
                } else {
                    rgb(0, 79, 169)
                }
            }
            Role::Number => {
                if self.dark {
                    rgb(189, 210, 163)
                } else {
                    rgb(113, 78, 142)
                }
            }
            Role::Directive | Role::Variable => {
                if self.dark {
                    rgb(196, 153, 218)
                } else {
                    rgb(127, 62, 150)
                }
            }
            Role::Type => {
                if self.dark {
                    rgb(78, 201, 176)
                } else {
                    rgb(0, 112, 104)
                }
            }
            Role::Function | Role::Constant => {
                if self.dark {
                    rgb(220, 205, 134)
                } else {
                    rgb(119, 85, 0)
                }
            }
            Role::Attribute => {
                if self.dark {
                    rgb(142, 205, 250)
                } else {
                    rgb(31, 89, 147)
                }
            }
            Role::Tag => {
                if self.dark {
                    rgb(205, 151, 218)
                } else {
                    rgb(135, 58, 142)
                }
            }
            Role::Error => {
                if self.dark {
                    rgb(255, 126, 139)
                } else {
                    rgb(180, 35, 48)
                }
            }
            Role::Code => {
                if self.dark {
                    rgb(232, 185, 130)
                } else {
                    rgb(150, 68, 22)
                }
            }
            Role::Emphasis => {
                if self.dark {
                    rgb(210, 177, 236)
                } else {
                    rgb(126, 60, 150)
                }
            }
            Role::Quote => {
                if self.dark {
                    rgb(152, 179, 206)
                } else {
                    rgb(79, 104, 130)
                }
            }
            Role::Muted => self.muted,
            Role::Heading | Role::Link | Role::Marker | Role::Operator => self.accent,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Editor {
    pub(crate) hwnd: HWND,
    _ui_thread: PhantomData<Rc<()>>,
}

pub struct DocumentHandle {
    raw: NonNull<c_void>,
    _ui_thread: PhantomData<Rc<()>>,
}

impl Drop for DocumentHandle {
    fn drop(&mut self) {
        // The owned reference survives view destruction; release through the pinned public interface.
        unsafe {
            rstpd_document_release(self.raw.as_ptr());
        }
    }
}

impl Editor {
    /// # Safety
    /// Handles must be valid and belong to the calling UI thread; register first.
    /// The control and its parent must stay alive for all uses of this view.
    pub unsafe fn new(parent: HWND, instance: HINSTANCE, id: usize, visible: bool) -> Result<Self> {
        let class: Vec<u16> = "Scintilla\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_TABSTOP | if visible { WS_VISIBLE } else { 0 },
                0,
                0,
                1,
                1,
                parent,
                id as _,
                instance,
                null_mut(),
            )
        };
        if hwnd.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let editor = Self {
            hwnd,
            _ui_thread: PhantomData,
        };
        for (message, value) in [
            (SCI_SETCODEPAGE, 65001),
            (SCI_SETTABWIDTH, 4),
            (SCI_SETINDENT, 4),
            (SCI_SETUSETABS, 0),
            (SCI_SETMULTIPLESELECTION, 1),
            (SCI_SETADDITIONALSELECTIONTYPING, 1),
            (SCI_SETMULTIPASTE, 1),
            (SCI_SETRECTANGULARSELECTIONMODIFIER, 4),
            (SCI_SETVIRTUALSPACEOPTIONS, 1),
            (SCI_SETSCROLLWIDTHTRACKING, 1),
            (SCI_SETSCROLLWIDTH, 1),
            (SCI_SETCARETLINEVISIBLE, 1),
            (SCI_SETCARETWIDTH, 2),
            (SCI_SETTECHNOLOGY, 1),
            (SCI_SETLAYOUTCACHE, 2),
            (SCI_AUTOCSETIGNORECASE, 1),
            (SCI_AUTOCSETORDER, 1),
            (SCI_AUTOCSETMULTI, 1),
            (SCI_AUTOCSETMAXHEIGHT, 10),
            (SCI_SETMOUSEDWELLTIME, 500),
            (SCI_SETMODEVENTMASK, 3),
        ] {
            editor.send(message, value, 0);
        }
        editor.send(SCI_SETMARGINTYPEN, 0, 1);
        editor.send(SCI_SETMARGINWIDTHN, 0, 52);
        editor.send(SCI_SETMARGINWIDTHN, 1, 0);
        editor.send(SCI_SETMARGINWIDTHN, 2, 16);
        editor.send(SCI_SETMARGINMASKN, 2, 0xfe000000_u32 as isize);
        editor.send(SCI_SETMARGINSENSITIVEN, 2, 1);
        editor.send(SCI_SETAUTOMATICFOLD, 7, 0);
        for (marker, symbol) in [
            (25, 13),
            (26, 15),
            (27, 11),
            (28, 10),
            (29, 9),
            (30, 12),
            (31, 14),
        ] {
            editor.send(SCI_MARKERDEFINE, marker, symbol);
        }
        Ok(editor)
    }
    pub fn send(self, message: u32, w: usize, l: isize) -> isize {
        assert!(
            SCALAR_MESSAGES.binary_search(&message).is_ok(),
            "Pointer-bearing Scintilla message {message} requires a typed wrapper."
        );
        unsafe { self.send_raw(message, w, l) }
    }
    /// # Safety
    /// Pointer arguments must satisfy the selected Scintilla message's lifetime, size,
    /// ownership and mutability requirements. Only call on the control's UI thread.
    pub unsafe fn send_raw(self, message: u32, w: usize, l: isize) -> isize {
        assert!(
            unsafe { IsWindow(self.hwnd) } != 0,
            "Editor window is no longer alive."
        );
        unsafe { SendMessageW(self.hwnd, message, w, l) }
    }
    pub fn create_document(self) -> Result<DocumentHandle> {
        let raw = unsafe { self.send_raw(SCI_CREATEDOCUMENT, 0, 0) } as *mut c_void;
        Ok(DocumentHandle {
            raw: NonNull::new(raw).ok_or("Could not allocate document.")?,
            _ui_thread: PhantomData,
        })
    }
    pub fn attach(self, document: &DocumentHandle) {
        if unsafe { self.send_raw(SCI_GETDOCPOINTER, 0, 0) } != document.raw.as_ptr() as isize {
            unsafe {
                self.send_raw(SCI_SETDOCPOINTER, 0, document.raw.as_ptr() as isize);
            }
        }
        self.send(SCI_SETCODEPAGE, 65001, 0);
    }
    pub fn length(self) -> usize {
        self.send(SCI_GETLENGTH, 0, 0) as usize
    }
    pub fn position(self) -> usize {
        self.send(SCI_GETCURRENTPOS, 0, 0) as usize
    }
    pub fn text(self) -> Result<String> {
        let len = self.length();
        let mut bytes = vec![0u8; len + 1];
        unsafe {
            self.send_raw(SCI_GETTEXT, bytes.len(), bytes.as_mut_ptr() as isize);
        }
        bytes.truncate(len);
        String::from_utf8(bytes)
            .map_err(|e| format!("The editor buffer contains invalid UTF-8: {e}"))
    }
    fn validate_range(self, range: &Range<usize>) -> Result<()> {
        if range.start > range.end || range.end > self.length() {
            return Err("Text range is outside the document.".into());
        }
        for position in [range.start, range.end] {
            let byte = self.send(SCI_GETCHARAT, position, 0) as u8;
            if byte & 0xc0 == 0x80 {
                return Err("Text range splits a UTF-8 character.".into());
            }
        }
        Ok(())
    }
    pub fn range(self, range: Range<usize>) -> Result<String> {
        self.validate_range(&range)?;
        #[repr(C)]
        struct TextRange {
            start: isize,
            end: isize,
            text: *mut u8,
        }
        let mut bytes = vec![0u8; range.len() + 1];
        let mut data = TextRange {
            start: range.start as isize,
            end: range.end as isize,
            text: bytes.as_mut_ptr(),
        };
        unsafe {
            self.send_raw(
                SCI_GETTEXTRANGEFULL,
                0,
                (&mut data as *mut TextRange) as isize,
            );
        }
        bytes.truncate(range.len());
        String::from_utf8(bytes)
            .map_err(|e| format!("The editor range contains invalid UTF-8: {e}"))
    }
    pub fn set_text(self, text: &str) -> Result<()> {
        if text.len() > MAX_DOCUMENT_BYTES {
            return Err("Text exceeds the document size limit.".into());
        }
        self.send(SCI_SETSTATUS, 0, 0);
        self.send(SCI_CLEARALL, 0, 0);
        unsafe {
            self.send_raw(SCI_ADDTEXT, text.len(), text.as_ptr() as isize);
        }
        self.send(SCI_EMPTYUNDOBUFFER, 0, 0);
        self.send(SCI_SETSAVEPOINT, 0, 0);
        self.send(SCI_GOTOPOS, 0, 0);
        self.check_status()
    }
    fn check_status(self) -> Result<()> {
        match self.send(SCI_GETSTATUS, 0, 0) {
            0 => Ok(()),
            status => Err(format!("Native editor operation failed (status {status}).")),
        }
    }
    pub fn replace(self, range: Range<usize>, text: &str) -> Result<()> {
        self.validate_range(&range)?;
        if self.length().saturating_sub(range.len()) + text.len() > MAX_DOCUMENT_BYTES {
            return Err("This edit would exceed the 128 MiB document limit.".into());
        }
        self.send(SCI_SETSTATUS, 0, 0);
        self.send(SCI_SETTARGETSTART, range.start, 0);
        self.send(SCI_SETTARGETEND, range.end, 0);
        unsafe {
            self.send_raw(SCI_REPLACETARGET, text.len(), text.as_ptr() as isize);
        }
        self.check_status()
    }
    pub fn replace_all(self, text: &str) -> Result<()> {
        self.send(SCI_BEGINUNDOACTION, 0, 0);
        let result = self.replace(0..self.length(), text);
        self.send(SCI_ENDUNDOACTION, 0, 0);
        result
    }
    pub fn select(self, range: Range<usize>) {
        self.send(SCI_SETSEL, range.start, range.end as isize);
        self.send(SCI_SCROLLCARET, 0, 0);
    }
    pub fn selection(self) -> Range<usize> {
        self.send(SCI_GETSELECTIONSTART, 0, 0) as usize
            ..self.send(SCI_GETSELECTIONEND, 0, 0) as usize
    }
    pub fn transform_selections(self, transform: impl Fn(&str) -> String) -> Result<()> {
        let mut edits = Vec::new();
        for i in 0..self.send(SCI_GETSELECTIONS, 0, 0) as usize {
            let range = self.send(SCI_GETSELECTIONNSTART, i, 0) as usize
                ..self.send(SCI_GETSELECTIONNEND, i, 0) as usize;
            if !range.is_empty() {
                edits.push((range.clone(), transform(&self.range(range)?)));
            }
        }
        if edits.is_empty() {
            return Err("Select text first. Alt+drag makes a rectangular selection.".into());
        }
        edits.sort_by_key(|(r, _)| r.start);
        let total = edits
            .iter()
            .fold(self.length(), |len, (r, s)| len - r.len() + s.len());
        if total > MAX_DOCUMENT_BYTES {
            return Err("This edit would exceed 128 MiB.".into());
        }
        self.send(SCI_BEGINUNDOACTION, 0, 0);
        let result = edits
            .into_iter()
            .rev()
            .try_for_each(|(range, text)| self.replace(range, &text));
        self.send(SCI_ENDUNDOACTION, 0, 0);
        result
    }
    pub fn language(self, language: &Language, palette: Palette) -> Result<()> {
        self.language_with_font(language, palette, &EditorFont::default())
    }
    pub fn language_with_font(
        self,
        language: &Language,
        palette: Palette,
        font: &EditorFont,
    ) -> Result<()> {
        self.clear_indicator(crate::markdown::STRIKE_INDICATOR);
        if language.uses_container() {
            unsafe {
                self.send_raw(SCI_SETILEXER, 0, 0);
            }
            self.theme_with_font(language, palette, font);
            return Ok(());
        }
        let name = CString::new(language.lexer.as_str()).map_err(|e| e.to_string())?;
        let lexer = unsafe { CreateLexer(name.as_ptr()) };
        if lexer.is_null() {
            return Err(format!(
                "Built-in lexer '{}' is unavailable.",
                language.lexer
            ));
        }
        unsafe {
            self.send_raw(SCI_SETILEXER, 0, lexer as isize);
        }
        for (i, set) in languages::keywords(language).iter().enumerate() {
            let set = CString::new(set.as_str()).map_err(|e| e.to_string())?;
            unsafe {
                self.send_raw(SCI_SETKEYWORDS, i, set.as_ptr() as isize);
            }
        }
        self.property("fold", "1");
        self.property(
            "asp.default.language",
            if language.name == "ASP" { "2" } else { "1" },
        );
        self.property("lexer.cpp.track.preprocessor", "0");
        if language.lexer == "cpp" {
            self.property("lexer.cpp.escape.sequence", "1");
            self.property(
                "lexer.cpp.backquoted.strings",
                if matches!(
                    language.name.as_str(),
                    "JavaScript" | "TypeScript" | "Go" | "Kotlin"
                ) {
                    "1"
                } else {
                    "0"
                },
            );
            self.property(
                "lexer.cpp.triplequoted.strings",
                if matches!(language.name.as_str(), "Swift" | "Kotlin") {
                    "1"
                } else {
                    "0"
                },
            );
            self.property(
                "lexer.cpp.allow.dollars",
                if matches!(
                    language.name.as_str(),
                    "JavaScript" | "TypeScript" | "Java" | "C#"
                ) {
                    "1"
                } else {
                    "0"
                },
            );
        }
        if language.lexer == "json" {
            self.property("lexer.json.escape.sequence", "1");
        }
        self.property(
            "lexer.json.allow.comments",
            if language.sample.ends_with(".jsonc") {
                "1"
            } else {
                "0"
            },
        );
        self.theme_with_font(language, palette, font);
        Ok(())
    }
    fn property(self, name: &str, value: &str) {
        let name = CString::new(name).expect("static property");
        let value = CString::new(value).expect("static property");
        unsafe {
            self.send_raw(
                SCI_SETPROPERTY,
                name.as_ptr() as usize,
                value.as_ptr() as isize,
            );
        }
    }
    pub fn theme(self, language: &Language, palette: Palette) {
        self.theme_with_font(language, palette, &EditorFont::default());
    }
    pub fn theme_with_font(self, language: &Language, palette: Palette, font: &EditorFont) {
        self.send(SCI_STYLESETFORE, 32, palette.text as isize);
        self.send(SCI_STYLESETBACK, 32, palette.background as isize);
        self.send(SCI_STYLESETBOLD, 32, 0);
        self.send(SCI_STYLESETITALIC, 32, 0);
        self.send(SCI_STYLESETUNDERLINE, 32, 0);
        self.send(SCI_STYLESETEOLFILLED, 32, 0);
        let default_font = language
            .custom
            .as_ref()
            .and_then(|custom| custom.styles[0].font.as_deref())
            .unwrap_or(font.family());
        let family = CString::new(default_font).expect("validated font family");
        unsafe {
            self.send_raw(SCI_STYLESETFONT, 32, family.as_ptr() as isize);
        }
        let size = language
            .custom
            .as_ref()
            .and_then(|custom| custom.styles[0].font_size)
            .map(|size| u32::from(size) * 100)
            .unwrap_or(font.size_hundredths());
        self.send(SCI_STYLESETSIZEFRACTIONAL, 32, size as isize);
        self.send(SCI_STYLECLEARALL, 0, 0);
        let count = self.send(SCI_GETNAMEDSTYLES, 0, 0).clamp(0, 256) as usize;
        let base_styles = if language.lexer == "markdown" {
            crate::markdown::styles()
        } else {
            syntax::styles(&language.lexer)
        };
        for (i, mut style) in base_styles.into_iter().enumerate() {
            if (32..=39).contains(&i) {
                continue;
            }
            let mut tags = [0u8; 1024];
            let len = if i < count {
                unsafe { self.send_raw(SCI_TAGSOFSTYLE, i, 0) }.max(0) as usize
            } else {
                0
            };
            if len > 0 && len < tags.len() {
                unsafe {
                    self.send_raw(SCI_TAGSOFSTYLE, i, tags.as_mut_ptr() as isize);
                }
                if let Ok(tags) = std::str::from_utf8(&tags[..len])
                    && let Some(role) = syntax::role(tags)
                    && (style.role == Role::Text || role != Role::Keyword)
                {
                    style.role = role;
                }
            }
            self.send(
                SCI_STYLESETFORE,
                i,
                palette.token_color(style.role) as isize,
            );
            self.send(
                SCI_STYLESETBACK,
                i,
                if style.shaded {
                    palette.panel
                } else {
                    palette.background
                } as isize,
            );
            self.send(SCI_STYLESETBOLD, i, style.bold as isize);
            self.send(SCI_STYLESETITALIC, i, style.italic as isize);
            self.send(SCI_STYLESETUNDERLINE, i, style.underline as isize);
            self.send(
                SCI_STYLESETEOLFILLED,
                i,
                (language.lexer == "markdown" && i == crate::markdown::CODE_BLOCK as usize)
                    as isize,
            );
        }
        self.send(SCI_STYLESETFORE, 33, palette.muted as isize);
        self.send(SCI_STYLESETBACK, 33, palette.panel as isize);
        self.send(SCI_SETCARETFORE, palette.text as usize, 0);
        self.send(SCI_SETSELFORE, 0, 0);
        self.send(SCI_SETSELBACK, 1, palette.selection as isize);
        self.send(SCI_SETADDITIONALSELBACK, palette.selection as usize, 0);
        self.send(SCI_SETCARETLINEBACK, palette.panel as usize, 0);
        self.send(SCI_SETFOLDMARGINCOLOUR, 1, palette.panel as isize);
        self.send(SCI_SETFOLDMARGINHICOLOUR, 1, palette.panel as isize);
        for id in 25..=31 {
            self.send(SCI_MARKERSETFORE, id, palette.muted as isize);
            self.send(SCI_MARKERSETBACK, id, palette.panel as isize);
        }
        for (id, color) in [
            (
                20,
                if palette.dark {
                    rgb(64, 43, 48)
                } else {
                    rgb(255, 226, 228)
                },
            ),
            (
                21,
                if palette.dark {
                    rgb(34, 60, 48)
                } else {
                    rgb(221, 246, 230)
                },
            ),
        ] {
            self.send(SCI_MARKERDEFINE, id, 22);
            self.send(SCI_MARKERSETBACK, id, color as isize);
        }
        for (id, color) in [(20, rgb(220, 85, 95)), (21, rgb(55, 175, 105))] {
            self.send(SCI_INDICSETSTYLE, id, 7);
            self.send(SCI_INDICSETFORE, id, color as isize);
            self.send(SCI_INDICSETALPHA, id, 100);
            self.send(SCI_INDICSETOUTLINEALPHA, id, 180);
            self.send(SCI_INDICSETUNDER, id, 1);
        }
        self.send(SCI_CALLTIPSETBACK, palette.panel as usize, 0);
        self.send(SCI_CALLTIPSETFORE, palette.text as usize, 0);
        self.send(SCI_CALLTIPSETFOREHLT, palette.accent as usize, 0);
        self.send(SCI_INDICSETSTYLE, crate::markdown::STRIKE_INDICATOR, 4);
        self.send(
            SCI_INDICSETFORE,
            crate::markdown::STRIKE_INDICATOR,
            palette.muted as isize,
        );
        self.send(SCI_INDICSETUNDER, crate::markdown::STRIKE_INDICATOR, 0);
        if let Some(custom) = &language.custom {
            for (id, style) in custom.styles.iter().enumerate() {
                let default_role = match id {
                    1 | 2 => "comment",
                    3 => "number",
                    4..=11 | 22..=24 => "keyword",
                    14..=21 => "string",
                    12 => "operator",
                    _ => "default",
                };
                let convert =
                    |color: u32| rgb((color >> 16) as u8, (color >> 8) as u8, color as u8);
                self.send(
                    SCI_STYLESETFORE,
                    id,
                    style
                        .foreground
                        .map(convert)
                        .unwrap_or(palette.syntax(default_role)) as isize,
                );
                self.send(
                    SCI_STYLESETBACK,
                    id,
                    style.background.map(convert).unwrap_or(palette.background) as isize,
                );
                self.send(SCI_STYLESETBOLD, id, (style.font_style & 1 != 0) as isize);
                self.send(SCI_STYLESETITALIC, id, (style.font_style & 2 != 0) as isize);
                self.send(
                    SCI_STYLESETUNDERLINE,
                    id,
                    (style.font_style & 4 != 0) as isize,
                );
                if let Some(font) = &style.font {
                    let font = CString::new(font.as_str()).expect("validated font family");
                    unsafe {
                        self.send_raw(SCI_STYLESETFONT, id, font.as_ptr() as isize);
                    }
                }
                if let Some(size) = style.font_size {
                    self.send(SCI_STYLESETSIZEFRACTIONAL, id, size as isize * 100);
                }
            }
        }
        self.send(SCI_COLOURISE, 0, -1);
        self.update_line_number_margin();
    }
    pub fn update_line_number_margin(self) {
        let digits = self.send(SCI_GETLINECOUNT, 0, 0).to_string().len().max(4);
        let sample = CString::new("9".repeat(digits)).expect("line-number digits");
        let measured = unsafe { self.send_raw(SCI_TEXTWIDTH, 33, sample.as_ptr() as isize) };
        let width = (measured + 12).max(52);
        if self.send(SCI_GETMARGINWIDTHN, 0, 0) != width {
            self.send(SCI_SETMARGINWIDTHN, 0, width);
        }
    }
    fn context(self) -> Result<(usize, usize, String)> {
        let pos = self.position();
        let mut begin = pos.saturating_sub(32_768);
        while self.send(SCI_GETCHARAT, begin, 0) as u8 & 0xc0 == 0x80 {
            begin += 1;
        }
        let mut end = (pos + 32_768).min(self.length());
        while self.send(SCI_GETCHARAT, end, 0) as u8 & 0xc0 == 0x80 {
            end += 1;
        }
        Ok((begin, pos, self.range(begin..end)?))
    }
    pub fn complete(
        self,
        language: &Language,
        extra: &[crate::completion::Api],
        manual: bool,
    ) -> Result<()> {
        let (begin, pos, text) = self.context()?;
        let suggestions = crate::completion::suggestions(
            &text,
            pos - begin,
            &language.name,
            &languages::keywords(language),
            extra,
            manual,
        );
        if !suggestions.words.is_empty() {
            let list = CString::new(suggestions.words.join(" ")).expect("word list without NUL");
            unsafe {
                self.send_raw(SCI_AUTOCSHOW, suggestions.entered, list.as_ptr() as isize);
            }
        } else if manual {
            self.send(SCI_AUTOCCANCEL, 0, 0);
        }
        Ok(())
    }
    pub fn call_tip(self, language: &Language, extra: &[crate::completion::Api]) -> Result<()> {
        let (begin, pos, text) = self.context()?;
        if let Some(tip) = crate::completion::call_tip(&text, pos - begin, &language.name, extra) {
            let signature = CString::new(tip.signature)
                .map_err(|_| "Function signature contains a NUL character.")?;
            unsafe {
                self.send_raw(
                    SCI_CALLTIPSHOW,
                    begin + tip.anchor,
                    signature.as_ptr() as isize,
                );
            }
            self.send(
                SCI_CALLTIPSETHLT,
                tip.parameter.start,
                tip.parameter.end as isize,
            );
        } else {
            self.send(SCI_CALLTIPCANCEL, 0, 0);
        }
        Ok(())
    }
    pub fn highlight(self, result: &crate::udl::Highlight) -> Result<()> {
        if result.styles.len() != self.length() {
            return Err("Highlight result no longer matches the document.".into());
        }
        self.send(SCI_STARTSTYLING, 0, 0);
        unsafe {
            self.send_raw(
                SCI_SETSTYLINGEX,
                result.styles.len(),
                result.styles.as_ptr() as isize,
            );
        }
        for (line, level) in result.folds.iter().enumerate() {
            self.send(SCI_SETFOLDLEVEL, line, *level as isize);
        }
        self.clear_indicator(crate::markdown::STRIKE_INDICATOR);
        for range in &result.strikes {
            self.indicator(crate::markdown::STRIKE_INDICATOR, range.clone())?;
        }
        Ok(())
    }
    pub fn clear_styles(self) {
        self.send(SCI_STARTSTYLING, 0, 0);
        self.send(SCI_SETSTYLING, self.length(), 0);
        self.clear_indicator(crate::markdown::STRIKE_INDICATOR);
    }
    pub fn indicator(self, id: usize, range: Range<usize>) -> Result<()> {
        self.validate_range(&range)?;
        self.send(SCI_SETINDICATORCURRENT, id, 0);
        self.send(SCI_INDICATORFILLRANGE, range.start, range.len() as isize);
        Ok(())
    }
    pub fn clear_indicator(self, id: usize) {
        self.send(SCI_SETINDICATORCURRENT, id, 0);
        self.send(SCI_INDICATORCLEARRANGE, 0, self.length() as isize);
    }
    pub fn clear_diff(self) {
        for id in [20, 21] {
            self.send(SCI_MARKERDELETEALL, id, 0);
            self.clear_indicator(id);
        }
    }
    pub fn focus(self) {
        unsafe {
            SetFocus(self.hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_language_has_a_static_lexer() {
        let available = available_lexers();
        for language in languages::catalog(&available) {
            assert!(
                available.contains(&language.lexer),
                "Missing lexer for {}: {}",
                language.name,
                language.lexer
            );
        }
    }
}
