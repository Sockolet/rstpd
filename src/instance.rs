use crate::{
    core::Result,
    launch::{self, Request},
};
use std::{
    path::Path,
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    System::{DataExchange::COPYDATASTRUCT, Threading::GetCurrentProcessId},
    UI::WindowsAndMessaging::*,
};

pub const COPYDATA_ID: usize = 0x52535401;

fn property(directory: &Path) -> Result<Vec<u16>> {
    Ok(format!("rstpd.Session.{}", launch::session_key(directory)?)
        .encode_utf16()
        .chain(Some(0))
        .collect())
}

pub(crate) struct Endpoint {
    hwnd: HWND,
    property: Vec<u16>,
}

impl Endpoint {
    pub(crate) fn new(hwnd: HWND, directory: &Path) -> Result<Self> {
        let property = property(directory)?;
        if unsafe { SetPropW(hwnd, property.as_ptr(), std::ptr::without_provenance_mut(1)) } == 0 {
            return Err(format!(
                "Could not register file launch forwarding: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self { hwnd, property })
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        unsafe { RemovePropW(self.hwnd, self.property.as_ptr()) };
    }
}

pub fn forward(directory: &Path, request: &Request) -> Result<()> {
    let property = property(directory)?;
    let bytes = request.encode()?;
    let class: Vec<u16> = "rstpd.Window".encode_utf16().chain(Some(0)).collect();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut hwnd = null_mut();
        unsafe {
            loop {
                hwnd = FindWindowExW(null_mut(), hwnd, class.as_ptr(), null());
                if hwnd.is_null() {
                    break;
                }
                if GetPropW(hwnd, property.as_ptr()).is_null() {
                    continue;
                }
                let mut process = 0;
                GetWindowThreadProcessId(hwnd, &mut process);
                if process == GetCurrentProcessId() {
                    continue;
                }
                AllowSetForegroundWindow(process);
                let data = COPYDATASTRUCT {
                    dwData: COPYDATA_ID,
                    cbData: bytes.len() as u32,
                    lpData: bytes.as_ptr().cast_mut().cast(),
                };
                let mut accepted = 0;
                if SendMessageTimeoutW(
                    hwnd,
                    WM_COPYDATA,
                    0,
                    std::ptr::from_ref(&data) as isize,
                    SMTO_ABORTIFHUNG | SMTO_BLOCK | SMTO_ERRORONEXIT,
                    5000,
                    &mut accepted,
                ) == 0
                {
                    return Err(format!(
                        "The running editor did not respond to the file launch: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                return if accepted == 1 {
                    Ok(())
                } else {
                    Err("The running editor rejected the file launch. Close an older version or retry after it finishes its current operation.".into())
                };
            }
        }
        if Instant::now() >= deadline {
            return Err("The session is in use, but its editor is not ready for file launches. Close an older version first, or retry after startup finishes.".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
