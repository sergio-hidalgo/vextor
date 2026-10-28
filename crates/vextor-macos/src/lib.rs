//! Vextor's macOS hook: the files Finder asks Vextor to open (double-click,
//! the Dock icon, "Open With"), handed over as paths on a channel.
//!
//! macOS does not pass these files on the command line; it sends an "open
//! documents" Apple event, which eframe and winit do not forward (spec 14b).
//! This is the only crate where `unsafe` is allowed, and only in
//! `open_documents`. It is empty on other platforms.

#[cfg(target_os = "macos")]
mod open_documents;

#[cfg(target_os = "macos")]
pub use open_documents::install;
