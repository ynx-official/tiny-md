// Adapted from guise-ui 1.9.1 (MIT). See ../THIRD_PARTY.md.
//! `MarkdownEditor` — an Obsidian-style live-preview markdown editor
//! (gpui entity).
//!
//! Every line renders formatted — headings sized, emphasis styled, list
//! markers replaced by bullets/checkboxes, fenced code highlighted — while
//! the cursor line (and any line the selection touches) *reveals* its
//! markdown syntax for editing. Text soft-wraps; list items keep a hanging
//! indent; checkboxes toggle in place; links open on Ctrl+click (Cmd+click
//! on macOS, plain click when read-only) via [`MarkdownEditorEvent::LinkClick`].
//!
//! ```ignore
//! let editor = cx.new(|cx| {
//!     MarkdownEditor::new(cx).value("# Notes\n\n- [ ] try **guise**")
//! });
//! cx.subscribe(&editor, |_this, _editor, event: &MarkdownEditorEvent, _cx| {
//!     match event {
//!         MarkdownEditorEvent::Change(text) => { /* persist */ }
//!         MarkdownEditorEvent::LinkClick(target) => { /* open */ }
//!     }
//! })
//! .detach();
//! ```

use crate::caret::CaretBlink;
use crate::chord::Chord;
use crate::diagrams::{self, DiagramKey, DiagramState, DiagramView};
use crate::editmenu::{self, EditMenu};
use crate::ime::{self, ImeState};
use crate::render_cache::{self, DocumentCache, RowSlots};
use crate::syntax;
use crate::tables::{self, Alignment};
use gpui::prelude::*;
use gpui::{
    AnyElement, AnyView, App, Bounds, ClipboardItem, Context, Div, DragMoveEvent,
    ElementInputHandler, Empty, Entity, EntityId, EventEmitter, FocusHandle, Font, FontStyle,
    FontWeight, Hsla, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, Pixels, ScrollHandle,
    SharedString, StrikethroughStyle, StyleRefinement, Task, TextAlign, TextRun, UnderlineStyle,
    Window, canvas, div, point, px,
};
use guise::actions;
use guise::overlay::ContextMenu;

use crate::html::{Annotation, src_for_vis, vis_for_src};
use crate::shaped_text::ShapedText;
use guise::editor::{EditorModel, Pos, TokenKind, token_color};
use guise::markdown::block::{Block, DocState, classify};
use guise::markdown::layout::{RowKind, RowPlan, byte_for_col, col_for_byte, metrics};
use guise::reactive::Signal;
use guise::theme::theme;
use guise::{Glyph, IconName};
use std::{cell::Cell, rc::Rc};

/// The monospace family for code spans and code blocks.
const MONO_FAMILY: &str = if cfg!(target_os = "macos") {
    "Menlo"
} else if cfg!(target_os = "windows") {
    "Consolas"
} else {
    "DejaVu Sans Mono"
};
/// Horizontal padding around the document, in px.
const PAD_X: f32 = 16.0;
/// Vertical padding above and below the document, in px.
const PAD_Y: f32 = 12.0;
/// Wrap width used before the first layout pass has measured the content.
const DEFAULT_WRAP: f32 = 640.0;

enum ScrollRequest {
    RevealCursor,
    LineStart,
}

gpui::actions!(
    tiny_md_editor,
    [EditorTab, EditorTabPrevious, EditorTableMenu]
);
struct EditorKeyBindings;
impl gpui::Global for EditorKeyBindings {}

#[path = "editor_html.rs"]
mod editor_html;
#[path = "editor_images.rs"]
mod editor_images;
#[path = "editor_tables.rs"]
mod editor_tables;
#[path = "image_flow.rs"]
mod image_flow;
#[cfg(test)]
#[path = "image_regressions.rs"]
mod image_regressions;
#[path = "editor_layers.rs"]
mod layers;
#[path = "platform_input.rs"]
mod platform_input;
#[cfg(test)]
#[path = "render_regressions.rs"]
mod render_regressions;
#[cfg(test)]
#[path = "table_interaction_tests.rs"]
mod table_interaction_tests;

/// Emitted as the user edits or activates a link.
#[derive(Debug, Clone)]
pub enum MarkdownEditorEvent {
    /// The document changed. Carries the full new text.
    Change(String),
    /// A link was activated (Ctrl/Cmd+click, or plain click when read-only).
    /// Carries the target — a url or a wikilink page name.
    LinkClick(String),
    /// A read-only snapshot requested in a separate diagram window.
    ViewDiagram(diagrams::Diagram),
}

/// The drag payload for selection-by-mouse; tagged with the owning entity so
/// two editors in one window never react to each other's drags.
struct MarkdownDrag(EntityId);

/// Per-editor visual overrides. Unset fields fall back to the theme-derived
/// defaults, so an empty style changes nothing.
#[derive(Clone, Copy, Default)]
pub struct MarkdownStyle {
    /// Paint no frame border and no corner radius (an embedded surface).
    pub bare: bool,
    /// Reduce display heading padding without changing text or cursor positions.
    pub compact_headings: bool,
    /// Use half-height single blank separators in formatted mode.
    pub compact_blank_lines: bool,
    pub bg: Option<Hsla>,
    pub text: Option<Hsla>,
    pub caret: Option<Hsla>,
    pub quote_text: Option<Hsla>,
    pub quote_bar: Option<Hsla>,
    pub selection: Option<Hsla>,
    /// Links, bullets and checked boxes.
    pub accent: Option<Hsla>,
    pub code_bg: Option<Hsla>,
    pub placeholder: Option<Hsla>,
}

fn row_padding(kind: &RowKind, compact: bool) -> (f32, f32) {
    let m = metrics(kind);
    let factor = if compact && matches!(kind, RowKind::Heading(_)) {
        0.6
    } else {
        1.0
    };
    (m.pad_top * factor, m.pad_bottom * factor)
}

/// One laid-out source line: its plan, shaped text, and geometry. Rebuilt
/// every render; mouse and caret math read the copy from the last frame.
struct Row {
    html_table: Option<editor_html::TableLayout>,
    plan: RowPlan,
    /// `Rc` so the paint closure can share the shaped line with the row.
    text: Option<std::rc::Rc<ShapedText>>,
    line_h: f32,
    pad_top: f32,
    inset: f32,
    height: f32,
    y: Cell<f32>,
    cells: Vec<TableCell>,
    align: TextAlign,
    wrap: f32,
}

#[derive(Clone, PartialEq)]
struct LayoutStyle {
    font: Font,
    base: f32,
    wrap: f32,
    compact: bool,
    compact_blank_lines: bool,
    colors: Vec<Hsla>,
}

#[derive(Clone, PartialEq)]
struct RowKey {
    html: u64,
    block: Block,
    compact_blank: bool,
    code: u64,
    table: u64,
    reveal: bool,
    active_column: Option<usize>,
    selected: bool,
    hidden: bool,
    code_edges: u8,
    preview: u32,
    image_preview: u32,
}

#[derive(Default)]
struct ModeLayout {
    style: Option<LayoutStyle>,
    rows: RowSlots<RowKey, Row>,
}

/// Cumulative work counters for performance regression checks, without UI text.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderWork {
    pub document_renders: u64,
    pub parsed_documents: u64,
    pub shaped_rows: u64,
    pub shaped_cells: u64,
    pub built_rows: u64,
    pub cached_rows: u64,
}

struct TableCell {
    source: std::ops::Range<usize>,
    x: f32,
    width: f32,
    missing: bool,
    row: Row,
}

struct CellStyle {
    base: f32,
    font: Font,
    text: Hsla,
    dimmed: Hsla,
    accent: Hsla,
    code_bg: Hsla,
    highlight_bg: Hsla,
    comment: Hsla,
}

fn shape_table_cell(
    source: &str,
    header: bool,
    reveal: bool,
    width: f32,
    alignment: Alignment,
    style: &CellStyle,
    window: &mut Window,
) -> Row {
    let (plan, extras) = if source.contains(['<', '&']) {
        crate::html::fragment_plan(source, reveal)
    } else {
        let p = tables::cell_plan(source, reveal);
        let extras = vec![crate::html::Style::default(); p.runs.len()];
        (p, extras)
    };
    shape_planned_cell((plan, extras), header, width, alignment, style, window)
}

fn shape_planned_cell(
    (plan, extras): (RowPlan, Vec<crate::html::Style>),
    header: bool,
    width: f32,
    alignment: Alignment,
    style: &CellStyle,
    window: &mut Window,
) -> Row {
    let runs = plan
        .runs
        .iter()
        .zip(&extras)
        .map(|(run, extra)| TextRun {
            len: run.len,
            font: Font {
                family: if run.style.code || extra.annotation.is_some() {
                    MONO_FAMILY.into()
                } else {
                    style.font.family.clone()
                },
                weight: if extra.annotation.is_none() && (header || run.style.bold) {
                    FontWeight::BOLD
                } else {
                    style.font.weight
                },
                style: if run.style.italic {
                    FontStyle::Italic
                } else {
                    style.font.style
                },
                ..style.font.clone()
            },
            color: if let Some(annotation) = extra.annotation {
                match annotation {
                    Annotation::Comment => style.comment,
                    Annotation::Break => style.dimmed,
                }
            } else if run.marker {
                style.dimmed
            } else if run.style.link {
                style.accent
            } else {
                extra
                    .color
                    .map(|c| gpui::rgb(c).into())
                    .unwrap_or(style.text)
            },
            background_color: if run.style.highlight {
                Some(style.highlight_bg)
            } else if run.style.code {
                Some(style.code_bg)
            } else {
                extra.background.map(|c| gpui::rgb(c).into())
            },
            underline: (run.style.link || extra.underline).then(|| UnderlineStyle {
                thickness: px(1.0),
                color: Some(style.accent),
                wavy: false,
            }),
            strikethrough: run.style.strike.then(|| StrikethroughStyle {
                thickness: px(1.0),
                color: Some(style.dimmed),
            }),
        })
        .collect::<Vec<_>>();
    let inset = (width * 0.15).min(12.0);
    let wrap = (width - inset * 2.0).max(8.0);
    let size = (style.base - 1.0).max(12.0);
    let line_h = (size * 1.6).round();
    let text = ShapedText::new(&plan.visible, size, &runs, &extras, wrap, window).map(Rc::new);
    let height = text.as_ref().map_or(1, |text| text.visual_rows()) as f32 * line_h;
    Row {
        html_table: None,
        plan,
        text,
        line_h,
        pad_top: 0.0,
        inset,
        height,
        y: Cell::new(0.0),
        cells: Vec::new(),
        align: match alignment {
            Alignment::Left => TextAlign::Left,
            Alignment::Center => TextAlign::Center,
            Alignment::Right => TextAlign::Right,
        },
        wrap,
    }
}

impl Row {
    fn visual_rows(&self) -> usize {
        if !self.cells.is_empty() {
            return self
                .cells
                .iter()
                .map(|cell| cell.row.visual_rows())
                .max()
                .unwrap_or(1);
        }
        self.text.as_ref().map_or(1, |t| t.visual_rows())
    }

    /// Byte offsets where the shaped line wraps, ascending.
    fn boundaries(&self) -> Vec<usize> {
        let Some(text) = &self.text else {
            return Vec::new();
        };
        text.boundaries()
    }

    /// End-semantics position for a visible byte: a wrap-boundary byte maps
    /// to the end of the earlier visual row.
    fn pos_end(&self, vis: usize) -> (f32, f32) {
        let (x, y) = self.pos_end_raw(vis);
        (x + self.align_offset((y / self.line_h).round() as usize), y)
    }

    fn pos_end_raw(&self, vis: usize) -> (f32, f32) {
        let Some(text) = &self.text else {
            return (0.0, 0.0);
        };
        match text.position_for_index(vis.min(text.len()), px(self.line_h)) {
            Some(p) => (f32::from(p.x), f32::from(p.y)),
            None => (0.0, 0.0),
        }
    }

    /// Caret position for a visible byte: (x, visual row). Start semantics —
    /// a wrap-boundary byte belongs to the row it starts.
    fn caret(&self, vis: usize) -> (f32, usize) {
        if let Some(cell) = self.cell_for_source(vis) {
            let local = vis.saturating_sub(cell.source.start).min(cell.source.len());
            let (x, row) = cell.row.caret(vis_for_src(&cell.row.plan.segs, local));
            return (cell.x + cell.row.inset + x, row);
        }
        let bounds = self.boundaries();
        let row = bounds.iter().filter(|&&b| b <= vis).count();
        if bounds.contains(&vis) {
            return (self.align_offset(row), row);
        }
        let (x, y) = self.pos_end(vis);
        (
            x,
            if self.line_h > 0.0 {
                (y / self.line_h).round() as usize
            } else {
                0
            },
        )
    }

    /// Selection rectangles for the visible byte range: (x, visual row,
    /// width). `newline` extends the last rect to show a selected line end.
    fn sel_rects(&self, vs: usize, ve: usize, newline: bool, cell: f32) -> Vec<(f32, usize, f32)> {
        if !self.cells.is_empty() {
            let mut rects = Vec::new();
            for table_cell in &self.cells {
                if vs > table_cell.source.end || ve < table_cell.source.start {
                    continue;
                }
                let start = vs
                    .saturating_sub(table_cell.source.start)
                    .min(table_cell.source.len());
                let end = ve
                    .saturating_sub(table_cell.source.start)
                    .min(table_cell.source.len());
                let start = vis_for_src(&table_cell.row.plan.segs, start);
                let end = vis_for_src(&table_cell.row.plan.segs, end);
                rects.extend(
                    table_cell
                        .row
                        .sel_rects(start, end, false, cell)
                        .into_iter()
                        .map(|(x, row, width)| {
                            (x + table_cell.x + table_cell.row.inset, row, width)
                        }),
                );
            }
            if newline && let Some(last) = rects.last_mut() {
                last.2 += cell;
            }
            return rects;
        }
        let bounds = self.boundaries();
        let (sr, er) = split_visual(&bounds, vs, ve);
        let (sx, _) = if bounds.contains(&vs) {
            (self.align_offset(sr), 0.0)
        } else {
            self.pos_end(vs)
        };
        let (ex, _) = self.pos_end(ve);
        let row_end = |r: usize| -> f32 {
            match bounds.get(r) {
                Some(&b) => self.pos_end(b).0,
                None => self.pos_end(usize::MAX).0,
            }
        };
        let mut rects = Vec::new();
        if sr == er {
            rects.push((sx, sr, (ex - sx).max(0.0)));
        } else {
            rects.push((sx, sr, (row_end(sr) - sx).max(0.0)));
            for r in sr + 1..er {
                rects.push((0.0, r, row_end(r).max(0.0)));
            }
            rects.push((0.0, er, ex.max(0.0)));
        }
        if newline && let Some(last) = rects.last_mut() {
            last.2 += cell;
        }
        rects.retain(|r| r.2 > 0.0);
        rects
    }

    fn cell_for_source(&self, byte: usize) -> Option<&TableCell> {
        self.cells
            .iter()
            .find(|cell| byte <= cell.source.end)
            .or_else(|| self.cells.last())
    }

    /// Use the painted glyph ranges for both pointer feedback and link activation.
    /// Each wrapped visual row gets its own region; table cells retain alignment.
    fn link_regions(&self) -> Vec<(Bounds<Pixels>, String)> {
        if !self.cells.is_empty() {
            return self
                .cells
                .iter()
                .flat_map(|cell| {
                    cell.row
                        .link_regions()
                        .into_iter()
                        .map(|(mut bounds, target)| {
                            bounds.origin += point(px(cell.x), px(self.pad_top));
                            (bounds, target)
                        })
                })
                .collect();
        }
        self.plan
            .links
            .iter()
            .flat_map(|(source, target)| {
                let start = vis_for_src(&self.plan.segs, source.start);
                let end = vis_for_src(&self.plan.segs, source.end);
                let boundaries = self.boundaries();
                let starts = std::iter::once(0).chain(boundaries.iter().copied());
                let ends = boundaries
                    .iter()
                    .copied()
                    .chain(std::iter::once(self.plan.visible.len()));
                starts
                    .zip(ends)
                    .enumerate()
                    .filter_map(|(row, (row_start, row_end))| {
                        let clipped_start = start.max(row_start);
                        let clipped_end = end.min(row_end);
                        if clipped_start >= clipped_end {
                            return None;
                        }
                        let x = if clipped_start == row_start {
                            self.align_offset(row)
                        } else {
                            self.pos_end(clipped_start).0
                        };
                        let width = self.pos_end(clipped_end).0 - x;
                        (width > 0.0).then(|| {
                            (
                                Bounds::new(
                                    point(
                                        px(self.inset + x),
                                        px(self.pad_top + row as f32 * self.line_h),
                                    ),
                                    gpui::size(px(width), px(self.line_h)),
                                ),
                                target.clone(),
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn align_offset(&self, visual_row: usize) -> f32 {
        let factor = match self.align {
            TextAlign::Center => 0.5,
            TextAlign::Right => 1.0,
            _ => return 0.0,
        };
        let width = self
            .text
            .as_ref()
            .map_or(0.0, |text| text.visual_width(visual_row));
        (self.wrap - width).max(0.0) * factor
    }

    fn source_at(&self, x: f32, y: f32) -> usize {
        if let Some(cell) = self
            .cells
            .iter()
            .find(|cell| x < cell.x + cell.width)
            .or_else(|| self.cells.last())
        {
            let source = cell.row.source_at(x - cell.x, y - self.pad_top);
            return cell.source.start + tables::cell_source_boundary(&cell.row.plan, source);
        }
        let visual_row = ((y - self.pad_top).max(0.0) / self.line_h) as usize;
        let local = point(
            px((x - self.inset - self.align_offset(visual_row)).max(0.0)),
            px((y - self.pad_top).max(0.0)),
        );
        let visible = self.text.as_ref().map_or(0, |text| {
            text.closest_index_for_position(local, px(self.line_h))
                .unwrap_or_else(|near| near)
        });
        src_for_vis(&self.plan.segs, visible)
    }
}

/// An Obsidian-style live-preview markdown editor. Create with
/// `cx.new(|cx| MarkdownEditor::new(cx))`.
///
/// The text model is the unit-tested [`EditorModel`] (char-index cursor,
/// anchor selection, coalesced undo); markdown structure comes from the pure
/// [`block`](super::block)/[`inline`](super::inline)/[`layout`](super::layout)
/// passes. Read-only editors render pure preview — selection, copy, and
/// plain-click links still work.
pub struct MarkdownEditor {
    model: EditorModel,
    ime: ImeState,
    source_mode: bool,
    focus_mode: bool,
    typewriter: bool,
    placeholder: SharedString,
    read_only: bool,
    font_size: f32,
    rows: Option<usize>,
    style: MarkdownStyle,
    focus: FocusHandle,
    caret: CaretBlink,
    _caret_task: Task<()>,
    document_layer: Entity<layers::DocumentLayer>,
    caret_layer: Entity<layers::CaretLayer>,
    document_focused: bool,
    /// The right-click Cut / Copy / Paste menu, built on each right-click.
    menu: Option<Entity<ContextMenu>>,
    table_menu: Option<Entity<editor_tables::TableMenu>>,
    table_menu_changes: Option<gpui::Subscription>,
    scroll: ScrollHandle,
    /// Window-space bounds of the document content, captured at prepaint.
    text_bounds: Bounds<Pixels>,
    /// Content width from the last frame — the wrap width for this one.
    wrap_w: f32,
    /// Measured prose cell advance, for indent units and newline cells.
    cell_w: f32,
    /// Last frame's per-line layout, for mouse/caret/scroll math.
    layout: Vec<Rc<Row>>,
    document_cache: DocumentCache,
    mode_layouts: [ModeLayout; 2],
    table_widths: Vec<Vec<f32>>,
    table_signatures: Vec<u64>,
    table_wrap: f32,
    work: RenderWork,
    /// Apply navigation after rebuilding the current line geometry.
    scroll_request: Option<ScrollRequest>,
    /// Sticky x (content space) for visual-row vertical movement.
    goal_x: Option<f32>,
    diagrams: std::collections::HashMap<DiagramKey, DiagramState>,
    diagram_views: std::collections::HashMap<usize, DiagramView>,
    diagram_geometry_dirty: bool,
    diagram_scrollbar_dragging: Rc<Cell<bool>>,
    diagram_was_dragging: bool,
    code_highlights: syntax::Cache,
    copied_block: Option<usize>,
    images: editor_images::Images,
    html_details: std::collections::HashMap<u64, bool>,
}

impl EventEmitter<MarkdownEditorEvent> for MarkdownEditor {}

impl MarkdownEditor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.on_release(|this, cx| {
            this.images.clear(cx);
            // Wait until this editor's diagrams/tasks and child views have
            // dropped before pruning their weak source registrations.
            cx.defer(crate::diagram_cache::prune_idle);
        })
        .detach();
        if !cx.has_global::<EditorKeyBindings>() {
            cx.bind_keys([
                gpui::KeyBinding::new("tab", EditorTab, Some("TinyMdMarkdown")),
                gpui::KeyBinding::new("shift-tab", EditorTabPrevious, Some("TinyMdMarkdown")),
                gpui::KeyBinding::new("shift-f10", EditorTableMenu, Some("TinyMdMarkdown")),
            ]);
            editor_tables::bind_keys(cx);
            cx.set_global(EditorKeyBindings);
        }
        let executor = cx.background_executor().clone();
        let caret_layer = cx.new(|_| layers::CaretLayer::default());
        let editor = cx.entity();
        let document_layer = cx.new(|layer_cx| layers::DocumentLayer::new(editor, layer_cx));
        let caret_task = cx.spawn(async move |this, cx| {
            loop {
                executor.timer(std::time::Duration::from_millis(100)).await;
                if this
                    .update(cx, |this, cx| {
                        if this.caret.tick(this.ime.active()) && this.caret_is_visible(cx) {
                            let visible = this.caret.visible || this.ime.active();
                            this.caret_layer.update(cx, |layer, cx| {
                                layer.visible = visible;
                                cx.notify();
                            });
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        MarkdownEditor {
            model: EditorModel::new(""),
            ime: ImeState::default(),
            source_mode: false,
            focus_mode: false,
            typewriter: false,
            placeholder: SharedString::default(),
            read_only: false,
            font_size: 15.0,
            rows: None,
            style: MarkdownStyle::default(),
            focus: cx.focus_handle(),
            caret: CaretBlink::default(),
            _caret_task: caret_task,
            document_layer,
            caret_layer,
            document_focused: false,
            menu: None,
            table_menu: None,
            table_menu_changes: None,
            scroll: ScrollHandle::new(),
            text_bounds: Bounds::default(),
            wrap_w: DEFAULT_WRAP,
            cell_w: 15.0 * 0.55,
            layout: Vec::new(),
            document_cache: DocumentCache::default(),
            mode_layouts: Default::default(),
            table_widths: Vec::new(),
            table_signatures: Vec::new(),
            table_wrap: 0.0,
            work: RenderWork::default(),
            scroll_request: None,
            goal_x: None,
            diagrams: std::collections::HashMap::new(),
            diagram_views: std::collections::HashMap::new(),
            diagram_geometry_dirty: false,
            diagram_scrollbar_dragging: Rc::new(Cell::new(false)),
            diagram_was_dragging: false,
            code_highlights: syntax::Cache::default(),
            copied_block: None,
            images: editor_images::Images::default(),
            html_details: Default::default(),
        }
    }

    // builders

    /// Initial markdown text (`text()` is the getter).
    pub fn value(mut self, text: &str) -> Self {
        self.model.set_text(text);
        self.normalize_heading_cursor();
        self
    }

    /// Dimmed hint shown while the document is empty and unfocused.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Render pure preview: no caret, no edits, no syntax reveal. Selection,
    /// copy, and plain-click link activation still work.
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Base font size in px (default 15.0). Headings scale from it.
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    /// Minimum height, as a number of paragraph lines.
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = Some(rows);
        self
    }

    /// Spaces inserted per list indent level (default 4).
    pub fn tab_size(mut self, n: usize) -> Self {
        self.model.set_tab_size(n);
        self
    }

    /// Per-editor visual overrides (see [`MarkdownStyle`]).
    pub fn style(mut self, style: MarkdownStyle) -> Self {
        self.style = style;
        self
    }

    /// Replace the style at runtime (theme switches).
    pub fn set_style(&mut self, style: MarkdownStyle, cx: &mut Context<Self>) {
        self.style = style;
        cx.notify();
    }

    // runtime API

    /// The current markdown text.
    pub fn text(&self) -> String {
        self.model.text()
    }

    pub fn render_work(&self) -> RenderWork {
        self.work
    }

    pub fn copy_plain(&mut self, cx: &mut Context<Self>) {
        self.finish_composition(cx);
        if let Some(text) = self.model.copy() {
            cx.write_to_clipboard(ClipboardItem::new_string(crate::commands::plain_text(
                &text,
            )));
        }
    }

    pub fn block_label(&self) -> &'static str {
        let mut state = DocState::default();
        let block = self
            .model
            .lines()
            .iter()
            .take(self.model.cursor().line + 1)
            .map(|line| classify(line, &mut state))
            .last();
        match block {
            Some(Block::Heading { level: 1, .. }) => "一级标题",
            Some(Block::Heading { level: 2, .. }) => "二级标题",
            Some(Block::Heading { level: 3, .. }) => "三级标题",
            Some(Block::Heading { level: 4, .. }) => "四级标题",
            Some(Block::Heading { level: 5, .. }) => "五级标题",
            Some(Block::Heading { .. }) => "六级标题",
            Some(Block::Quote { .. }) => "引用",
            Some(Block::Bullet { .. } | Block::Ordered { .. } | Block::Task { .. }) => "列表",
            Some(Block::CodeLine | Block::Fence { .. }) => "代码块",
            Some(Block::Table) => "表格",
            _ => "段落",
        }
    }

    /// Replace the document, resetting cursor, selection, and history.
    pub fn set_text(&mut self, value: &str, cx: &mut Context<Self>) {
        self.html_details.clear();
        self.images.clear(cx);
        self.caret.reset();
        self.copied_block = None;
        self.diagrams.clear();
        self.diagram_views.clear();
        self.diagram_geometry_dirty = false;
        self.diagram_scrollbar_dragging.set(false);
        self.diagram_was_dragging = false;
        self.ime.reset();
        self.model.set_text(value);
        self.document_cache = DocumentCache::default();
        self.mode_layouts = Default::default();
        self.table_wrap = 0.0;
        self.layout.clear();
        self.normalize_heading_cursor();
        self.scroll.set_offset(point(px(0.0), px(0.0)));
        self.scroll_request = Some(ScrollRequest::RevealCursor);
        self.goal_x = None;
        cx.notify();
    }

    /// Apply a disk update as one undoable edit, retaining selection and scroll.
    /// Hosts must retry later while the platform owns an IME composition.
    pub fn apply_external_text(&mut self, value: &str, cx: &mut Context<Self>) -> bool {
        if self.ime.active() {
            return false;
        }
        if self.model.text() != value {
            crate::external_text::apply(&mut self.model, value);
            self.copied_block = None;
            self.goal_x = None;
            self.caret.reset();
            cx.emit(MarkdownEditorEvent::Change(self.model.text()));
            cx.notify();
        }
        true
    }

    /// The editor's focus handle, so a host can focus it on open.
    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// Share the document viewport with a host-owned scrollbar.
    pub fn scroll_handle(&self) -> ScrollHandle {
        self.scroll.clone()
    }

    /// Navigate to a source line and align its start near the viewport top.
    /// The next render uses fresh geometry and clamps at the document end.
    pub fn jump_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        self.edit(cx, |model| model.move_to(line, 0, false));
        self.goal_x = None;
        self.scroll_request = Some(ScrollRequest::LineStart);
    }

    /// Hosts report scrollbar thumb dragging so diagram sizes remain stable.
    pub fn scrollbar_drag_state(&self) -> Rc<Cell<bool>> {
        self.diagram_scrollbar_dragging.clone()
    }

    /// Toggle presentation without replacing the buffer, selection or history.
    pub fn set_source_mode(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.finish_composition(cx);
        self.source_mode = enabled;
        self.normalize_heading_cursor();
        self.scroll_request = Some(ScrollRequest::RevealCursor);
        cx.notify();
    }

    pub fn finish_composition(&mut self, cx: &mut Context<Self>) {
        if self.ime.active() {
            self.ime.unmark(&mut self.model);
            self.after_edit(cx);
        }
    }

    pub fn is_composing(&self) -> bool {
        self.ime.active()
    }

    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        self.finish_composition(cx);
        self.read_only = read_only;
        cx.notify();
    }

    pub fn set_font_size(&mut self, size: f32, cx: &mut Context<Self>) {
        self.font_size = size.clamp(12.0, 30.0);
        self.scroll_request = Some(ScrollRequest::RevealCursor);
        cx.notify();
    }

    pub fn set_writing_modes(&mut self, focus: bool, typewriter: bool, cx: &mut Context<Self>) {
        self.focus_mode = focus;
        self.typewriter = typewriter;
        self.scroll_request = Some(ScrollRequest::RevealCursor);
        cx.notify();
    }

    pub fn command(&mut self, command: crate::EditorCommand, cx: &mut Context<Self>) {
        use crate::EditorCommand;
        if self.read_only && !matches!(command, EditorCommand::SelectLine) {
            return;
        }
        self.finish_composition(cx);
        match command {
            EditorCommand::Wrap(marker) => self.toggle_wrap(marker, cx),
            EditorCommand::Link => self.insert_link(cx),
            EditorCommand::ToggleTask => {
                self.toggle_task(self.model.cursor().line, cx);
            }
            EditorCommand::TableEdit(command) => {
                if self
                    .table_context()
                    .is_some_and(|cell| cell.allows(command))
                {
                    self.edit(cx, |model| tables::edit(model, command));
                }
            }
            command => self.edit(cx, |model| crate::commands::apply(model, command)),
        }
    }

    pub fn match_count(&self, query: &str) -> usize {
        crate::commands::matches(&self.text(), query).len()
    }

    /// Search uses original buffer byte positions, including hidden Markdown syntax.
    pub fn find(
        &mut self,
        query: &str,
        backwards: bool,
        advance: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        self.finish_composition(cx);
        let ranges = crate::commands::matches(&self.text(), query);
        if ranges.is_empty() {
            return false;
        }
        let selection = self.model.selection();
        let pos = match selection {
            Some((start, end)) if advance => {
                if backwards {
                    start
                } else {
                    end
                }
            }
            Some((start, _)) => start,
            None => self.model.cursor(),
        };
        let byte = crate::commands::byte_position(&self.model, pos);
        let range = if backwards {
            ranges
                .iter()
                .rev()
                .find(|range| range.end <= byte)
                .unwrap_or(ranges.last().unwrap())
        } else {
            ranges
                .iter()
                .find(|range| range.start >= byte)
                .unwrap_or(&ranges[0])
        }
        .clone();
        self.edit(cx, |model| crate::commands::select_match(model, range));
        true
    }

    pub fn replace_match(&mut self, query: &str, value: &str, cx: &mut Context<Self>) -> bool {
        if self.read_only || query.is_empty() {
            return false;
        }
        self.finish_composition(cx);
        if self.model.selected_text().as_deref() != Some(query)
            && !self.find(query, false, false, cx)
        {
            return false;
        }
        self.edit(cx, |model| {
            if value.is_empty() {
                model.delete_selection();
            } else {
                model.insert(value);
            }
        });
        true
    }

    pub fn replace_all(&mut self, query: &str, value: &str, cx: &mut Context<Self>) -> usize {
        if self.read_only || query.is_empty() {
            return 0;
        }
        self.finish_composition(cx);
        let source = self.text();
        let count = crate::commands::matches(&source, query).len();
        if count > 0 {
            let replaced = source.replace(query, value);
            self.edit(cx, |model| {
                model.select_all();
                if replaced.is_empty() {
                    model.delete_selection();
                } else {
                    model.insert(&replaced);
                }
            });
        }
        count
    }

    /// Read access to the underlying [`EditorModel`] — cursor, selection,
    /// lines — for hosts building features over the buffer.
    pub fn model(&self) -> &EditorModel {
        &self.model
    }

    /// Mutate the [`EditorModel`] directly. Emits
    /// [`MarkdownEditorEvent::Change`] when the text changed, keeps the
    /// caret visible, and repaints.
    pub fn edit<R>(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut EditorModel) -> R) -> R {
        self.caret.reset();
        self.finish_composition(cx);
        let before = self.model.text();
        let result = f(&mut self.model);
        self.normalize_heading_cursor();
        let after = self.model.text();
        if after != before {
            cx.emit(MarkdownEditorEvent::Change(after));
        }
        self.scroll_request = Some(ScrollRequest::RevealCursor);
        cx.notify();
        result
    }

    /// Two-way bind this editor's text to a `Signal<String>`. The signal is
    /// the source of truth; equality guards on both directions prevent
    /// update loops.
    pub fn bind(entity: &Entity<MarkdownEditor>, signal: &Signal<String>, cx: &mut App) {
        let initial = signal.get(cx);
        entity.update(cx, |this, cx| {
            if this.text() != initial {
                this.set_text(&initial, cx);
            }
        });
        let sink = signal.clone();
        cx.subscribe(entity, move |_editor, event: &MarkdownEditorEvent, cx| {
            if let MarkdownEditorEvent::Change(text) = event {
                sink.set_if_changed(cx, text.clone());
            }
        })
        .detach();
        // Weak handle: a strong clone would form a retain cycle with the
        // subscription above and leak both the editor and the signal.
        let editor = entity.downgrade();
        cx.observe(signal.entity(), move |observed, cx| {
            let value = observed.read(cx).clone();
            editor
                .update(cx, |this, cx| {
                    if this.text() != value {
                        this.set_text(&value, cx);
                    }
                })
                .ok();
        })
        .detach();
    }

    // markdown editing commands

    /// Toggle a task in place without moving the reading viewport or selection.
    /// Returns whether the line was a writable task.
    pub fn toggle_task(&mut self, line: usize, cx: &mut Context<Self>) -> bool {
        if self.read_only {
            return false;
        }
        let Some(text) = self.model.line(line) else {
            return false;
        };
        let Block::Task { checked, state, .. } = classify_alone(text) else {
            return false;
        };
        let cursor = self.model.cursor();
        let anchor = self
            .model
            .selection()
            .map(|(start, end)| if cursor == start { end } else { start });
        self.edit(cx, |m| {
            // The marker prefix is ASCII, so `state` is also a char column.
            m.move_to(line, state, false);
            m.move_to(line, state + 1, true);
            m.insert(if checked { " " } else { "x" });
            if let Some(anchor) = anchor {
                m.move_to(anchor.line, anchor.col, false);
                m.move_to(cursor.line, cursor.col, true);
            } else {
                m.move_to(cursor.line, cursor.col, false);
            }
        });
        // The caret may be far outside the manually scrolled viewport. This
        // in-place action must not follow it as ordinary typing does.
        self.scroll_request = None;
        true
    }

    /// Wrap the selection (or the word at the cursor) in `marker`, or unwrap
    /// it when already wrapped. Single-line selections only.
    fn toggle_wrap(&mut self, marker: &str, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let chars = marker.chars().count();
        if self.model.selection().is_none() {
            self.model.select_word();
        }
        let Some((start, end)) = self.model.selection() else {
            // Empty spot: insert a pair and park the cursor inside it.
            self.edit(cx, |m| {
                m.insert(&format!("{marker}{marker}"));
                for _ in 0..chars {
                    m.move_left(false);
                }
            });
            return;
        };
        if start.line != end.line {
            return;
        }
        // Selection just inside an existing pair: widen to include it.
        let line = self.model.line(start.line).unwrap_or("");
        let (sb, eb) = (byte_for_col(line, start.col), byte_for_col(line, end.col));
        if line[..sb].ends_with(marker) && line[eb..].starts_with(marker) {
            self.model.move_to(start.line, start.col - chars, false);
            self.model.move_to(end.line, end.col + chars, true);
        }
        let Some(sel) = self.model.selected_text() else {
            return;
        };
        let unwrap =
            sel.starts_with(marker) && sel.ends_with(marker) && sel.len() >= 2 * marker.len();
        let replacement = if unwrap {
            sel[marker.len()..sel.len() - marker.len()].to_string()
        } else {
            format!("{marker}{sel}{marker}")
        };
        self.edit(cx, |m| m.insert(&replacement));
    }

    /// Wrap the selection as a markdown link and park the cursor where the
    /// missing half goes (url for a selection, text otherwise).
    fn insert_link(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        let single_line = matches!(self.model.selection(), Some((s, e)) if s.line == e.line);
        self.edit(cx, |m| {
            if single_line {
                let sel = m.selected_text().unwrap_or_default();
                m.insert(&format!("[{sel}]()"));
                m.move_left(false);
            } else {
                m.insert("[]()");
                for _ in 0..3 {
                    m.move_left(false);
                }
            }
        });
    }

    /// Enter: continue lists and quotes; an empty item exits the list.
    fn on_enter(&mut self, cx: &mut Context<Self>) {
        if !self.source_mode
            && self.model.selection().is_none()
            && let Some(changed) = tables::navigate(&mut self.model, tables::Navigation::NextRow)
        {
            if changed {
                self.after_edit(cx);
            } else {
                self.after_move(cx);
            }
            return;
        }
        let cursor = self.model.cursor();
        let line = self.model.line(cursor.line).unwrap_or("").to_string();
        let in_code = matches!(
            self.layout.get(cursor.line).map(|r| &r.plan.kind),
            Some(RowKind::Code { .. } | RowKind::Fence { .. } | RowKind::FrontMatter)
        );
        let marker = if in_code || self.model.selection().is_some() {
            None
        } else {
            continuation(&line)
        };
        match marker {
            Some(_) if line[prefix_end(&line)..].trim().is_empty() => {
                // Enter on an empty item clears the marker instead of
                // continuing the list.
                let cols = line.chars().count();
                self.edit(cx, |m| {
                    m.move_to(cursor.line, 0, false);
                    m.move_to(cursor.line, cols, true);
                    m.delete_selection();
                });
            }
            Some(marker) => self.edit(cx, |m| {
                m.newline();
                m.insert(&marker);
            }),
            None => self.edit(cx, |m| m.newline()),
        }
    }

    /// Tab / Shift+Tab: indent or outdent list items; plain Tab elsewhere.
    fn on_tab(&mut self, outdent: bool, cx: &mut Context<Self>) {
        if !self.source_mode
            && let Some(changed) = tables::navigate(
                &mut self.model,
                if outdent {
                    tables::Navigation::PreviousCell
                } else {
                    tables::Navigation::NextCell
                },
            )
        {
            if changed {
                self.after_edit(cx);
            } else {
                self.after_move(cx);
            }
            return;
        }
        let cursor = self.model.cursor();
        let line = self.model.line(cursor.line).unwrap_or("").to_string();
        let is_item = matches!(
            classify_alone(&line),
            Block::Bullet { .. } | Block::Ordered { .. } | Block::Task { .. }
        );
        if !is_item {
            if !outdent {
                self.edit(cx, |m| m.tab());
            }
            return;
        }
        let n = self.model.tab_size();
        if outdent {
            let lead = line.chars().take_while(|&c| c == ' ').count().min(n);
            if lead == 0 {
                return;
            }
            self.edit(cx, |m| {
                m.move_to(cursor.line, 0, false);
                m.move_to(cursor.line, lead, true);
                m.delete_selection();
                m.move_to(cursor.line, cursor.col.saturating_sub(lead), false);
            });
        } else {
            self.edit(cx, |m| {
                m.move_to(cursor.line, 0, false);
                m.insert(&" ".repeat(n));
                m.move_to(cursor.line, cursor.col + n, false);
            });
        }
    }

    /// Backspace at the start of an item's content removes the whole marker
    /// (list or quote) instead of eating into it invisibly.
    fn backspace_marker(&mut self, cx: &mut Context<Self>) -> bool {
        let cursor = self.model.cursor();
        let line = self.model.line(cursor.line).unwrap_or("").to_string();
        let content = match classify_alone(&line) {
            Block::Heading { content, .. }
            | Block::Bullet { content, .. }
            | Block::Ordered { content, .. }
            | Block::Task { content, .. }
            | Block::Quote { content, .. } => content,
            _ => return false,
        };
        // Marker prefixes are ASCII, so `content` is also a char column.
        if content == 0 || cursor.col != content {
            return false;
        }
        self.edit(cx, |m| {
            m.move_to(cursor.line, 0, false);
            m.move_to(cursor.line, content, true);
            m.delete_selection();
        });
        true
    }

    // input handling

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.caret.reset();
        if editmenu::is_open(&self.menu, cx) || self.table_menu_open(cx) {
            return;
        }
        // The OS owns candidate navigation, Enter and Escape during composition.
        if self.ime.active() {
            return;
        }
        let ks = &event.keystroke;
        let m = ks.modifiers;
        let shift = m.shift;
        if !matches!(ks.key.as_str(), "up" | "down") {
            self.goal_x = None;
        }
        match ks.key.as_str() {
            "left" => {
                if m.line() {
                    let cursor = self.model.cursor();
                    let start = if self.source_mode {
                        0
                    } else {
                        crate::commands::heading_start(&self.model, cursor.line)
                    };
                    self.model.move_to(cursor.line, start, shift);
                } else if m.word() {
                    self.model.word_left(shift);
                } else {
                    crate::unicode::move_grapheme(&mut self.model, false, shift);
                }
                self.after_move(cx);
            }
            "right" => {
                if m.line() {
                    self.model.end(shift);
                } else if m.word() {
                    self.model.word_right(shift);
                } else {
                    crate::unicode::move_grapheme(&mut self.model, true, shift);
                }
                self.after_move(cx);
            }
            "up" => {
                if m.line() {
                    self.model.doc_start(shift);
                    self.after_move(cx);
                } else {
                    self.move_visual(false, shift, cx);
                }
            }
            "down" => {
                if m.line() {
                    self.model.doc_end(shift);
                    self.after_move(cx);
                } else {
                    self.move_visual(true, shift, cx);
                }
            }
            "home" => {
                if m.cmd() {
                    self.model.doc_start(shift);
                } else {
                    let cursor = self.model.cursor();
                    let start = if self.source_mode {
                        0
                    } else {
                        crate::commands::heading_start(&self.model, cursor.line)
                    };
                    self.model.move_to(cursor.line, start, shift);
                }
                self.after_move(cx);
            }
            "end" => {
                if m.cmd() {
                    self.model.doc_end(shift);
                } else {
                    self.model.end(shift);
                }
                self.after_move(cx);
            }
            "backspace" => {
                if self.read_only {
                    return;
                }
                if self.model.selection().is_none()
                    && !self.source_mode
                    && !m.line()
                    && !m.word()
                    && self.backspace_marker(cx)
                {
                    cx.stop_propagation();
                    return;
                }
                let changed = if self.model.selection().is_some() {
                    self.model.delete_selection()
                } else if m.line() {
                    self.model.home(true);
                    self.model.delete_selection()
                } else if m.word() {
                    self.model.word_left(true);
                    self.model.delete_selection()
                } else {
                    crate::unicode::delete_grapheme(&mut self.model, false)
                };
                if changed {
                    self.after_edit(cx);
                } else {
                    cx.stop_propagation();
                }
            }
            "delete" => {
                if self.read_only {
                    return;
                }
                let changed = if self.model.selection().is_some() {
                    self.model.delete_selection()
                } else if m.line() {
                    self.model.end(true);
                    self.model.delete_selection()
                } else if m.word() {
                    self.model.word_right(true);
                    self.model.delete_selection()
                } else {
                    crate::unicode::delete_grapheme(&mut self.model, true)
                };
                if changed {
                    self.after_edit(cx);
                } else {
                    cx.stop_propagation();
                }
            }
            "enter" if m.cmd() => {
                if !self.read_only && self.toggle_task(self.model.cursor().line, cx) {
                    cx.stop_propagation();
                }
            }
            "enter" => {
                if self.read_only {
                    return;
                }
                self.on_enter(cx);
                cx.stop_propagation();
            }
            "tab" => {
                if m.cmd() || self.read_only {
                    return;
                }
                self.on_tab(shift, cx);
                cx.stop_propagation();
            }
            // Escape bubbles (dialogs close on it) but drops the selection.
            "escape" => {
                if self.model.selection().is_some() {
                    self.model.clear_selection();
                    cx.notify();
                }
            }
            "a" if m.cmd() => self.select_all(cx),
            "b" if m.cmd() => {
                self.toggle_wrap("**", cx);
                cx.stop_propagation();
            }
            "i" if m.cmd() => {
                self.toggle_wrap("*", cx);
                cx.stop_propagation();
            }
            "k" if m.cmd() => {
                self.insert_link(cx);
                cx.stop_propagation();
            }
            "c" if m.cmd() => self.copy(cx),
            "x" if m.cmd() => self.cut(cx),
            "v" if m.cmd() => self.paste(cx),
            "z" if m.cmd() => self.history(m.shift, cx),
            // Printable text arrives through EntityInputHandler, including ordinary
            // typing and IME commits. Handling key_char here would insert it twice.
            _ => {}
        }
        let _ = window;
    }

    // clipboard and history, shared by the keys and the Edit-menu actions

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.model.select_all();
        cx.notify();
        cx.stop_propagation();
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        self.finish_composition(cx);
        if let Some(text) = self.model.copy() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        cx.stop_propagation();
    }

    fn cut(&mut self, cx: &mut Context<Self>) {
        self.finish_composition(cx);
        if self.read_only {
            // Selection stays; degrade cut to copy.
            return self.copy(cx);
        }
        if let Some(text) = self.model.cut() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            return self.after_edit(cx);
        }
        cx.stop_propagation();
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        self.finish_composition(cx);
        if !self.read_only
            && let Some(text) = cx.read_from_clipboard().and_then(|item| item.text())
            && !text.is_empty()
        {
            let text = self.table_input(&text, None);
            self.model.insert(&text);
            return self.after_edit(cx);
        }
        cx.stop_propagation();
    }

    /// Literal pipe input in a visual cell must not create an extra column.
    fn table_input(&self, text: &str, range: Option<std::ops::Range<usize>>) -> String {
        if self.source_mode || !text.contains('|') || text.contains(['\n', '\r']) {
            return text.to_owned();
        }
        let range = range
            .or_else(|| self.ime.marked())
            .unwrap_or_else(|| ime::selection(&self.model));
        let start = ime::pos_utf16(&self.model, range.start);
        let end = ime::pos_utf16(&self.model, range.end);
        if start.line != end.line
            || self
                .layout
                .get(start.line)
                .is_none_or(|row| row.cells.is_empty())
        {
            return text.to_owned();
        }
        let source = self.model.line(start.line).unwrap_or("");
        let start = byte_for_col(source, start.col);
        let end = byte_for_col(source, end.col);
        if tables::cells(source)
            .iter()
            .any(|cell| cell.start <= start && end <= cell.end)
        {
            tables::escape_pipes(text, &source[..start])
        } else {
            text.to_owned()
        }
    }

    fn history(&mut self, redo: bool, cx: &mut Context<Self>) {
        self.finish_composition(cx);
        if !self.read_only {
            let changed = if redo {
                self.model.redo()
            } else {
                self.model.undo()
            };
            if changed {
                return self.after_edit(cx);
            }
        }
        cx.stop_propagation();
    }

    /// Right-click: with no selection the caret moves to the click first, so
    /// Paste lands where the user pointed; an existing selection is kept for
    /// the menu to act on.
    fn on_right_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.caret.reset();
        self.finish_composition(cx);
        window.focus(&self.focus);
        if self.model.selection().is_none() {
            let x = f32::from(ev.position.x) - f32::from(self.text_bounds.origin.x);
            let y = f32::from(ev.position.y) - f32::from(self.text_bounds.origin.y);
            let (line, col) = self.hit(x, y);
            self.model.move_to(line, col, false);
        }
        let menu = EditMenu::new(self.model.selection().is_some(), false, self.read_only);
        let mut slot = self.menu.take();
        editmenu::open(&mut slot, menu, &self.focus, ev.position, window, cx);
        self.menu = slot;
        cx.stop_propagation();
        cx.notify();
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.caret.reset();
        self.finish_composition(cx);
        window.focus(&self.focus);
        self.goal_x = None;
        let x = f32::from(ev.position.x) - f32::from(self.text_bounds.origin.x);
        let y = f32::from(ev.position.y) - f32::from(self.text_bounds.origin.y);
        let (line, col) = self.hit(x, y);

        if !self.read_only
            && let Some(row) = self.layout.get(line)
            && let Some(column) = row.cells.iter().position(|cell| x < cell.x + cell.width)
            && row.cells[column].missing
        {
            tables::focus_cell(&mut self.model, line, column);
            self.after_edit(cx);
            return;
        }

        // A click on a task's checkbox toggles it in place.
        if !self.read_only
            && ev.click_count == 1
            && let Some(row) = self.layout.get(line)
            && let RowKind::Task { .. } = row.plan.kind
        {
            let in_slot = x < row.inset && x >= 0.0;
            let in_first_row = y >= row.y.get() && y < row.y.get() + row.pad_top + row.line_h;
            if in_slot && in_first_row && self.toggle_task(line, cx) {
                return;
            }
        }
        match ev.click_count {
            2 => {
                self.model.move_to(line, col, false);
                self.model.select_word();
            }
            n if n > 2 => {
                self.model.move_to(line, col, false);
                self.model.select_line();
            }
            _ => self.model.move_to(line, col, ev.modifiers.shift),
        }
        cx.notify();
    }

    fn on_drag_move(
        &mut self,
        ev: &DragMoveEvent<MarkdownDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ev.drag(cx).0 != cx.entity_id() {
            return;
        }
        let x = f32::from(ev.event.position.x) - f32::from(self.text_bounds.origin.x);
        let y = f32::from(ev.event.position.y) - f32::from(self.text_bounds.origin.y);
        let (line, col) = self.hit(x, y);
        self.model.move_to(line, col, true);
        self.caret.reset();
        self.scroll_request = Some(ScrollRequest::RevealCursor);
        cx.notify();
    }

    /// Content-space position → (line, col), through the last frame's
    /// layout and each row's source↔visible mapping.
    fn hit(&self, x: f32, y: f32) -> (usize, usize) {
        if self.layout.is_empty() {
            return (0, 0);
        }
        let mut line = self.layout.len() - 1;
        for (i, row) in self.layout.iter().enumerate() {
            if y < row.y.get() + row.height {
                line = i;
                break;
            }
        }
        line = line.min(self.model.line_count().saturating_sub(1));
        let row = &self.layout[line];
        let Some(text) = self.model.line(line) else {
            return (line, 0);
        };
        let src = row.source_at(x, y - row.y.get());
        (line, col_for_byte(text, src))
    }

    /// Arrow up/down across *visual* rows, holding a sticky goal x through
    /// shorter rows — soft wrap means source-line movement would skip rows.
    fn move_visual(&mut self, down: bool, extend: bool, cx: &mut Context<Self>) {
        let cursor = self.model.cursor();
        if self.layout.len() != self.model.line_count() {
            // Stale layout (edit since last frame): plain line movement.
            if down {
                self.model.move_down(extend);
            } else {
                self.model.move_up(extend);
            }
            self.after_move(cx);
            return;
        }
        let row = &self.layout[cursor.line];
        let text = self.model.line(cursor.line).unwrap_or("");
        let vis = vis_for_src(&row.plan.segs, byte_for_col(text, cursor.col));
        let (x, vrow) = row.caret(vis);
        let goal = self.goal_x.unwrap_or(row.inset + x);
        self.goal_x = Some(goal);

        let target = if down {
            if vrow + 1
                < row
                    .cell_for_source(vis)
                    .map_or(row.visual_rows(), |cell| cell.row.visual_rows())
            {
                Some((cursor.line, vrow + 1))
            } else {
                (cursor.line + 1..self.layout.len())
                    .find(|&line| self.layout[line].height > 0.0)
                    .map(|line| (line, 0))
            }
        } else if vrow > 0 {
            Some((cursor.line, vrow - 1))
        } else {
            (0..cursor.line)
                .rev()
                .find(|&line| self.layout[line].height > 0.0)
                .map(|line| {
                    let row = &self.layout[line];
                    let count = row
                        .cells
                        .iter()
                        .find(|cell| goal < cell.x + cell.width)
                        .map_or(row.visual_rows(), |cell| cell.row.visual_rows());
                    (line, count - 1)
                })
        };
        let Some((tline, tvrow)) = target else {
            if down {
                self.model.doc_end(extend);
            } else {
                self.model.doc_start(extend);
            }
            self.after_move(cx);
            return;
        };
        let trow = &self.layout[tline];
        let src = trow.source_at(goal, trow.pad_top + (tvrow as f32 + 0.5) * trow.line_h);
        let col = col_for_byte(self.model.line(tline).unwrap_or(""), src);
        self.model.move_to(tline, col, extend);
        self.after_move(cx);
    }

    fn after_edit(&mut self, cx: &mut Context<Self>) {
        self.caret.reset();
        cx.emit(MarkdownEditorEvent::Change(self.model.text()));
        self.scroll_request = Some(ScrollRequest::RevealCursor);
        cx.notify();
        cx.stop_propagation();
    }

    fn after_move(&mut self, cx: &mut Context<Self>) {
        self.caret.reset();
        self.normalize_heading_cursor();
        self.scroll_request = Some(ScrollRequest::RevealCursor);
        cx.notify();
        cx.stop_propagation();
    }

    fn normalize_heading_cursor(&mut self) {
        if !self.source_mode && !self.ime.active() {
            crate::commands::clamp_heading_cursor(&mut self.model);
        }
    }

    fn cursor_vertical_bounds(&self) -> Option<(f32, f32)> {
        let cursor = self.model.cursor();
        let row = self.layout.get(cursor.line)?;
        let text = self.model.line(cursor.line).unwrap_or("");
        let vis = vis_for_src(&row.plan.segs, byte_for_col(text, cursor.col));
        let (_, vrow) = row.caret(vis);
        let top = row.y.get() + row.pad_top + vrow as f32 * row.line_h;
        Some((top, top + row.line_h + 2.0 * PAD_Y))
    }

    /// Nudge the scroll offset so the caret's visual row is inside the
    /// viewport. Runs during render, right after the fresh layout is built.
    fn ensure_cursor_visible(&mut self) {
        let Some((top, bottom)) = self.cursor_vertical_bounds() else {
            return;
        };
        let view_h = f32::from(self.scroll.bounds().size.height);
        if view_h <= 0.0 {
            return;
        }
        let offset = self.scroll.offset();
        let padding = if self.typewriter {
            view_h * 0.45
        } else {
            PAD_Y
        };
        let y = if self.typewriter {
            (view_h * 0.45 - top - padding).min(0.0)
        } else {
            scroll_adjust(f32::from(offset.y), view_h, top + padding, bottom + padding)
        };
        if y != f32::from(offset.y) {
            self.scroll.set_offset(point(offset.x, px(y)));
        }
    }

    fn align_cursor_line_at_top(&mut self) {
        let Some(row) = self.layout.get(self.model.cursor().line) else {
            return;
        };
        let view_h = f32::from(self.scroll.bounds().size.height);
        if view_h <= 0.0 {
            return;
        }
        let padding = if self.typewriter {
            view_h * 0.45
        } else {
            PAD_Y
        };
        // Cancel the document/typewriter padding for later chapters, retaining
        // the usual small reading inset. GPUI clamps to the fresh content size.
        let y = (PAD_Y - padding - row.y.get()).min(0.0);
        let offset = self.scroll.offset();
        self.scroll.set_offset(point(offset.x, px(y)));
    }
}

impl MarkdownEditor {
    fn render_document(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.work.document_renders += 1;
        if let Some(menu) = self.table_menu.clone()
            && menu.read(cx).is_open()
            && (self.source_mode || self.read_only || !menu.read(cx).valid(&self.model))
        {
            menu.update(cx, |menu, cx| menu.close(window, cx));
        }
        // A popup owns keyboard focus, but the original cell keeps its editing
        // layout and selection so opening the menu cannot reflow the document.
        let editor_focused = self.focus.is_focused(window);
        let focused = editor_focused || self.table_menu_open(cx);
        let caret_focused = editor_focused && window.is_window_active() && !self.read_only;
        if caret_focused && !self.caret.focused {
            self.caret.reset();
        }
        self.caret.focused = caret_focused;
        let base = self.font_size;

        let t = theme(cx);
        let style = self.style;
        let is_dark = t.scheme.is_dark();
        let frame_border = if focused {
            t.primary().hsla()
        } else {
            t.border().hsla()
        };
        let bg = style.bg.unwrap_or_else(|| t.surface().hsla());
        let text_color = style.text.unwrap_or_else(|| t.text().hsla());
        let dimmed = t.dimmed().hsla();
        let marker_color = t.dimmed().alpha(0.75);
        let comment_color = t
            .color(guise::theme::ColorName::Yellow, if is_dark { 4 } else { 7 })
            .hsla();
        let accent = style.accent.unwrap_or_else(|| t.primary().hsla());
        let caret_color = style.caret.unwrap_or(text_color);
        let selection_bg = style.selection.unwrap_or_else(|| t.primary().alpha(0.25));
        let code_bg = style
            .code_bg
            .unwrap_or_else(|| t.surface_hover().alpha(if is_dark { 0.45 } else { 0.6 }));
        let highlight_bg = t
            .color(guise::theme::ColorName::Yellow, if is_dark { 7 } else { 2 })
            .alpha(if is_dark { 0.45 } else { 0.7 });
        let placeholder_color = style.placeholder.unwrap_or_else(|| t.dimmed().hsla());
        let rule_color = t.border().hsla();
        let quote_bar = style.quote_bar.unwrap_or(rule_color);
        let quote_text = style.quote_text.unwrap_or(dimmed);
        let radius = t.radius(t.default_radius);
        let token_colors: [Hsla; 8] = TokenKind::ALL.map(|kind| token_color(kind, t));
        let table_stripe_bg = t.surface_hover().alpha(if is_dark { 0.55 } else { 0.65 });
        let table_edit_bg = t.primary().alpha(0.06);

        let prose = window.text_style().font();
        let mono = Font {
            family: MONO_FAMILY.into(),
            ..prose.clone()
        };
        let cell_w = {
            let ts = window.text_system();
            let font_id = ts.resolve_font(&prose);
            ts.ch_advance(font_id, px(base))
                .map(f32::from)
                .unwrap_or(base * 0.55)
        };
        self.cell_w = cell_w;
        let base_line_h = (base * 1.6).round();

        let cursor = self.model.cursor();
        let selection = self.model.selection();
        let reveal_range = match selection {
            _ if !focused || self.read_only => None,
            Some((s, e)) => Some((s.line, e.line)),
            None => Some((cursor.line, cursor.line)),
        };
        let marked = self.ime.marked().map(|range| {
            (
                ime::pos_utf16(&self.model, range.start),
                ime::pos_utf16(&self.model, range.end),
            )
        });
        let show_placeholder = self.model.is_empty() && !focused && !self.placeholder.is_empty();

        // build rows: classify, plan, shape
        let wrap_total = self.wrap_w.max(120.0);
        let previous = self.document_cache.current();
        let (parsed, changed) = self.document_cache.update(self.model.lines());
        if !window.is_window_active() {
            self.diagram_scrollbar_dragging.set(false);
        }
        let dragging = self.diagram_scrollbar_dragging.get();
        self.diagram_geometry_dirty |= self.diagram_was_dragging && !dragging;
        self.diagram_was_dragging = dragging;
        let scroll_offset = self.scroll.offset();
        let padding = if self.typewriter {
            f32::from(self.scroll.bounds().size.height) * 0.45
        } else {
            PAD_Y
        };
        let top = (-f32::from(scroll_offset.y) - padding).max(0.0);
        let diagram_anchor = (self.diagram_geometry_dirty
            && !changed
            && !self.source_mode
            && self.scroll_request.is_none())
        .then(|| {
            self.layout
                .iter()
                .enumerate()
                .find(|(_, row)| row.height > 0.0 && row.y.get() + row.height > top)
                .map(|(line, row)| (line, row.y.get()))
        })
        .flatten();
        self.diagram_geometry_dirty = false;
        if changed {
            self.work.parsed_documents += 1;
            for mode in &mut self.mode_layouts {
                mode.rows.rebase(
                    previous.as_ref().map_or(&[], |doc| doc.lines.as_slice()),
                    &parsed.lines,
                );
            }
            self.code_highlights.update(&parsed.code);
        }
        if changed || self.table_wrap != wrap_total {
            self.table_wrap = wrap_total;
            self.table_widths = parsed
                .tables
                .iter()
                .map(|table| table.widths(&parsed.lines, wrap_total))
                .collect();
            self.table_signatures = parsed
                .tables
                .iter()
                .zip(&self.table_widths)
                .map(|(table, widths)| {
                    let alignments: Vec<_> = table
                        .alignments
                        .iter()
                        .map(|alignment| match alignment {
                            Alignment::Left => 0u8,
                            Alignment::Center => 1,
                            Alignment::Right => 2,
                        })
                        .collect();
                    render_cache::fingerprint(&(
                        widths
                            .iter()
                            .map(|width| width.to_bits())
                            .collect::<Vec<_>>(),
                        alignments,
                    ))
                })
                .collect();
        }
        let tables = if self.source_mode {
            &parsed.tables[..0]
        } else {
            parsed.tables.as_slice()
        };
        let markdown_tables: Vec<_> = parsed
            .tables
            .iter()
            .map(|table| !editor_tables::inside_html_table(&parsed.html, table.start, table.end))
            .collect();
        let table_membership: Vec<_> = parsed
            .table_membership
            .iter()
            .map(|index| {
                if self.source_mode {
                    None
                } else {
                    index.filter(|&index| markdown_tables[index])
                }
            })
            .collect();
        let active_table = (focused && !self.read_only)
            .then(|| {
                table_membership
                    .get(cursor.line)
                    .copied()
                    .flatten()
                    .and_then(|index| tables[index].context(&self.model))
            })
            .flatten();
        let cell_style = CellStyle {
            base,
            font: prose.clone(),
            text: text_color,
            dimmed,
            accent,
            code_bg,
            highlight_bg,
            comment: comment_color,
        };
        let blocks = if self.source_mode {
            &parsed.code[..0]
        } else {
            parsed.code.as_slice()
        };
        let membership: Vec<_> = parsed
            .code_membership
            .iter()
            .map(|index| if self.source_mode { None } else { *index })
            .collect();
        let all_keys = parsed
            .code
            .iter()
            .map(|block| {
                block.is_mermaid().then(|| DiagramKey {
                    source: block.source.clone(),
                    dark: is_dark,
                })
            })
            .collect::<Vec<_>>();
        self.diagram_views.retain(|line, _| {
            parsed
                .code
                .iter()
                .any(|block| block.start == *line && block.is_mermaid())
        });
        for block in parsed.code.iter().filter(|block| block.is_mermaid()) {
            self.diagram_views.entry(block.start).or_default();
        }
        // Dropping obsolete pending tasks cancels their debounce/work; stale results
        // cannot replace a newer source or the other theme's preview.
        self.diagrams
            .retain(|key, _| all_keys.iter().flatten().any(|current| current == key));
        crate::diagram_cache::prune_idle(cx);
        for key in all_keys.iter().flatten() {
            if self.diagrams.contains_key(key) {
                continue;
            }
            if let Some(result) = crate::diagram_cache::layout(cx, key) {
                self.diagrams.insert(
                    key.clone(),
                    match result {
                        Ok(diagram) => DiagramState::Ready(diagram),
                        Err(error) => DiagramState::Error(error),
                    },
                );
                continue;
            }
            let key = key.clone();
            let cache_key = key.clone();
            let task_key = key.clone();
            let executor = cx.background_executor().clone();
            let capacity = crate::diagram_work::layout(cx);
            let ticket = crate::diagram_work::reserve_layout(cx);
            let request = crate::diagram_cache::request_layout(cx, &key);
            let task = cx.spawn(async move |this, cx| {
                executor.timer(std::time::Duration::from_millis(180)).await;
                let ticket = ticket.wait().await;
                let request_permit = request.acquire().await;
                // Another document may have filled the shared cache while this
                // request waited in FIFO order. Recheck before doing CPU work.
                let Ok(cached) =
                    this.update(cx, |_, cx| crate::diagram_cache::layout(cx, &task_key))
                else {
                    return;
                };
                // Waiting tasks use no worker thread. Keep the permit inside the
                // worker so cancellation cannot overlap a still-running layout.
                let (result, ticket, request_permit) = if let Some(result) = cached {
                    (result, ticket, request_permit)
                } else {
                    let permit = capacity.acquire_arc().await;
                    executor
                        .spawn(async move {
                            let _permit = permit;
                            (diagrams::render(&task_key), ticket, request_permit)
                        })
                        .await
                };
                // Make the memoized result available before waiting to display
                // it. Otherwise a later job holding the same key can deadlock
                // an earlier job that must publish first.
                let stored = this.update(cx, |_, cx| {
                    crate::diagram_cache::store_layout(cx, &key, &result)
                });
                drop(request_permit);
                if stored.is_err() {
                    return;
                }
                ticket.publish().await;
                let _ = this.update(cx, |this, cx| {
                    if let std::collections::hash_map::Entry::Occupied(mut entry) =
                        this.diagrams.entry(key.clone())
                    {
                        entry.insert(match result {
                            Ok(diagram) => DiagramState::Ready(diagram),
                            Err(error) => DiagramState::Error(error),
                        });
                        // Restore the reading row after correcting estimated
                        // heights; loading must never pull the view to the caret.
                        this.diagram_geometry_dirty = true;
                        cx.notify();
                    }
                });
            });
            self.diagrams
                .insert(cache_key, DiagramState::Loading { _task: task });
        }
        // Invalidate a replaced source/theme even if its row is currently offscreen.
        let mut preview_blocked = false;
        let mut preview_order = vec![false; all_keys.len()];
        for (index, key) in all_keys.iter().enumerate() {
            if let Some(key) = key {
                let diagram = match self.diagrams.get(key) {
                    Some(DiagramState::Ready(diagram)) => Some(diagram),
                    _ => None,
                };
                // Repeated SVG sources may be ready early. Their later rows
                // still wait behind preceding diagrams before requesting pixels.
                preview_blocked |=
                    matches!(self.diagrams.get(key), Some(DiagramState::Loading { .. }));
                preview_order[index] = !preview_blocked;
                let view = self
                    .diagram_views
                    .get_mut(&parsed.code[index].start)
                    .unwrap();
                view.reserve_space(
                    &key.source,
                    diagram,
                    matches!(self.diagrams.get(key), Some(DiagramState::Error(_))),
                    dragging,
                );
                view.retain_preview(diagram, cx);
                if let Some(diagram) = diagram.filter(|_| preview_order[index]) {
                    let (width, height) = diagram
                        .display_size(wrap_total - 32.0 - diagrams::PREVIEW_PADDING, view.zoom);
                    // Populate every diagram at document-open time; row elements remain virtualized.
                    view.preview(diagram, width, height, window.scale_factor(), cx);
                }
            }
        }
        let keys = if self.source_mode {
            &all_keys[..0]
        } else {
            all_keys.as_slice()
        };
        let editing_range = selection.map(|(s, e)| (s.line, e.line)).or(reveal_range);
        self.html_details
            .retain(|key, _| parsed.html.details.iter().any(|d| d.key == *key));
        let cursor_offset =
            parsed.html.offsets[cursor.line] + byte_for_col(&parsed.lines[cursor.line], cursor.col);
        let details_open: Vec<_> = parsed
            .html
            .details
            .iter()
            .map(|d| {
                self.html_details.get(&d.key).copied().unwrap_or(d.open)
                    || (!self.read_only && focused && d.body.contains(&cursor_offset))
                    || selection.is_some_and(|(start, end)| {
                        let start = parsed.html.offsets[start.line]
                            + byte_for_col(&parsed.lines[start.line], start.col);
                        let end = parsed.html.offsets[end.line]
                            + byte_for_col(&parsed.lines[end.line], end.col);
                        start < d.body.end && end > d.body.start
                    })
            })
            .collect();
        let mut html_hidden: Vec<_> = if self.source_mode {
            Vec::new()
        } else {
            parsed
                .html
                .details
                .iter()
                .zip(&details_open)
                .filter(|(_, open)| !**open)
                .map(|(d, _)| d.body.clone())
                .collect()
        };
        let html_table_expanded: Vec<_> = parsed
            .html
            .tables
            .iter()
            .map(|table| {
                self.source_mode
                    || editing_range.is_some_and(|(start, end)| {
                        start <= parsed.html.line(table.range.end.saturating_sub(1))
                            && end >= parsed.html.line(table.range.start)
                    })
            })
            .collect();
        let html_table_visible: Vec<_> = parsed
            .html
            .tables
            .iter()
            .zip(&html_table_expanded)
            .map(|(table, expanded)| {
                !expanded
                    && !html_hidden
                        .iter()
                        .any(|range| range.contains(&table.range.start))
            })
            .collect();
        for (table, visible) in parsed.html.tables.iter().zip(&html_table_visible) {
            if *visible {
                html_hidden.push(table.range.clone());
            }
        }
        let html_images_expanded: Vec<_> = parsed
            .html
            .image_groups
            .iter()
            .map(|group| {
                self.source_mode
                    || editing_range.is_some_and(|(start, end)| {
                        start <= parsed.html.line(group.range.end.saturating_sub(1))
                            && end >= parsed.html.line(group.range.start)
                    })
            })
            .collect();
        let html_images_visible: Vec<_> = parsed
            .html
            .image_groups
            .iter()
            .zip(&html_images_expanded)
            .map(|(group, expanded)| {
                !expanded
                    && !html_hidden
                        .iter()
                        .any(|range| range.contains(&group.range.start))
            })
            .collect();
        for (group, visible) in parsed.html.image_groups.iter().zip(&html_images_visible) {
            if *visible {
                html_hidden.push(group.range.clone());
            }
        }
        let html_visibility_signature = render_cache::fingerprint(&html_hidden);
        let expanded = blocks
            .iter()
            .enumerate()
            .map(|(index, block)| {
                block.touches(editing_range)
                    || keys[index].as_ref().is_some_and(|key| {
                        matches!(self.diagrams.get(key), Some(DiagramState::Error(_)))
                            && self.diagram_views[&block.start].show_error_source
                    })
            })
            .collect::<Vec<_>>();
        let preview_heights = keys
            .iter()
            .enumerate()
            .map(|(index, key)| match key {
                Some(_) => self.diagram_views[&blocks[index].start]
                    .reserved_height(wrap_total - 32.0 - diagrams::PREVIEW_PADDING),
                None => 0.0,
            })
            .collect::<Vec<_>>();
        self.sync_images(&parsed.images, cx);
        let image_flows = self.image_flows(
            &parsed.html,
            &parsed.images,
            &html_images_visible,
            wrap_total,
            dragging,
        );
        let mut image_heights = self.image_heights(&parsed.images, wrap_total, dragging);
        let html_image_signature = render_cache::fingerprint(
            &image_heights
                .iter()
                .flatten()
                .map(|h| h.to_bits())
                .collect::<Vec<_>>(),
        );
        let mut visible_images = parsed.images.clone();
        for (line, links) in visible_images.iter_mut().enumerate() {
            let keep: Vec<_> = links
                .iter()
                .enumerate()
                .filter(|(_, link)| {
                    !html_hidden
                        .iter()
                        .any(|range| range.contains(&link.source_offset))
                })
                .map(|(index, _)| index)
                .collect();
            image_heights[line] = keep
                .iter()
                .filter_map(|&index| image_heights[line].get(index).copied())
                .collect();
            *links = keep.iter().map(|&index| links[index].clone()).collect();
        }
        for (line, heights) in image_heights.iter_mut().enumerate() {
            if parsed
                .html
                .details
                .iter()
                .zip(&details_open)
                .any(|(d, open)| {
                    !open
                        && d.body.start <= parsed.html.offsets[line]
                        && d.body.end >= parsed.html.offsets[line] + parsed.lines[line].len()
                })
            {
                heights.clear();
            }
        }
        let table_widths = &self.table_widths;
        let mode = usize::from(self.source_mode);
        let layout_style = LayoutStyle {
            font: prose.clone(),
            base,
            wrap: wrap_total,
            compact: style.compact_headings,
            compact_blank_lines: style.compact_blank_lines,
            colors: [
                vec![
                    text_color,
                    dimmed,
                    marker_color,
                    accent,
                    code_bg,
                    highlight_bg,
                    quote_text,
                    comment_color,
                ],
                token_colors.to_vec(),
            ]
            .concat(),
        };
        if self.mode_layouts[mode].style.as_ref() != Some(&layout_style) {
            self.mode_layouts[mode].rows.clear();
            self.mode_layouts[mode].style = Some(layout_style);
        }
        let mut rows: Vec<Rc<Row>> = Vec::with_capacity(self.model.line_count());
        let mut y = 0.0;
        for (i, line) in self.model.lines().iter().enumerate() {
            let lang = if self.source_mode {
                Some("markdown")
            } else {
                parsed.languages[i].as_deref()
            };
            let html_editing =
                parsed
                    .html
                    .tables
                    .iter()
                    .zip(&html_table_expanded)
                    .any(|(t, expanded)| {
                        *expanded
                            && i >= parsed.html.line(t.range.start)
                            && i <= parsed.html.line(t.range.end.saturating_sub(1))
                    })
                    || parsed
                        .html
                        .image_groups
                        .iter()
                        .zip(&html_images_expanded)
                        .any(|(group, expanded)| {
                            *expanded
                                && i >= parsed.html.line(group.range.start)
                                && i <= parsed.html.line(group.range.end.saturating_sub(1))
                        });
            let block = if self.source_mode || html_editing {
                Block::CodeLine
            } else if matches!(parsed.blocks[i], Block::CodeLine)
                && (parsed.html.heading_at(parsed.html.offsets[i]).is_some()
                    || parsed.html.annotation_at(parsed.html.offsets[i]).is_some())
            {
                // Indentation inside HTML headings/comments belongs to that HTML.
                // Source mode still shows it verbatim.
                Block::Paragraph
            } else {
                parsed.blocks[i].clone()
            };
            let reveal = html_editing || reveal_range.is_some_and(|(s, e)| i >= s && i <= e);
            let code_index = membership[i];
            let code_block = code_index.map(|index| &blocks[index]);
            let table_index = table_membership[i];
            let active_column = table_index.filter(|_| reveal).map(|index| {
                let ranges = &tables[index].rows[i - tables[index].start];
                let cursor_byte = byte_for_col(line, cursor.col);
                ranges
                    .iter()
                    .position(|range| cursor_byte <= range.end)
                    .unwrap_or(ranges.len().saturating_sub(1))
            });
            let image_height: f32 = image_heights[i].iter().sum::<f32>()
                + image_flows[i].as_ref().map_or(0.0, |flow| flow.height);
            let hidden = (!self.source_mode
                && !reveal
                && parsed.images[i].iter().any(|image| image.standalone))
                || code_index.is_some_and(|index| {
                    !expanded[index]
                        && (blocks[index].is_mermaid() || matches!(block, Block::Fence { .. }))
                });
            // Keep explicit blank runs and document-edge typing space full-height.
            // Neighbor-dependent spacing belongs in the key: inserting a blank
            // can change the height of an otherwise unchanged cached row.
            let compact_blank = style.compact_blank_lines
                && matches!(block, Block::Blank)
                && i > 0
                && i + 1 < parsed.blocks.len()
                && !matches!(parsed.blocks[i - 1], Block::Blank)
                && !matches!(parsed.blocks[i + 1], Block::Blank);
            let key = RowKey {
                html: if self.source_mode {
                    0
                } else {
                    render_cache::fingerprint(&(
                        parsed.html.signatures[i],
                        html_visibility_signature,
                        if parsed
                            .html
                            .tables
                            .iter()
                            .any(|t| parsed.html.line(t.range.start) == i)
                        {
                            html_image_signature
                        } else {
                            0
                        },
                        &details_open,
                        html_editing,
                        parsed
                            .html
                            .tables
                            .iter()
                            .filter(|t| parsed.html.line(t.range.start) == i)
                            .map(|t| t.signature)
                            .collect::<Vec<_>>(),
                    ))
                },
                block: block.clone(),
                compact_blank,
                code: if self.source_mode {
                    parsed.source_signatures[i]
                } else {
                    code_index.map_or(0, |index| parsed.code_signatures[index])
                },
                table: table_index.map_or(0, |index| self.table_signatures[index]),
                reveal: reveal && !self.source_mode,
                active_column,
                selected: table_index.is_some() && reveal && selection.is_some(),
                hidden,
                code_edges: code_block.map_or(0, |block| {
                    u8::from(i == block.start) | (u8::from(i == block.end) << 1)
                }) | table_index.map_or(0, |index| {
                    (u8::from(i == tables[index].start) << 2)
                        | (u8::from(i == tables[index].start + 1) << 3)
                        | (u8::from(!self.read_only) << 4)
                }),
                preview: code_index.map_or(0, |index| preview_heights[index].to_bits()),
                image_preview: image_height.to_bits(),
            };
            if let Some(row) = self.mode_layouts[mode].rows.get(i, &key) {
                row.y.set(y);
                y += row.height;
                rows.push(row);
                self.work.cached_rows += 1;
                continue;
            }
            let (plan, extras) = parsed.html.plan(
                line,
                parsed.html.offsets[i],
                &block,
                lang,
                reveal && !matches!(block, Block::Heading { .. }),
                &html_hidden,
            );

            let m = metrics(&plan.kind);
            let scale = match plan.kind {
                RowKind::Heading(1) => 2.0,
                RowKind::Heading(2) => 1.6,
                RowKind::Heading(3) => 1.35,
                _ => m.scale,
            };
            let lh = m.line_height;
            let (pt, pb) = row_padding(&plan.kind, style.compact_headings);
            let size = (base * scale).round();
            let line_h = (size * lh).round();
            let line_h = if compact_blank {
                (line_h * 0.5).round()
            } else {
                line_h
            };
            let mut pad_top = (base * pt).round();
            let mut pad_bottom = (base * pb).round();

            if let Some(block) = code_block {
                if i == block.start {
                    pad_top += 40.0;
                }
                if i == block.end {
                    pad_bottom += 12.0;
                }
            }

            let disclosure = !self.source_mode
                && parsed.html.details.iter().any(|d| {
                    parsed.html.line(d.label) == i
                        && !html_hidden.iter().any(|r| r.contains(&d.range.start))
                });
            let inset = if disclosure && !reveal {
                22.0
            } else {
                match &plan.kind {
                    RowKind::Bullet { cols } | RowKind::Task { cols, .. } => {
                        *cols as f32 * cell_w + (base * 1.6).round()
                    }
                    RowKind::Ordered { cols, number } => {
                        let shaped = window.text_system().shape_line(
                            SharedString::from(format!("{number}.")),
                            px(size),
                            &[TextRun {
                                len: format!("{number}.").len(),
                                font: prose.clone(),
                                color: Hsla::default(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        );
                        *cols as f32 * cell_w + f32::from(shaped.width) + (base * 0.55).round()
                    }
                    RowKind::Quote { depth } => *depth as f32 * (base * 1.1).round(),
                    RowKind::Code { .. } | RowKind::Fence { .. } => (base * 0.75).round(),
                    _ => 0.0,
                }
            };
            let right_pad = match &plan.kind {
                RowKind::Code { .. } | RowKind::Fence { .. } => inset,
                _ => 0.0,
            };
            let wrap = (wrap_total - inset - right_pad).max(60.0);

            let runs = if let RowKind::Code { .. } = &plan.kind {
                let tokens = if self.source_mode {
                    parsed.source_tokens[i].as_slice()
                } else if let Some(block) = code_block {
                    self.code_highlights.line(block.start, i - block.start - 1)
                } else {
                    &[]
                };
                cover(plan.visible.len(), tokens)
                    .into_iter()
                    .map(|(len, kind)| TextRun {
                        len,
                        font: mono.clone(),
                        color: kind.map_or(text_color, |k| token_colors[k.index()]),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    })
                    .collect::<Vec<_>>()
            } else {
                let heading = matches!(plan.kind, RowKind::Heading(_));
                plan.runs
                    .iter()
                    .zip(&extras)
                    .map(|(run, extra)| {
                        let s = run.style;
                        let font = Font {
                            family: if s.code || extra.annotation.is_some() {
                                MONO_FAMILY.into()
                            } else {
                                prose.family.clone()
                            },
                            weight: if extra.annotation.is_none() && (heading || s.bold) {
                                FontWeight::BOLD
                            } else {
                                prose.weight
                            },
                            style: if s.italic {
                                FontStyle::Italic
                            } else {
                                prose.style
                            },
                            ..prose.clone()
                        };
                        let color = if let Some(annotation) = extra.annotation {
                            match annotation {
                                Annotation::Comment => comment_color,
                                Annotation::Break => marker_color,
                            }
                        } else if run.marker {
                            marker_color
                        } else if run.dim {
                            dimmed
                        } else if s.link {
                            accent
                        } else if matches!(plan.kind, RowKind::Quote { .. }) {
                            quote_text
                        } else {
                            extra
                                .color
                                .map(|c| gpui::rgb(c).into())
                                .unwrap_or(text_color)
                        };
                        TextRun {
                            len: run.len,
                            font,
                            color,
                            background_color: if s.highlight {
                                Some(highlight_bg)
                            } else if s.code {
                                Some(code_bg)
                            } else {
                                extra.background.map(|c| gpui::rgb(c).into())
                            },
                            underline: ((s.link || extra.underline) && !run.marker).then(|| {
                                UnderlineStyle {
                                    thickness: px(1.0),
                                    color: Some(accent),
                                    wavy: false,
                                }
                            }),
                            strikethrough: s.strike.then(|| StrikethroughStyle {
                                thickness: px(1.0),
                                color: Some(dimmed),
                            }),
                        }
                    })
                    .collect::<Vec<_>>()
            };

            // Grid rows use cell layout; hidden source never needs glyphs.
            let grid_row = table_index.is_some_and(|index| i != tables[index].start + 1);
            let hidden_delimiter =
                table_index.is_some_and(|index| i == tables[index].start + 1 && !reveal);
            let shaped = if hidden || grid_row || hidden_delimiter || plan.visible.is_empty() {
                None
            } else {
                self.work.shaped_rows += 1;
                ShapedText::new(&plan.visible, size, &runs, &extras, wrap, window).map(Rc::new)
            };
            let visual_rows = shaped.as_ref().map_or(1, |s| s.visual_rows()).max(1);
            let mut height = pad_top
                + if hidden {
                    0.0
                } else {
                    visual_rows as f32 * line_h
                }
                + pad_bottom;
            if let Some(index) = code_index {
                let block = &blocks[index];
                // A collapsed diagram occupies its opening row; all following
                // source rows are zero-height. Expanded diagrams follow the source.
                if block.is_mermaid() {
                    if !expanded[index] {
                        height = if i == block.start {
                            40.0 + preview_heights[index] + 12.0
                        } else {
                            0.0
                        };
                    } else if i == block.end {
                        height += preview_heights[index];
                    }
                }
            }
            let mut row = Row {
                html_table: None,
                plan,
                text: shaped,
                line_h,
                pad_top,
                inset,
                height,
                y: Cell::new(y),
                cells: Vec::new(),
                align: match extras.iter().find_map(|s| s.align) {
                    Some(1) => TextAlign::Center,
                    Some(2) => TextAlign::Right,
                    _ => TextAlign::Left,
                },
                wrap,
            };
            let affected = parsed.html.tags.iter().any(|t| {
                t.range.start <= parsed.html.offsets[i] + line.len()
                    && t.range.end > parsed.html.offsets[i]
            }) || parsed.html.comments.iter().any(|r| {
                r.start <= parsed.html.offsets[i] + line.len() && r.end > parsed.html.offsets[i]
            }) || html_hidden.iter().any(|r| {
                r.start <= parsed.html.offsets[i] && r.end >= parsed.html.offsets[i] + line.len()
            });
            if !self.source_mode && !reveal && affected && row.plan.visible.trim().is_empty() {
                row.height = 0.0;
                row.text = None;
            }
            if let Some(table) =
                parsed
                    .html
                    .tables
                    .iter()
                    .zip(&html_table_visible)
                    .find_map(|(table, visible)| {
                        (*visible && parsed.html.line(table.range.start) == i).then_some(table)
                    })
            {
                let layout = editor_html::shape_table(
                    table,
                    &parsed.html,
                    &parsed.lines,
                    wrap_total,
                    &cell_style,
                    &parsed.images,
                    &self.images.entries,
                    window,
                );
                self.work.shaped_cells += layout.cells.len() as u64;
                row.height += layout.height;
                row.html_table = Some(layout);
            }
            if let Some(index) = table_membership[i] {
                let table = &tables[index];
                // Keep the delimiter available when navigating its source line.
                // All data cells retain their own source mapping and native caret.
                if i == table.start + 1 {
                    if !reveal {
                        row.height = 0.0;
                        row.text = None;
                    }
                } else {
                    row.plan = guise::markdown::layout::plan(line, &Block::Table, None, true);
                    row.text = None;
                    row.inset = 0.0;
                    row.pad_top = 10.0;
                    let ranges = &table.rows[i - table.start];
                    let cursor_byte = byte_for_col(line, cursor.col);
                    let active_column = ranges
                        .iter()
                        .position(|range| cursor_byte <= range.end)
                        .unwrap_or(ranges.len().saturating_sub(1));
                    let mut x = 0.0;
                    for (column, &width) in table_widths[index].iter().enumerate() {
                        let source = ranges
                            .get(column)
                            .cloned()
                            .unwrap_or(line.len()..line.len());
                        let reveal_cell =
                            reveal && (selection.is_some() || column == active_column);
                        let cell = shape_table_cell(
                            &line[source.clone()],
                            i == table.start,
                            reveal_cell,
                            width,
                            table.alignments[column],
                            &cell_style,
                            window,
                        );
                        self.work.shaped_cells += 1;
                        row.plan
                            .links
                            .extend(cell.plan.links.iter().map(|(range, target)| {
                                (
                                    source.start + range.start..source.start + range.end,
                                    target.clone(),
                                )
                            }));
                        row.cells.push(TableCell {
                            source,
                            x,
                            width,
                            missing: column >= ranges.len(),
                            row: cell,
                        });
                        x += width;
                    }
                    row.line_h = row.cells[0].row.line_h;
                    row.height = row
                        .cells
                        .iter()
                        .map(|cell| cell.row.height)
                        .fold(0.0, f32::max)
                        + 20.0;
                    if i == table.start && !self.read_only {
                        row.pad_top += editor_tables::TOOLBAR_HEIGHT;
                        row.height += editor_tables::TOOLBAR_HEIGHT;
                    }
                }
            }
            row.height += image_height;
            if !self.source_mode
                && parsed
                    .html
                    .details
                    .iter()
                    .zip(&details_open)
                    .any(|(d, open)| {
                        !open
                            && d.body.start <= parsed.html.offsets[i]
                            && d.body.end >= parsed.html.offsets[i] + line.len()
                    })
            {
                row.height = 0.0;
                row.text = None;
            }
            y += row.height;
            rows.push(self.mode_layouts[mode].rows.insert(i, key, row));
        }
        self.layout = rows;
        if let Some((line, previous_y)) = diagram_anchor
            && let Some(row) = self.layout.get(line)
        {
            self.scroll.set_offset(point(
                scroll_offset.x,
                scroll_offset.y - px(row.y.get() - previous_y),
            ));
        }
        if let Some(request) = self.scroll_request.take() {
            match request {
                ScrollRequest::RevealCursor => self.ensure_cursor_visible(),
                ScrollRequest::LineStart => self.align_cursor_line_at_top(),
            }
        }

        // build elements
        let view_height = f32::from(self.scroll.bounds().size.height).max(1.0);
        let padding = if self.typewriter {
            view_height * 0.45
        } else {
            PAD_Y
        };
        let view_height = if view_height <= 1.0 {
            f32::from(window.viewport_size().height)
        } else {
            view_height
        };
        let visible = render_cache::visible_rows(
            self.layout.len(),
            |i| self.layout[i].height,
            (-f32::from(self.scroll.offset().y) - padding).max(0.0),
            view_height,
            view_height,
        );
        let mut row_divs: Vec<Div> = Vec::with_capacity(visible.range.len());
        for i in visible.range.clone() {
            let row = &self.layout[i];
            if row.height == 0.0 {
                continue;
            }
            self.work.built_rows += 1;
            let size_of_row = row
                .text
                .as_ref()
                .map(|t| f32::from(t.font_size()))
                .unwrap_or(base);
            let mut el = div().relative().w_full().h(px(row.height));
            if let Some(flow) = &image_flows[i] {
                el = el.child(self.image_flow_element(flow, row.height, rule_color, dimmed, cx));
            }
            if let Some(table) = &row.html_table {
                el = el.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(row.height
                            - table.height
                            - image_heights[i].iter().sum::<f32>()))
                        .w_full()
                        .child(self.html_table_element(table, i, rule_color, code_bg, cx)),
                );
            }
            if !self.source_mode {
                for (index, detail) in parsed.html.details.iter().enumerate() {
                    if parsed.html.line(detail.label) == i
                        && !html_hidden.iter().any(|r| r.contains(&detail.range.start))
                        && !row.plan.revealed
                    {
                        el = el.child(self.details_toggle(
                            index,
                            detail.key,
                            Pos::new(
                                i,
                                col_for_byte(
                                    &parsed.lines[i],
                                    detail.label - parsed.html.offsets[i],
                                ),
                            ),
                            details_open[index],
                            base,
                            dimmed,
                            cx,
                        ));
                    }
                }
            }
            if !image_heights[i].is_empty() {
                el = el.child(self.image_elements(
                    i,
                    &visible_images[i],
                    &image_heights[i],
                    wrap_total,
                    row.height,
                    rule_color,
                    dimmed,
                    cx,
                ));
            }
            if self.focus_mode
                && i != cursor.line
                && !selection.is_some_and(|(start, end)| i >= start.line && i <= end.line)
                && active_table.is_none_or(|cell| cell.start != i)
            {
                el = el.opacity(0.35);
            }
            if matches!(row.plan.kind, RowKind::Heading(1) | RowKind::Heading(2))
                && !self.source_mode
            {
                el = el.border_b_1().border_color(rule_color);
            }

            if let Some(index) = table_membership[i] {
                let table = &tables[index];
                let toolbar_height = if i == table.start && !self.read_only {
                    editor_tables::TOOLBAR_HEIGHT
                } else {
                    0.0
                };
                let mut grid = div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(toolbar_height))
                    .bottom_0()
                    .border_l_1()
                    .border_r_1()
                    .border_b_1()
                    .border_color(rule_color);
                if i == table.start {
                    grid = grid.border_t_1().bg(code_bg);
                } else if (i - table.start) % 2 == 1 {
                    grid = grid.bg(table_stripe_bg);
                }
                if i == cursor.line && focused && !self.read_only {
                    grid = grid.bg(table_edit_bg);
                }
                el = el.child(grid);
                if i == table.start
                    && let Some(cell) = active_table.filter(|cell| cell.start == i)
                {
                    el = el.child(self.table_toolbar(cell, cx));
                }
                for (column, cell) in row.cells.iter().enumerate() {
                    if column > 0 {
                        el = el.child(
                            div()
                                .absolute()
                                .left(px(cell.x))
                                .top(px(toolbar_height))
                                .bottom_0()
                                .w(px(1.0))
                                .bg(rule_color),
                        );
                    }
                    if let Some(shaped) = cell.row.text.clone() {
                        let line_h = px(row.line_h);
                        let align = cell.row.align;
                        el = el.child(
                            div()
                                .absolute()
                                .left(px(cell.x + cell.row.inset))
                                .top(px(row.pad_top))
                                .w(px(cell.row.wrap))
                                .h(px(cell.row.height))
                                .child(canvas(
                                    |_, _, _| (),
                                    move |bounds, _, window, cx| {
                                        shaped
                                            .paint_background(
                                                bounds.origin,
                                                line_h,
                                                align,
                                                None,
                                                window,
                                                cx,
                                            )
                                            .ok();
                                        shaped
                                            .paint(bounds.origin, line_h, align, None, window, cx)
                                            .ok();
                                    },
                                )),
                        );
                    }
                }
            }

            if let Some(index) = membership[i] {
                let block = &blocks[index];
                el = el
                    .bg(code_bg)
                    .border_l_1()
                    .border_r_1()
                    .border_color(rule_color);
                if i == block.start {
                    el = el.rounded_t(px(8.0)).border_t_1();
                    let label = if block.language.is_empty() {
                        "CODE".to_owned()
                    } else {
                        block.language.to_uppercase()
                    };
                    let source = block.source.clone();
                    let edit_line = (block.start + 1).min(block.end);
                    let mut diagram_controls = div().flex().items_center().gap(px(8.0));
                    if let Some(DiagramState::Ready(diagram)) =
                        keys[index].as_ref().and_then(|key| self.diagrams.get(key))
                    {
                        let zoom = self.diagram_views[&block.start].zoom;
                        let scale = diagram
                            .display_scale(wrap_total - 32.0 - diagrams::PREVIEW_PADDING, zoom);
                        for (id, label, next_zoom) in [
                            ("diagram-zoom-out", "−", Some(scale / 1.25)),
                            ("diagram-zoom-in", "+", Some(scale * 1.25)),
                            ("diagram-actual-size", "原始", Some(1.0)),
                            ("diagram-fit-width", "适宽", None),
                        ] {
                            diagram_controls = diagram_controls.child(
                                div()
                                    .id((id, i))
                                    .px(px(4.0))
                                    .cursor_pointer()
                                    .child(label)
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            if let Some(view) = this.diagram_views.get_mut(&i) {
                                                view.set_zoom(next_zoom);
                                                cx.notify();
                                            }
                                        }),
                                    ),
                            );
                        }
                        diagram_controls = diagram_controls.child(format!("{:.0}%", scale * 100.0));
                        let snapshot = diagram.clone();
                        diagram_controls = diagram_controls.child(
                            div()
                                .id(("diagram-view", i))
                                .debug_selector(|| "diagram-view".into())
                                .px(px(6.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .hover(|style| style.bg(code_bg))
                                .child("查看")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |_, _, _, cx| {
                                        cx.stop_propagation();
                                        cx.emit(MarkdownEditorEvent::ViewDiagram(snapshot.clone()));
                                    }),
                                ),
                        );
                    }
                    el = el.child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(36.0))
                            .px(px(14.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(rule_color)
                            .text_size(px(11.0))
                            .text_color(dimmed)
                            .child(label)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.0))
                                    .child(diagram_controls)
                                    .when(block.is_mermaid() && !self.read_only, |el| {
                                        el.child(
                                            div()
                                                .id(("edit-diagram", i))
                                                .cursor_pointer()
                                                .child("编辑")
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, _, window, cx| {
                                                        cx.stop_propagation();
                                                        this.finish_composition(cx);
                                                        this.model.move_to(edit_line, 0, false);
                                                        window.focus(&this.focus);
                                                        cx.notify();
                                                    }),
                                                ),
                                        )
                                    })
                                    .child(
                                        div()
                                            .id(("copy-code", i))
                                            .cursor_pointer()
                                            .child(if self.copied_block == Some(i) {
                                                "已复制"
                                            } else {
                                                "复制"
                                            })
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |this, _, _, cx| {
                                                    cx.stop_propagation();
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(source.clone()),
                                                    );
                                                    this.copied_block = Some(i);
                                                    cx.notify();
                                                    let executor = cx.background_executor().clone();
                                                    cx.spawn(async move |this, cx| {
                                                        executor
                                                            .timer(std::time::Duration::from_secs(
                                                                2,
                                                            ))
                                                            .await;
                                                        let _ = this.update(cx, |this, cx| {
                                                            if this.copied_block == Some(i) {
                                                                this.copied_block = None;
                                                                cx.notify();
                                                            }
                                                        });
                                                    })
                                                    .detach();
                                                }),
                                            ),
                                    ),
                            ),
                    );
                }
                if i == block.end || (block.is_mermaid() && !expanded[index]) {
                    el = el.rounded_b(px(8.0)).border_b_1();
                }
                if block.is_mermaid()
                    && ((!expanded[index] && i == block.start)
                        || (expanded[index] && i == block.end))
                {
                    let preview_height = preview_heights[index];
                    let top = row.height - preview_height - 12.0;
                    let edit_line = (block.start + 1).min(block.end);
                    let mut preview = div()
                        .absolute()
                        .left(px(15.0))
                        .right(px(15.0))
                        .top(px(top))
                        .h(px(preview_height))
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.0))
                        .text_color(dimmed)
                        .when(!self.read_only, |preview| preview.cursor_pointer())
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                if !this.read_only {
                                    this.finish_composition(cx);
                                    this.model.move_to(edit_line, 0, false);
                                    window.focus(&this.focus);
                                    cx.notify();
                                }
                            }),
                        );
                    match keys[index].as_ref().and_then(|key| self.diagrams.get(key)) {
                        Some(DiagramState::Ready(diagram)) if preview_order[index] => {
                            let view = self.diagram_views.get_mut(&block.start).unwrap();
                            let available = wrap_total - 32.0;
                            let (width, height) = diagram
                                .display_size(available - diagrams::PREVIEW_PADDING, view.zoom);
                            let viewport_height = preview_height;
                            let image_view =
                                view.preview(diagram, width, height, window.scale_factor(), cx);
                            preview = preview
                                .child(
                                    div()
                                        .id(("diagram-container", block.start))
                                        .relative()
                                        .w_full()
                                        .h(px(viewport_height))
                                        .flex_shrink_0()
                                        // Vertical scrolling belongs to the document;
                                        // horizontal gestures only move the wide diagram.
                                        .on_scroll_wheel(|event, _, cx| {
                                            let delta = event.delta.pixel_delta(px(1.0));
                                            if delta.x.abs() > delta.y.abs() {
                                                cx.stop_propagation();
                                            }
                                        })
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                        .child(
                                            div()
                                                .id(("diagram-viewport", block.start))
                                                .size_full()
                                                .overflow_x_scroll()
                                                .map(|mut el| {
                                                    el.style().restrict_scroll_to_axis = Some(true);
                                                    el
                                                })
                                                .track_scroll(&view.scroll)
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .w(px((width + diagrams::PREVIEW_PADDING)
                                                            .max(available)))
                                                        .h(px(height + diagrams::PREVIEW_PADDING))
                                                        .child(
                                                            div().child(image_view)
                                                                .flex_shrink_0()
                                                                .w(px(width))
                                                                .h(px(height))
                                                                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                                                                    cx.stop_propagation();
                                                                    if !this.read_only {
                                                                        this.finish_composition(cx);
                                                                        this.model.move_to(edit_line, 0, false);
                                                                        window.focus(&this.focus);
                                                                        cx.notify();
                                                                    }
                                                                })),
                                                        ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .left_0()
                                                .right_0()
                                                .bottom_0()
                                                .h(px(10.0))
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                                .child(guise::Scrollbar::new(("diagram-scroll-x", block.start), &view.scroll).horizontal(true)),
                                        ),
                                );
                        }
                        Some(DiagramState::Error(error)) => {
                            preview = preview
                                .items_start()
                                .child("流程图暂时无法预览，请检查源码")
                                .child(
                                    div()
                                        .mt(px(6.0))
                                        .max_h(px(50.0))
                                        .overflow_hidden()
                                        .child(error.clone()),
                                );
                        }
                        _ => preview = preview.child(diagrams::skeleton(rule_color, dimmed)),
                    }
                    el = el.child(preview);
                }
            }

            match &row.plan.kind {
                RowKind::Code { .. } => el = el.bg(code_bg),
                RowKind::Fence { open } => {
                    el = el.bg(code_bg);
                    el = if *open {
                        el.rounded_t(px(radius))
                    } else {
                        el.rounded_b(px(radius))
                    };
                }
                RowKind::Rule if !row.plan.revealed => {
                    el = el.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(px((row.height / 2.0 - 1.0).max(0.0)))
                            .h(px(2.0))
                            .rounded(px(1.0))
                            .bg(rule_color),
                    );
                }
                RowKind::Quote { depth } => {
                    let step = (base * 1.1).round();
                    for k in 0..*depth {
                        el = el.child(
                            div()
                                .absolute()
                                .left(px(k as f32 * step + 1.0))
                                .top_0()
                                .bottom_0()
                                .w(px(3.0))
                                .rounded(px(1.5))
                                .bg(quote_bar),
                        );
                    }
                }
                RowKind::Bullet { .. } => {
                    let dot = (base * 0.36).round().max(4.0);
                    el = el.child(
                        div()
                            .absolute()
                            .left(px(row.inset - dot - (base * 0.6).round()))
                            .top(px(row.pad_top + (row.line_h - dot) / 2.0))
                            .size(px(dot))
                            .rounded_full()
                            .bg(marker_color),
                    );
                }
                RowKind::Ordered { number, .. } => {
                    el = el.child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(row.pad_top))
                            .w(px((row.inset - (base * 0.35)).max(0.0)))
                            .h(px(row.line_h))
                            .flex()
                            .items_center()
                            .justify_end()
                            .text_size(px(size_of_row))
                            .text_color(marker_color)
                            .child(SharedString::from(format!("{number}."))),
                    );
                }
                RowKind::Task { checked, .. } => {
                    let box_s = (size_of_row * 1.05).round();
                    let boxed = div()
                        .absolute()
                        .left(px(row.inset - box_s - (base * 0.45).round()))
                        .top(px(row.pad_top + (row.line_h - box_s) / 2.0))
                        .size(px(box_s))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center();
                    el = el.child(if *checked {
                        boxed
                            .bg(accent)
                            .text_size(px(box_s * 0.8))
                            .text_color(gpui::white())
                            .child(Glyph::Lucide(IconName::Check))
                    } else {
                        boxed.border_1().border_color(dimmed)
                    });
                }
                _ => {}
            }

            // Selection rectangles.
            if let Some((start, end)) = selection
                && let Some(text) = self.model.line(i)
                && let Some((s_col, e_col, newline)) =
                    line_selection(start, end, i, text.chars().count())
            {
                let vs = vis_for_src(&row.plan.segs, byte_for_col(text, s_col));
                let ve = vis_for_src(&row.plan.segs, byte_for_col(text, e_col));
                for (x, vrow, w) in row.sel_rects(vs, ve, newline, cell_w) {
                    el = el.child(
                        div()
                            .absolute()
                            .left(px(row.inset + x))
                            .top(px(row.pad_top + vrow as f32 * row.line_h))
                            .w(px(w.max(2.0)))
                            .h(px(row.line_h))
                            .bg(selection_bg),
                    );
                }
            }

            // The shaped text itself, painted directly so caret/selection
            // math and the glyphs always agree.
            if let Some(shaped) = row.text.clone()
                && !row.plan.visible.is_empty()
            {
                let line_h = px(row.line_h);
                let align = row.align;
                let text_h = row.visual_rows() as f32 * row.line_h;
                el = el.child(
                    div()
                        .absolute()
                        .left(px(row.inset))
                        .top(px(row.pad_top))
                        .w(px((wrap_total - row.inset).max(60.0)))
                        .h(px(text_h))
                        .child(canvas(
                            |_, _, _| (),
                            move |bounds, _, window, cx| {
                                let origin = bounds.origin;
                                shaped
                                    .paint_background(origin, line_h, align, None, window, cx)
                                    .ok();
                                shaped.paint(origin, line_h, align, None, window, cx).ok();
                            },
                        )),
                );
            }

            // Underline provisional input without adding it to document history.
            if let Some((start, end)) = marked
                && let Some(text) = self.model.line(i)
                && let Some((s, e, newline)) = line_selection(start, end, i, text.chars().count())
            {
                let vs = vis_for_src(&row.plan.segs, byte_for_col(text, s));
                let ve = vis_for_src(&row.plan.segs, byte_for_col(text, e));
                for (x, visual_row, width) in row.sel_rects(vs, ve, newline, cell_w) {
                    el = el.child(
                        div()
                            .absolute()
                            .left(px(row.inset + x))
                            .top(px(row.pad_top + (visual_row + 1) as f32 * row.line_h - 1.0))
                            .w(px(width.max(1.0)))
                            .h(px(1.0))
                            .bg(accent),
                    );
                }
            }

            // Transparent interaction regions sit above the glyph canvas so
            // a hand cursor covers only the actual link text, including tables.
            let link_regions = if self.source_mode {
                Vec::new()
            } else {
                row.link_regions()
            };
            for (index, (bounds, target)) in link_regions.into_iter().enumerate() {
                el = el.child(
                    div()
                        .absolute()
                        .left(bounds.origin.x)
                        .top(bounds.origin.y)
                        .w(bounds.size.width)
                        .h(bounds.size.height)
                        .cursor_pointer()
                        .debug_selector(move || format!("markdown-link-{i}-{index}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                                if ev.modifiers.cmd() || this.read_only {
                                    this.caret.reset();
                                    this.finish_composition(cx);
                                    window.focus(&this.focus);
                                    cx.emit(MarkdownEditorEvent::LinkClick(target.clone()));
                                    cx.stop_propagation();
                                }
                            }),
                        ),
                );
            }
            if !self.read_only && matches!(row.plan.kind, RowKind::Task { .. }) {
                el = el.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .w(px(row.inset))
                        .h(px(row.pad_top + row.line_h))
                        .cursor_pointer()
                        .debug_selector(move || format!("markdown-task-{i}")),
                );
            }

            row_divs.push(el);
        }

        // Invisible bounds probe: its painted origin is content cell (0, 0)
        // and its width is next frame's wrap width.
        let entity = cx.entity();
        let input_entity = entity.clone();
        let input_focus = self.focus.clone();
        let editable = !self.read_only;
        let probe = canvas(
            move |bounds, window, cx| {
                entity.update(cx, |this, cx| {
                    this.text_bounds = bounds;
                    let caret_bounds = this.caret_bounds();
                    this.caret_layer.update(cx, |layer, _| {
                        layer.bounds = caret_bounds;
                    });
                    let w = f32::from(bounds.size.width);
                    if (w - this.wrap_w).abs() > 0.5 {
                        this.wrap_w = w;
                        // A cached view is laid out during prepaint. GPUI does
                        // not deliver observer notifications while drawing;
                        // invalidate after the frame so the document cache sees
                        // the newly measured width on the following frame.
                        cx.defer_in(window, |_, _, cx| cx.notify());
                    }
                });
            },
            move |bounds, _, window, cx| {
                if editable {
                    window.handle_input(
                        &input_focus,
                        ElementInputHandler::new(bounds, input_entity),
                        cx,
                    );
                }
            },
        )
        .absolute()
        .size_full();

        let mut lines_col = div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .child(probe)
            .when(visible.before > 0.0, |column| {
                column.child(div().h(px(visible.before)).flex_shrink_0())
            })
            .children(row_divs)
            .when(visible.after > 0.0, |column| {
                column.child(div().h(px(visible.after)).flex_shrink_0())
            });
        if show_placeholder {
            lines_col = lines_col.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .text_color(placeholder_color)
                    .child(self.placeholder.clone()),
            );
        }

        let content = div()
            .w_full()
            .py(px(padding))
            .px(px(PAD_X))
            .child(lines_col);

        let mut body = div()
            .id("guise-markdown-body")
            .key_context("TinyMdMarkdown")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_mouse_down))
            .children(editmenu::slot(&self.menu))
            .children(
                self.table_menu
                    .clone()
                    .map(|menu| div().absolute().child(menu)),
            )
            .on_drag(MarkdownDrag(cx.entity_id()), |_, _, _, cx| {
                cx.new(|_| Empty)
            })
            .on_drag_move(cx.listener(Self::on_drag_move))
            .on_action(cx.listener(|this, _: &actions::Copy, _, cx| this.copy(cx)))
            .on_action(cx.listener(|this, _: &actions::Cut, _, cx| this.cut(cx)))
            .on_action(cx.listener(|this, _: &actions::Paste, _, cx| this.paste(cx)))
            .on_action(cx.listener(|this, _: &actions::SelectAll, _, cx| this.select_all(cx)))
            .on_action(cx.listener(|this, _: &actions::Undo, _, cx| this.history(false, cx)))
            .on_action(cx.listener(|this, _: &actions::Redo, _, cx| this.history(true, cx)))
            .on_action(cx.listener(|this, _: &EditorTab, _, cx| {
                if this.read_only || this.ime.active() {
                    cx.propagate();
                    return;
                }
                this.on_tab(false, cx);
            }))
            .on_action(cx.listener(|this, _: &EditorTabPrevious, _, cx| {
                if this.read_only || this.ime.active() {
                    cx.propagate();
                    return;
                }
                this.on_tab(true, cx);
            }))
            .on_action(cx.listener(|this, _: &EditorTableMenu, window, cx| {
                let position = this.text_bounds.origin
                    + point(
                        px(this.wrap_w - PAD_X * 2.0),
                        px(this
                            .layout
                            .get(this.model.cursor().line)
                            .map_or(0.0, |row| row.y.get())),
                    );
                this.open_table_menu(position, window, cx);
            }))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .w_full()
            .h_full()
            .max_h_full()
            .cursor_text()
            .child(content);
        if let Some(rows) = self.rows {
            body = body.min_h(px(rows as f32 * base_line_h + 2.0 * PAD_Y));
        }

        self.caret_layer.update(cx, |layer, cx| {
            if layer.color != caret_color {
                layer.color = caret_color;
                cx.notify();
            }
        });
        let mut frame = div().flex().flex_col().w_full().h_full();
        if !style.bare {
            frame = frame
                .rounded(px(radius))
                .border_1()
                .border_color(frame_border);
        }
        frame
            .bg(bg)
            .overflow_hidden()
            .text_size(px(base))
            .line_height(px(base_line_h))
            .text_color(text_color)
            .child(body)
            .into_any_element()
    }

    fn caret_bounds(&self) -> Option<Bounds<Pixels>> {
        let cursor = self.model.cursor();
        let row = self.layout.get(cursor.line)?;
        let text = self.model.line(cursor.line)?;
        let vis = vis_for_src(&row.plan.segs, byte_for_col(text, cursor.col));
        let (x, vrow) = row.caret(vis);
        let border = if self.style.bare { 0.0 } else { 1.0 };
        // Use the measured content origin: prepaint may clamp the requested
        // scroll offset as a wrapped paragraph grows or the viewport changes.
        Some(Bounds::new(
            self.text_bounds.origin - self.scroll.bounds().origin
                + point(
                    px(row.inset + x + border),
                    px(row.y.get() + row.pad_top + vrow as f32 * row.line_h + border),
                ),
            gpui::size(px(1.0), px(row.line_h)),
        ))
    }

    fn caret_is_visible(&self, cx: &App) -> bool {
        self.caret_layer.read(cx).bounds.is_some_and(|bounds| {
            bounds.origin.y + bounds.size.height > px(0.0)
                && bounds.origin.y < self.scroll.bounds().size.height
        })
    }
}

impl Render for MarkdownEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        if self.document_focused != focused {
            self.document_focused = focused;
            self.caret.reset();
            self.document_layer.update(cx, |_, cx| cx.notify());
        }
        self.caret.focused = focused && window.is_window_active() && !self.read_only;
        let visible = self.caret.focused && (self.caret.visible || self.ime.active());
        self.caret_layer.update(cx, |layer, cx| {
            if layer.visible != visible {
                layer.visible = visible;
                cx.notify();
            }
        });
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(
                AnyView::from(self.document_layer.clone())
                    .cached(StyleRefinement::default().size_full()),
            )
            .child(self.caret_layer.clone())
    }
}

// pure helpers (unit-tested)

/// Classify a single line out of document context (fences and frontmatter
/// need the document pass; lists, quotes, and headings don't).
fn classify_alone(line: &str) -> Block {
    let mut state = DocState::default();
    classify("", &mut state);
    classify(line, &mut state)
}

/// The list/quote marker that continues `line` onto the next one, ready to
/// insert after the auto-copied indent — `None` when the line isn't a
/// continuable item.
fn continuation(line: &str) -> Option<String> {
    match classify_alone(line) {
        Block::Task { indent, .. } => {
            let ch = line.as_bytes()[indent] as char;
            Some(format!("{ch} [ ] "))
        }
        Block::Bullet { indent, .. } => {
            let ch = line.as_bytes()[indent] as char;
            Some(format!("{ch} "))
        }
        Block::Ordered { indent, number, .. } => {
            let delim = line[indent..]
                .bytes()
                .find(|b| *b == b'.' || *b == b')')
                .unwrap_or(b'.') as char;
            Some(format!("{}{delim} ", number + 1))
        }
        Block::Quote { depth, .. } => Some("> ".repeat(depth as usize)),
        _ => None,
    }
}

/// Where a continuable item's marker prefix ends (its content offset).
fn prefix_end(line: &str) -> usize {
    match classify_alone(line) {
        Block::Task { content, .. }
        | Block::Bullet { content, .. }
        | Block::Ordered { content, .. }
        | Block::Quote { content, .. } => content,
        _ => 0,
    }
}

/// The visual rows a visible byte range spans, given the wrap-boundary
/// bytes: (start row, end row). Range starts at a boundary belong to the
/// later row; range ends at a boundary belong to the earlier one.
fn split_visual(bounds: &[usize], vs: usize, ve: usize) -> (usize, usize) {
    let sr = bounds.iter().filter(|&&b| b <= vs).count();
    let er = bounds.iter().filter(|&&b| b < ve).count();
    (sr, er.max(sr))
}

/// Cover `len` bytes with contiguous span lengths: token ranges keep their
/// kind, gaps get `None`. Clamps overlap/overflow so lengths sum to `len`.
fn cover(
    len: usize,
    tokens: &[(std::ops::Range<usize>, TokenKind)],
) -> Vec<(usize, Option<TokenKind>)> {
    let mut out = Vec::new();
    let mut at = 0;
    for (range, kind) in tokens {
        let start = range.start.max(at).min(len);
        let end = range.end.max(start).min(len);
        if start > at {
            out.push((start - at, None));
        }
        if end > start {
            out.push((end - start, Some(*kind)));
        }
        at = end.max(at);
    }
    if at < len {
        out.push((len - at, None));
    }
    out
}

/// The selected char-column range on `line` for a normalized selection
/// `(start, end)`, or `None` when the selection misses the line. The `bool`
/// is whether the selection continues past this line's end.
fn line_selection(
    start: Pos,
    end: Pos,
    line: usize,
    line_len: usize,
) -> Option<(usize, usize, bool)> {
    if line < start.line || line > end.line {
        return None;
    }
    let s = if line == start.line {
        start.col.min(line_len)
    } else {
        0
    };
    let e = if line == end.line {
        end.col.min(line_len)
    } else {
        line_len
    };
    let e = e.max(s);
    let newline = line < end.line;
    if e == s && !newline {
        return None;
    }
    Some((s, e, newline))
}

/// Adjust a scroll offset (0 or negative, more negative = scrolled further)
/// so the content range `top..bottom` is inside a `view`-long viewport.
fn scroll_adjust(offset: f32, view: f32, top: f32, bottom: f32) -> f32 {
    let mut adjusted = offset;
    if bottom + adjusted > view {
        adjusted = view - bottom;
    }
    if top + adjusted < 0.0 {
        adjusted = -top;
    }
    adjusted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_headings_reduce_whitespace_and_preserve_other_block_spacing() {
        for level in 1..=6 {
            let kind = RowKind::Heading(level);
            let normal = row_padding(&kind, false);
            let compact = row_padding(&kind, true);
            assert!(compact.0 + compact.1 < (normal.0 + normal.1) * 0.75);
        }
        for kind in [
            RowKind::Blank,
            RowKind::Paragraph,
            RowKind::Table,
            RowKind::FrontMatter,
        ] {
            assert_eq!(row_padding(&kind, false), row_padding(&kind, true));
        }
    }

    #[test]
    fn continuation_markers() {
        assert_eq!(continuation("- item"), Some("- ".into()));
        assert_eq!(continuation("* item"), Some("* ".into()));
        assert_eq!(continuation("  - item"), Some("- ".into()));
        assert_eq!(continuation("- [x] done"), Some("- [ ] ".into()));
        assert_eq!(continuation("3. third"), Some("4. ".into()));
        assert_eq!(continuation("3) third"), Some("4) ".into()));
        assert_eq!(continuation("> quoted"), Some("> ".into()));
        assert_eq!(continuation("> > deep"), Some("> > ".into()));
        assert_eq!(continuation("plain"), None);
        assert_eq!(continuation("# heading"), None);
    }

    #[test]
    fn prefix_end_finds_content() {
        assert_eq!(prefix_end("- item"), 2);
        assert_eq!(prefix_end("  - [ ] x"), 8);
        assert_eq!(prefix_end("> q"), 2);
        assert_eq!(prefix_end("plain"), 0);
    }

    #[test]
    fn split_visual_rows() {
        // No wrapping: everything is row 0.
        assert_eq!(split_visual(&[], 0, 10), (0, 0));
        // One boundary at 10: [0..10) is row 0, [10..) is row 1.
        assert_eq!(split_visual(&[10], 2, 8), (0, 0));
        assert_eq!(split_visual(&[10], 2, 15), (0, 1));
        assert_eq!(split_visual(&[10], 12, 15), (1, 1));
        // Starts at the boundary → later row; ends at it → earlier row.
        assert_eq!(split_visual(&[10], 10, 15), (1, 1));
        assert_eq!(split_visual(&[10], 2, 10), (0, 0));
        // Degenerate empty range never inverts.
        assert_eq!(split_visual(&[10], 10, 10), (1, 1));
    }

    #[test]
    fn cover_spans_exactly() {
        let tokens = vec![(2..5, TokenKind::Keyword)];
        let s = cover(10, &tokens);
        let total: usize = s.iter().map(|(len, _)| len).sum();
        assert_eq!(total, 10);
        assert_eq!(s[1], (3, Some(TokenKind::Keyword)));
        assert_eq!(cover(4, &[]), vec![(4, None)]);
    }

    #[test]
    fn scroll_adjust_reveals_target() {
        assert_eq!(scroll_adjust(-10.0, 100.0, 20.0, 40.0), -10.0);
        assert_eq!(scroll_adjust(-50.0, 100.0, 20.0, 40.0), -20.0);
        assert_eq!(scroll_adjust(0.0, 100.0, 150.0, 170.0), -70.0);
    }

    fn at(line: usize, col: usize) -> Pos {
        Pos::new(line, col)
    }

    #[test]
    fn line_selection_matches_editor_semantics() {
        assert_eq!(
            line_selection(at(1, 2), at(1, 5), 1, 8),
            Some((2, 5, false))
        );
        assert_eq!(line_selection(at(0, 3), at(2, 2), 1, 4), Some((0, 4, true)));
        assert_eq!(line_selection(at(0, 3), at(2, 2), 3, 4), None);
        assert_eq!(line_selection(at(0, 0), at(2, 1), 1, 0), Some((0, 0, true)));
    }
}
