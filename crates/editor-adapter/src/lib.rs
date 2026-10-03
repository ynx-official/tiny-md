//! Guise-based editor integration for tiny-md.
//!
//! The editor view is a small, attributed fork. Its model, Markdown parsing,
//! source mapping, theme, and highlighting remain supplied by Guise.
mod chord;
mod code_blocks;
mod diagrams;
mod editmenu;
mod ime;
mod markdown_editor;
mod tables;
mod unicode;

pub use guise::editor::Pos;
pub use markdown_editor::{MarkdownEditor, MarkdownEditorEvent, MarkdownStyle};
