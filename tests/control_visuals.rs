#![cfg(windows)]
use rstpd::{
    controls::{self, Colors},
    editor::Palette,
};
use std::{
    mem::zeroed,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::RECT,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::*, WindowsAndMessaging::*},
};

#[test]
fn text_buttons_have_borders_and_distinct_interaction_states() {
    unsafe {
        let instance = GetModuleHandleW(null());
        let class: Vec<_> = "STATIC\0".encode_utf16().collect();
        let button_class: Vec<_> = "BUTTON\0".encode_utf16().collect();
        let parent = CreateWindowExW(
            0,
            class.as_ptr(),
            null(),
            WS_OVERLAPPED,
            0,
            0,
            400,
            200,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        assert!(!parent.is_null());
        let button = CreateWindowExW(
            0,
            button_class.as_ptr(),
            null(),
            WS_CHILD | BS_OWNERDRAW as u32,
            0,
            0,
            240,
            64,
            parent,
            null_mut(),
            instance,
            null(),
        );
        assert!(!button.is_null());
        let screen = GetDC(null_mut());
        let dc = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, 240, 64);
        let old = SelectObject(dc, bitmap);
        let font = GetStockObject(DEFAULT_GUI_FONT);
        for dark in [false, true] {
            let palette = Palette::new(dark);
            let colors = Colors::new(palette);
            for (state, hot, surface, border) in [
                (0, false, colors.button, colors.border),
                (0, true, colors.hover, palette.accent),
                (ODS_SELECTED, false, palette.selection, palette.accent),
                (ODS_FOCUS, false, colors.button, palette.accent),
                (ODS_DISABLED, true, palette.panel, colors.disabled_border),
            ] {
                let item = DRAWITEMSTRUCT {
                    CtlType: ODT_BUTTON,
                    hwndItem: button,
                    hDC: dc,
                    rcItem: RECT {
                        left: 0,
                        top: 0,
                        right: 240,
                        bottom: 64,
                    },
                    itemState: state,
                    ..zeroed()
                };
                controls::draw_button(&item, "Find all", font, palette, hot).unwrap();
                assert_eq!(
                    GetPixel(dc, 0, 32),
                    border,
                    "Missing button edge for state {state}"
                );
                assert_eq!(
                    GetPixel(dc, 12, 12),
                    surface,
                    "Incorrect button surface for state {state}"
                );
                let mut text_pixels = 0;
                for y in 20..44 {
                    for x in 60..180 {
                        let color = GetPixel(dc, x, y);
                        if color != surface && color != border {
                            text_pixels += 1;
                        }
                    }
                }
                assert!(text_pixels > 20, "Button caption is not visibly rendered");
            }
        }
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(null_mut(), screen);
        DestroyWindow(parent);
    }
}
