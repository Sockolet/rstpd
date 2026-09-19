#![cfg(windows)]
use rstpad::{
    editor::Palette,
    toolbar::{self, Icon, Tooltips},
};
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::*, WindowsAndMessaging::*},
};

#[test]
fn icons_draw_visible_artwork_and_keep_named_tooltips() {
    unsafe {
        let instance = GetModuleHandleW(null());
        let classes = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        };
        assert_ne!(InitCommonControlsEx(&classes), 0);
        let parent_class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let button_class: Vec<u16> = "BUTTON\0".encode_utf16().collect();
        let parent = CreateWindowExW(
            0,
            parent_class.as_ptr(),
            null(),
            WS_OVERLAPPED,
            0,
            0,
            600,
            200,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        assert!(!parent.is_null());
        let mut tooltips = Tooltips::new(parent).unwrap();
        let screen = GetDC(null_mut());
        let dc = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, 64, 64);
        let old = SelectObject(dc, bitmap);
        let icons = [
            Icon::New,
            Icon::Open,
            Icon::Save,
            Icon::Find,
            Icon::Split,
            Icon::Compare,
            Icon::Json,
            Icon::Map,
        ];
        let names = [
            "New",
            "Open",
            "Save",
            "Find",
            "Split",
            "Compare",
            "JSON tree",
            "Document map",
        ];
        let mut buttons = Vec::new();
        for (index, (icon, name)) in icons.into_iter().zip(names).enumerate() {
            let caption: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let button = CreateWindowExW(
                0,
                button_class.as_ptr(),
                caption.as_ptr(),
                WS_CHILD | BS_OWNERDRAW as u32,
                0,
                0,
                64,
                64,
                parent,
                (100 + index) as _,
                instance,
                null(),
            );
            assert!(!button.is_null());
            buttons.push(button);
            tooltips.add(button, &format!("{name} action")).unwrap();
            for dark in [false, true] {
                for hot in [false, true] {
                    let palette = Palette::new(dark);
                    let item = DRAWITEMSTRUCT {
                        CtlType: ODT_BUTTON,
                        hwndItem: button,
                        hDC: dc,
                        rcItem: windows_sys::Win32::Foundation::RECT {
                            left: 0,
                            top: 0,
                            right: 64,
                            bottom: 64,
                        },
                        ..zeroed()
                    };
                    toolbar::draw(&item, icon, palette, hot).unwrap();
                    let background = if hot {
                        palette.selection
                    } else {
                        palette.panel
                    };
                    let mut ink = 0;
                    for y in 0..64 {
                        for x in 0..64 {
                            if GetPixel(dc, x, y) != background {
                                ink += 1;
                            }
                        }
                    }
                    assert!(ink > 35, "{name} icon is blank");
                    assert!(ink < 900, "{name} icon is clipped or fills its button");
                }
            }
        }
        assert_eq!(SendMessageW(tooltips.hwnd, TTM_GETTOOLCOUNT, 0, 0), 8);
        for (button, name) in buttons.into_iter().zip(names) {
            let mut text = [0u16; 256];
            let mut info: TTTOOLINFOW = zeroed();
            info.cbSize = std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32;
            info.hwnd = parent;
            info.uId = button as usize;
            info.lpszText = text.as_mut_ptr();
            SendMessageW(
                tooltips.hwnd,
                TTM_GETTEXTW,
                text.len(),
                (&mut info as *mut TTTOOLINFOW) as isize,
            );
            let end = text.iter().position(|ch| *ch == 0).unwrap();
            assert_eq!(
                String::from_utf16(&text[..end]).unwrap(),
                format!("{name} action")
            );
            GetWindowTextW(button, text.as_mut_ptr(), text.len() as i32);
            let end = text.iter().position(|ch| *ch == 0).unwrap();
            assert_eq!(String::from_utf16(&text[..end]).unwrap(), name);
        }
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(null_mut(), screen);
        DestroyWindow(parent);
        drop(tooltips);
    }
}
