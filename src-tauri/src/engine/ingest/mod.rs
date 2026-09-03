//! File-format ingest paths that need more than a plain table import.

pub mod docs;
#[cfg(feature = "xlsx")]
pub mod excel;
