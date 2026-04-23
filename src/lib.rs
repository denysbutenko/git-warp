//! Git-Warp: High-performance, UX-focused Git worktree manager
//!
//! This crate combines Copy-on-Write (CoW) filesystem operations with advanced Git worktree
//! management to provide fast, reliable development environment setup.

pub mod agents;
pub mod config;
pub mod cow;
pub mod error;
pub mod git;
pub mod hooks;
pub mod post_create;
pub mod process;
pub mod rewrite;
pub mod terminal;
pub mod tui;

pub use error::{GitWarpError, Result};
