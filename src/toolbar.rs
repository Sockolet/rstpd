use crate::{core::Result, editor::Palette};
use std::{
    mem::zeroed,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::Gdi::*,
    UI::{Controls::*, HiDpi::GetDpiForWindow, WindowsAndMessaging::*},
};

#[derive(Clone, Copy, Debug)]
pub enum Icon {
    New,
    Open,
    Save,
    Find,
    Split,
    Compare,
    Json,
    Map,
}

pub struct Button {
    pub command: usize,
    pub name: &'static str,
    pub tooltip: &'static str,
    pub icon: Icon,
}

pub struct Tooltips {
    pub hwnd: HWND,
    owner: HWND,
    texts: Vec<Vec<u16>>,
}
impl Tooltips {
    /// # Safety
    /// The owner must be a live window on the calling UI thread.
    pub unsafe fn new(owner: HWND) -> Result<Self> {
        let class: Vec<u16> = "tooltips_class32\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST,
                class.as_ptr(),
                null(),
                WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                owner,
                null_mut(),
                null_mut(),
                null(),
            )
        };
        if hwnd.is_null() {
            return Err("Could not create toolbar tooltips.".into());
        }
        unsafe {
            SendMessageW(hwnd, TTM_SETMAXTIPWIDTH, 0, 400);
            SendMessageW(hwnd, TTM_SETDELAYTIME, TTDT_INITIAL as usize, 400);
        }
        Ok(Self {
            hwnd,
            owner,
            texts: Vec::new(),
        })
    }
    /// # Safety
    /// The button must be a live child of the owner, on the same UI thread.
    pub unsafe fn add(&mut self, button: HWND, text: &str) -> Result<()> {
        self.texts
            .push(text.encode_utf16().chain(Some(0)).collect());
        let mut info: TTTOOLINFOW = unsafe { zeroed() };
        // The unused reserved field is v6-only; the v2 layout works with both common-control versions.
        info.cbSize = std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32;
        info.uFlags = TTF_IDISHWND | TTF_SUBCLASS;
        info.hwnd = self.owner;
        info.uId = button as usize;
        info.lpszText = self.texts.last_mut().unwrap().as_mut_ptr();
        if unsafe {
            SendMessageW(
                self.hwnd,
                TTM_ADDTOOLW,
                0,
                (&info as *const TTTOOLINFOW) as isize,
            )
        } == 0
        {
            return Err("Could not attach a toolbar tooltip.".into());
        }
        Ok(())
    }
}
impl Drop for Tooltips {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(self.hwnd) != 0 {
                DestroyWindow(self.hwnd);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Op {
    M(f32, f32),
    L(f32, f32),
    C(f32, f32, f32, f32, f32, f32),
    Close,
}
use Op::*;

struct Canvas {
    dc: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    pen: HPEN,
    old_pen: HGDIOBJ,
    old_brush: HGDIOBJ,
    size: i32,
}
impl Canvas {
    unsafe fn new(target: HDC, size: i32, foreground: u32, background: u32) -> Result<Self> {
        let mut canvas = Self {
            dc: unsafe { CreateCompatibleDC(target) },
            bitmap: null_mut(),
            old_bitmap: null_mut(),
            pen: null_mut(),
            old_pen: null_mut(),
            old_brush: null_mut(),
            size,
        };
        if canvas.dc.is_null() {
            return Err("Could not create an icon drawing context.".into());
        }
        canvas.bitmap = unsafe { CreateCompatibleBitmap(target, size, size) };
        if canvas.bitmap.is_null() {
            return Err("Could not allocate an icon bitmap.".into());
        }
        canvas.old_bitmap = unsafe { SelectObject(canvas.dc, canvas.bitmap) };
        let brush = unsafe { CreateSolidBrush(background) };
        if brush.is_null() {
            return Err("Could not allocate an icon background brush.".into());
        }
        unsafe {
            FillRect(
                canvas.dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: size,
                    bottom: size,
                },
                brush,
            );
            DeleteObject(brush);
        }
        let brush = LOGBRUSH {
            lbStyle: BS_SOLID,
            lbColor: foreground,
            lbHatch: 0,
        };
        canvas.pen = unsafe {
            ExtCreatePen(
                (PS_GEOMETRIC | PS_SOLID | PS_ENDCAP_ROUND | PS_JOIN_ROUND) as u32,
                ((size as f32 / 24.0) * 2.0).round().max(1.0) as u32,
                &brush,
                0,
                null(),
            )
        };
        if canvas.pen.is_null() {
            return Err("Could not allocate an icon stroke.".into());
        }
        canvas.old_pen = unsafe { SelectObject(canvas.dc, canvas.pen) };
        canvas.old_brush = unsafe { SelectObject(canvas.dc, GetStockObject(NULL_BRUSH)) };
        Ok(canvas)
    }
    fn point(&self, x: f32, y: f32) -> POINT {
        let scale = self.size as f32 / 24.0;
        POINT {
            x: (x * scale).round() as i32,
            y: (y * scale).round() as i32,
        }
    }
    unsafe fn path(&self, ops: &[Op]) -> Result<()> {
        unsafe {
            if BeginPath(self.dc) == 0 {
                return Err("Could not start an icon path.".into());
            }
            for op in ops {
                let ok = match *op {
                    M(x, y) => {
                        let p = self.point(x, y);
                        MoveToEx(self.dc, p.x, p.y, null_mut())
                    }
                    L(x, y) => {
                        let p = self.point(x, y);
                        LineTo(self.dc, p.x, p.y)
                    }
                    C(a, b, c, d, x, y) => {
                        let points = [self.point(a, b), self.point(c, d), self.point(x, y)];
                        PolyBezierTo(self.dc, points.as_ptr(), 3)
                    }
                    Close => CloseFigure(self.dc),
                };
                if ok == 0 {
                    AbortPath(self.dc);
                    return Err("Could not draw an icon path.".into());
                }
            }
            if EndPath(self.dc) == 0 || StrokePath(self.dc) == 0 {
                return Err("Could not finish an icon path.".into());
            }
        }
        Ok(())
    }
    unsafe fn round_box(&self) -> Result<()> {
        unsafe {
            self.path(&[
                M(5., 3.),
                L(19., 3.),
                C(20.105, 3., 21., 3.895, 21., 5.),
                L(21., 19.),
                C(21., 20.105, 20.105, 21., 19., 21.),
                L(5., 21.),
                C(3.895, 21., 3., 20.105, 3., 19.),
                L(3., 5.),
                C(3., 3.895, 3.895, 3., 5., 3.),
                Close,
            ])
        }
    }
    // Native vector adaptations of the pinned Lucide artwork; see assets\icons\LICENSE.txt.
    unsafe fn icon(&self, icon: Icon) -> Result<()> {
        unsafe {
            match icon {
                Icon::New => {
                    self.path(&[
                        M(11.35, 22.),
                        L(6., 22.),
                        C(4.895, 22., 4., 21.105, 4., 20.),
                        L(4., 4.),
                        C(4., 2.895, 4.895, 2., 6., 2.),
                        L(14., 2.),
                        C(14.637, 2., 15.256, 2.256, 15.706, 2.706),
                        L(19.294, 6.294),
                        C(19.744, 6.744, 20., 7.363, 20., 8.),
                        L(20., 13.35),
                    ])?;
                    self.path(&[
                        M(14., 2.),
                        L(14., 7.),
                        C(14., 7.552, 14.448, 8., 15., 8.),
                        L(20., 8.),
                        M(14., 19.),
                        L(20., 19.),
                        M(17., 16.),
                        L(17., 22.),
                    ])?;
                }
                Icon::Open => self.path(&[
                    M(6., 14.),
                    L(7.5, 11.1),
                    C(7.844, 10.426, 8.507, 10., 9.24, 10.),
                    L(20., 10.),
                    C(21.306, 10., 22.264, 11.23, 21.94, 12.5),
                    L(20.4, 18.5),
                    C(20.175, 19.384, 19.36, 20., 18.45, 20.),
                    L(4., 20.),
                    C(2.895, 20., 2., 19.105, 2., 18.),
                    L(2., 5.),
                    C(2., 3.895, 2.895, 3., 4., 3.),
                    L(7.9, 3.),
                    C(8.58, 3., 9.23, 3.34, 9.59, 3.9),
                    L(10.4, 5.1),
                    C(10.77, 5.68, 11.4, 6., 12.07, 6.),
                    L(18., 6.),
                    C(19.105, 6., 20., 6.895, 20., 8.),
                    L(20., 10.),
                ])?,
                Icon::Save => {
                    self.path(&[
                        M(15.2, 3.),
                        C(15.73, 3., 16.225, 3.225, 16.6, 3.6),
                        L(20.4, 7.4),
                        C(20.775, 7.775, 21., 8.27, 21., 8.8),
                        L(21., 19.),
                        C(21., 20.105, 20.105, 21., 19., 21.),
                        L(5., 21.),
                        C(3.895, 21., 3., 20.105, 3., 19.),
                        L(3., 5.),
                        C(3., 3.895, 3.895, 3., 5., 3.),
                        Close,
                    ])?;
                    self.path(&[
                        M(17., 21.),
                        L(17., 14.),
                        C(17., 13.448, 16.552, 13., 16., 13.),
                        L(8., 13.),
                        C(7.448, 13., 7., 13.448, 7., 14.),
                        L(7., 21.),
                        M(7., 3.),
                        L(7., 7.),
                        C(7., 7.552, 7.448, 8., 8., 8.),
                        L(15., 8.),
                    ])?;
                }
                Icon::Find => {
                    self.path(&[M(21., 21.), L(16.66, 16.66)])?;
                    let a = self.point(3., 3.);
                    let b = self.point(19., 19.);
                    if Ellipse(self.dc, a.x, a.y, b.x, b.y) == 0 {
                        return Err("Could not draw the search icon.".into());
                    }
                }
                Icon::Split => {
                    self.round_box()?;
                    self.path(&[M(12., 3.), L(12., 21.)])?;
                }
                Icon::Compare => self.path(&[
                    M(12., 3.),
                    L(12., 17.),
                    M(5., 10.),
                    L(19., 10.),
                    M(5., 21.),
                    L(19., 21.),
                ])?,
                Icon::Json => self.path(&[
                    M(8., 5.),
                    L(21., 5.),
                    M(13., 12.),
                    L(21., 12.),
                    M(13., 19.),
                    L(21., 19.),
                    M(3., 10.),
                    C(3., 11.105, 3.895, 12., 5., 12.),
                    L(8., 12.),
                    M(3., 5.),
                    L(3., 17.),
                    C(3., 18.105, 3.895, 19., 5., 19.),
                    L(8., 19.),
                ])?,
                Icon::Map => {
                    self.round_box()?;
                    self.path(&[M(15., 3.), L(15., 21.)])?;
                }
            }
        }
        Ok(())
    }
}
impl Drop for Canvas {
    fn drop(&mut self) {
        unsafe {
            if !self.old_brush.is_null() {
                SelectObject(self.dc, self.old_brush);
            }
            if !self.old_pen.is_null() {
                SelectObject(self.dc, self.old_pen);
            }
            if !self.old_bitmap.is_null() {
                SelectObject(self.dc, self.old_bitmap);
            }
            if !self.pen.is_null() {
                DeleteObject(self.pen);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
        }
    }
}

/// # Safety
/// The draw-item structure and its device context must belong to an active WM_DRAWITEM call.
pub unsafe fn draw(item: &DRAWITEMSTRUCT, icon: Icon, palette: Palette, hot: bool) -> Result<()> {
    unsafe {
        let selected = item.itemState & ODS_SELECTED != 0;
        let disabled = item.itemState & ODS_DISABLED != 0;
        let highlighted = (selected || hot) && !disabled;
        let background = if highlighted {
            palette.selection
        } else {
            palette.panel
        };
        let foreground = if disabled {
            palette.muted
        } else if highlighted {
            palette.accent
        } else {
            palette.text
        };
        let brush = CreateSolidBrush(background);
        if brush.is_null() {
            return Err("Could not paint a toolbar button.".into());
        }
        FillRect(item.hDC, &item.rcItem, brush);
        DeleteObject(brush);
        let dpi = GetDpiForWindow(item.hwndItem).max(96) as i32;
        let size = (20 * dpi / 96).min((item.rcItem.right - item.rcItem.left - 6).max(1));
        let canvas = Canvas::new(item.hDC, size * 4, foreground, background)?;
        canvas.icon(icon)?;
        let left = item.rcItem.left + (item.rcItem.right - item.rcItem.left - size) / 2;
        let top = item.rcItem.top + (item.rcItem.bottom - item.rcItem.top - size) / 2;
        let old_mode = SetStretchBltMode(item.hDC, HALFTONE);
        let mut old_origin: POINT = zeroed();
        SetBrushOrgEx(item.hDC, 0, 0, &mut old_origin);
        let copied = StretchBlt(
            item.hDC,
            left,
            top,
            size,
            size,
            canvas.dc,
            0,
            0,
            canvas.size,
            canvas.size,
            SRCCOPY,
        );
        SetBrushOrgEx(item.hDC, old_origin.x, old_origin.y, null_mut());
        SetStretchBltMode(item.hDC, old_mode);
        if copied == 0 {
            return Err("Could not display a toolbar icon.".into());
        }
        if item.itemState & ODS_FOCUS != 0 {
            let mut focus = item.rcItem;
            InflateRect(&mut focus, -3, -3);
            DrawFocusRect(item.hDC, &focus);
        }
        Ok(())
    }
}
