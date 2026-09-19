#![windows_subsystem = "windows"]

fn main() {
    if let Err(error) = rstpad::ui::run() {
        rstpad::ui::show_error(&error);
    }
}
