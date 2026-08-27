#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    inkwell_desktop_lib::run();
}
