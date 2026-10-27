//! Guise-based editor integration for tiny-md.
//!
//! The editor view is a small, attributed fork. Its model, Markdown parsing,
//! source mapping and theme remain supplied by Guise; code highlighting uses
//! Syntect grammars with Guise's theme palette.
mod caret;
mod chord;
mod code_blocks;
mod commands;
mod diagram_cache;
mod diagram_image;
mod diagram_layout;
mod diagram_raster;
mod diagram_work;
mod diagrams;
mod editmenu;
mod external_text;
mod html;
mod ime;
mod markdown_editor;
mod markdown_images;
mod mermaid_parse;
mod render_cache;
mod shaped_text;
mod syntax;
mod tables;
mod unicode;

pub use commands::{BlockStyle, EditorCommand};
pub use diagram_image::DiagramImage;
pub use diagrams::{Diagram, RasterSize};
pub use guise::editor::Pos;
pub use markdown_editor::{MarkdownEditor, MarkdownEditorEvent, MarkdownStyle, RenderWork};
pub use tables::{Alignment as TableAlignment, TableCommand};
