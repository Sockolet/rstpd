#![cfg(windows)]

use rstpd::editor::sci::{SCI_GETLENGTH, SCI_GETMODIFY, SCI_GOTOPOS};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    ptr::{null, null_mut},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{Controls::TCM_GETITEMCOUNT, WindowsAndMessaging::*},
};

fn wait(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "Instance launch did not complete"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn command(directory: &Path, working_directory: &Path, paths: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rstpd"));
    command
        .current_dir(working_directory)
        .arg("--session-dir")
        .arg(directory)
        .args(paths);
    command
}

struct Running {
    process: Child,
    hwnd: HWND,
}

impl Running {
    fn start(directory: &Path, working_directory: &Path, paths: &[&str]) -> Self {
        Self::start_command(command(directory, working_directory, paths))
    }

    fn start_command(mut command: Command) -> Self {
        let process = command.spawn().unwrap();
        let mut app = Self {
            process,
            hwnd: null_mut(),
        };
        let class: Vec<_> = "rstpd.Window".encode_utf16().chain(Some(0)).collect();
        wait(|| unsafe {
            let mut hwnd = null_mut();
            loop {
                hwnd = FindWindowExW(null_mut(), hwnd, class.as_ptr(), null());
                if hwnd.is_null() {
                    assert!(
                        app.process.try_wait().unwrap().is_none(),
                        "Editor exited during startup"
                    );
                    return false;
                }
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, &mut pid);
                if pid == app.process.id() && IsWindowVisible(hwnd) != 0 {
                    app.hwnd = hwnd;
                    return true;
                }
            }
        });
        app
    }

    fn tabs(&self) -> usize {
        unsafe { SendMessageW(GetDlgItem(self.hwnd, 302), TCM_GETITEMCOUNT, 0, 0) as usize }
    }

    fn editor(&self) -> HWND {
        unsafe { GetDlgItem(self.hwnd, 101) }
    }

    fn close(&mut self) {
        unsafe { PostMessageW(self.hwnd, WM_CLOSE, 0, 0) };
        wait(|| self.process.try_wait().unwrap().is_some());
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if self.process.try_wait().ok().flatten().is_none() {
            let _ = self.process.kill();
            let _ = self.process.wait();
        }
    }
}

fn forward(directory: &Path, working_directory: &Path, paths: &[&str]) {
    finish_launch(command(directory, working_directory, paths));
}

fn finish_launch(mut command: Command) {
    let process = command.spawn().unwrap();
    let mut launch = Running {
        process,
        hwnd: null_mut(),
    };
    let mut status = None;
    wait(|| {
        status = launch.process.try_wait().unwrap();
        status.is_some()
    });
    assert!(status.unwrap().success(), "Forwarding process failed");
}

struct DirectoryCleanup(PathBuf);

impl Drop for DirectoryCleanup {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn open_with_reuses_the_matching_instance_and_preserves_unsaved_documents() {
    let directory = std::env::temp_dir().join(format!(
        "rstpd-instance-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let _cleanup = DirectoryCleanup(directory.clone());
    let state = directory.join("state");
    let other_state = directory.join("other-state");
    fs::write(directory.join("first.txt"), "original\n").unwrap();
    fs::write(directory.join("file with spaces-\u{65e5}.txt"), "unicode\n").unwrap();
    fs::write(directory.join("third.txt"), "third\n").unwrap();
    let mut app = Running::start(&state, &directory, &["first.txt"]);
    let mut other = Running::start(&other_state, &directory, &["third.txt"]);
    wait(|| app.tabs() == 1 && other.tabs() == 1);
    unsafe {
        let length = SendMessageW(app.editor(), SCI_GETLENGTH, 0, 0);
        SendMessageW(app.editor(), SCI_GOTOPOS, length as usize, 0);
        SendMessageW(app.editor(), WM_CHAR, b'X' as usize, 0);
        assert_eq!(SendMessageW(app.editor(), SCI_GETLENGTH, 0, 0), length + 1);
        assert_ne!(SendMessageW(app.editor(), SCI_GETMODIFY, 0, 0), 0);
        ShowWindow(app.hwnd, SW_MINIMIZE);
    }
    forward(&state.join("."), &directory, &["first.txt"]);
    wait(|| unsafe { IsIconic(app.hwnd) == 0 });
    assert_eq!(app.tabs(), 1);
    unsafe {
        assert_eq!(SendMessageW(app.editor(), SCI_GETLENGTH, 0, 0), 10);
        assert_ne!(SendMessageW(app.editor(), SCI_GETMODIFY, 0, 0), 0);
    }
    forward(
        &state,
        &directory,
        &["file with spaces-\u{65e5}.txt", "third.txt"],
    );
    wait(|| app.tabs() == 3);
    assert_eq!(
        other.tabs(),
        1,
        "A different workspace must not receive the files"
    );
    forward(&state, &directory, &[]);
    assert_eq!(
        app.tabs(),
        3,
        "A no-argument relaunch must only activate the window"
    );
    app.close();
    other.close();
    assert_eq!(
        fs::read_to_string(directory.join("first.txt")).unwrap(),
        "original\n"
    );
    let recovered = rstpd::session::load(&state.join("session.json")).unwrap();
    assert!(
        recovered
            .documents
            .iter()
            .any(|doc| doc.text == "original\nX" && doc.dirty)
    );

    let local_app_data = directory.join("local-app-data");
    let default_command = |paths: &[&str]| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rstpd"));
        command
            .current_dir(&directory)
            .env("LOCALAPPDATA", &local_app_data)
            .args(paths);
        command
    };
    let mut default_app = Running::start_command(default_command(&["first.txt"]));
    finish_launch(default_command(&["file with spaces-\u{65e5}.txt"]));
    wait(|| default_app.tabs() == 2);
    default_app.close();
    let default_session =
        rstpd::session::load(&local_app_data.join("rstpd").join("session.json")).unwrap();
    assert_eq!(
        default_session.documents.len(),
        2,
        "Open with without --session-dir must reuse the default workspace"
    );
}
