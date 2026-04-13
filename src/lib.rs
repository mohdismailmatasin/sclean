#![deny(warnings)]
#![doc = include_str!("../README.md")]

pub mod cleaner;
pub mod config;
pub mod error;

pub use cleaner::{
    clean_directory, clean_journal_logs, clean_orphans, clean_system_logs, format_size,
    remove_empty_dirs, remove_lock_file, CleanResult,
};
pub use config::{CleanTarget, Config, TargetType};
pub use error::Error;
