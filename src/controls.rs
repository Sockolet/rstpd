use crate::{
    core::Result,
    editor::{Palette, rgb},
};
use std::mem::zeroed;
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::*,
    UI::{
        Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_SELECTED},
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled},
        WindowsAndMessaging::*,
    },
};

#[derive(Clone, Copy)]
pub struct Colors {
    pub field: u32,
    pub button: u32,
    pub hover: u32,
    pub border: u32,
    pub disabled_border: u32,
}
impl Colors {
    pub fn new(palette: Palette) -> Self {
        if palette.dark {
            Self {
                field: rgb(22, 25, 30),
                button: rgb(53, 58, 67),
                hover: rgb(61, 75, 94),
                border: rgb(137, 149, 166),
                disabled_border: rgb(75, 81, 92),
            }
        } else {
            Self {
                field: rgb(255, 255, 255),
                button: rgb(255, 255, 255),
                hover: rgb(229, 239, 252),
                border: rgb(105, 118, 136),
                disabled_border: rgb(185, 193, 205),
            }
        }
    }
}

unsafe fn fill(dc: HDC, rect: &RECT, color: u32, frame: bool) -> Result<()> {
    unsafe {
        let brush = CreateSolidBrush(color);
        if brush.is_null() {
            return Err("Could not allocate a control paint brush.".into());
        }
        let result = if frame {
            FrameRect(dc, rect, brush)
        } else {
            FillRect(dc, rect, brush)
        };
        DeleteObject(brush);
        if result == 0 {
            return Err("Could not paint a control surface.".into());
        }
        Ok(())
    }
}

/// # Safety
/// The item must be from an active WM_DRAWITEM call, with a valid DC and font.
pub unsafe fn draw_button(
    item: &DRAWITEMSTRUCT,
    title: &str,
    font: HFONT,
    palette: Palette,
    hot: bool,
) -> Result<()> {
    unsafe {
        let colors = Colors::new(palette);
        let disabled = item.itemState & ODS_DISABLED != 0;
        let pressed = item.itemState & ODS_SELECTED != 0 && !disabled;
        let focused = item.itemState & ODS_FOCUS != 0 && !disabled;
        let background = if disabled {
            palette.panel
        } else if pressed {
            palette.selection
        } else if hot {
            colors.hover
        } else {
            colors.button
        };
        let border = if disabled {
            colors.disabled_border
        } else if focused || hot || pressed {
            palette.accent
        } else {
            colors.border
        };
        fill(item.hDC, &item.rcItem, background, false)?;
        let dpi = GetDpiForWindow(item.hwndItem).max(96) as i32;
        let mut edge = item.rcItem;
        let thickness = ((if focused { 2 } else { 1 }) * dpi / 96).max(1);
        for _ in 0..thickness {
            fill(item.hDC, &edge, border, true)?;
            InflateRect(&mut edge, -1, -1);
        }
        let saved = SaveDC(item.hDC);
        if saved == 0 {
            return Err("Could not preserve a control drawing context.".into());
        }
        SelectObject(item.hDC, font);
        SetBkMode(item.hDC, TRANSPARENT as i32);
        SetTextColor(
            item.hDC,
            if disabled {
                palette.muted
            } else {
                palette.text
            },
        );
        let mut text_rect = item.rcItem;
        InflateRect(&mut text_rect, -(8 * dpi / 96), -2);
        if pressed {
            OffsetRect(&mut text_rect, (dpi / 96).max(1), (dpi / 96).max(1));
        }
        let text: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        DrawTextW(
            item.hDC,
            text.as_ptr(),
            (text.len() - 1) as i32,
            &mut text_rect,
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
        if focused {
            let mut focus = item.rcItem;
            InflateRect(&mut focus, -(4 * dpi / 96), -(4 * dpi / 96));
            DrawFocusRect(item.hDC, &focus);
        }
        RestoreDC(item.hDC, saved);
        Ok(())
    }
}

/// # Safety
/// `hwnd` must be a live bordered edit control owned by the current UI thread.
pub unsafe fn draw_field_border(hwnd: HWND, palette: Palette) -> Result<()> {
    unsafe {
        let mut rect: RECT = zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return Err("Could not measure a search field.".into());
        }
        let (left, top) = (rect.left, rect.top);
        OffsetRect(&mut rect, -left, -top);
        if rect.right <= 0 || rect.bottom <= 0 {
            return Ok(());
        }
        let dc = GetWindowDC(hwnd);
        if dc.is_null() {
            return Err("Could not paint a search-field border.".into());
        }
        let colors = Colors::new(palette);
        let enabled = IsWindowEnabled(hwnd) != 0;
        let focused = GetFocus() == hwnd && enabled;
        let color = if !enabled {
            colors.disabled_border
        } else if focused {
            palette.accent
        } else {
            colors.border
        };
        let mut result = Ok(());
        for _ in 0..if focused { 2 } else { 1 } {
            result = fill(dc, &rect, color, true);
            if result.is_err() {
                break;
            }
            InflateRect(&mut rect, -1, -1);
        }
        ReleaseDC(hwnd, dc);
        result
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub struct SearchLayout {
    pub height: i32,
    pub labels: [Bounds; 3],
    pub fields: [Bounds; 3],
    pub checks: [Bounds; 2],
    pub buttons: [Bounds; 7],
}
impl SearchLayout {
    pub fn new(width: i32, dpi: u32) -> Self {
        let dpi = dpi.max(96) as i32;
        let scale = |value: i32| value * dpi / 96;
        let compact = width < scale(1040);
        let make = |x: i32, row: i32, w: i32| Bounds {
            x: scale(x),
            y: scale(8 + row * 40),
            width: scale(w),
            height: scale(32),
        };
        let mut fields = [make(76, 0, 1), make(76, 1, 1), make(76, 2, 180)];
        for field in &mut fields[..2] {
            field.width = (width - scale(356)).max(1);
        }
        let mut buttons = [Bounds::default(); 7];
        for (index, right, row, w) in [
            (0, 272, 0, 80),
            (1, 184, 0, 80),
            (2, 272, 1, 80),
            (3, 184, 1, 96),
            (4, 92, 0, 80),
        ] {
            buttons[index] = Bounds {
                x: width - scale(right),
                ..make(0, row, w)
            };
        }
        buttons[5] = make(
            if compact { 76 } else { 528 },
            if compact { 3 } else { 2 },
            218,
        );
        buttons[6] = make(
            if compact { 302 } else { 754 },
            if compact { 3 } else { 2 },
            238,
        );
        Self {
            height: scale(if compact { 168 } else { 128 }),
            labels: [make(12, 0, 56), make(12, 1, 56), make(12, 2, 56)],
            fields,
            checks: [make(272, 2, 108), make(392, 2, 112)],
            buttons,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn contrast(a: u32, b: u32) -> f64 {
        fn luminance(color: u32) -> f64 {
            let c = |shift| {
                let value = ((color >> shift) & 255u32) as f64 / 255.0;
                if value <= 0.04045 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * c(0) + 0.7152 * c(8) + 0.0722 * c(16)
        }
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }
    #[test]
    fn fields_and_buttons_have_visible_boundaries_and_readable_text() {
        for dark in [false, true] {
            let p = Palette::new(dark);
            let c = Colors::new(p);
            assert_ne!(c.field, p.panel);
            for background in [c.field, c.button, c.hover, p.selection] {
                assert!(
                    contrast(p.text, background) >= 4.5,
                    "Text contrast is insufficient ({dark})"
                );
            }
            for background in [c.field, c.button, p.panel] {
                assert!(
                    contrast(c.border, background) >= 3.0,
                    "Boundary contrast is insufficient ({dark})"
                );
            }
            assert!(contrast(p.accent, c.field) >= 3.0);
            assert_ne!(c.button, c.hover);
            assert_ne!(c.hover, p.selection);
        }
    }
    #[test]
    fn search_controls_fit_without_overlap_at_small_sizes_and_high_dpi() {
        for dpi in [96, 120, 144, 192] {
            for logical_width in [740, 764, 780, 900, 1039, 1040, 1100, 1280, 1600] {
                let width = logical_width * dpi / 96;
                let layout = SearchLayout::new(width, dpi as u32);
                let controls: Vec<_> = layout
                    .labels
                    .into_iter()
                    .chain(layout.fields)
                    .chain(layout.checks)
                    .chain(layout.buttons)
                    .collect();
                for (index, a) in controls.iter().enumerate() {
                    assert!(
                        a.x >= 0
                            && a.y >= 0
                            && a.x + a.width <= width
                            && a.y + a.height <= layout.height,
                        "Out of bounds: {logical_width}/{dpi} {a:?}"
                    );
                    for b in &controls[index + 1..] {
                        let overlaps = a.x < b.x + b.width
                            && b.x < a.x + a.width
                            && a.y < b.y + b.height
                            && b.y < a.y + a.height;
                        assert!(
                            !overlaps,
                            "Overlapping controls: {logical_width}/{dpi} {a:?} {b:?}"
                        );
                    }
                }
                assert!(layout.fields[0].width >= 380 * dpi / 96);
            }
        }
    }
}
