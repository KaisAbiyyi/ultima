//! Ultimate AI - Application Entry Point
//!
//! This is the main entry point for the Tauri desktop application.
//! The actual application logic is in lib.rs.

// Prevents additional console window on Windows in release
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

fn main() {
    ultima_lib::run()
}
