#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod icon;
mod theme;

fn main() -> eframe::Result {
    app::run()
}
