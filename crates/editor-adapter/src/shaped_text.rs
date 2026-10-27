//! One source row can contain several hard line breaks (HTML br or table cells).
//! Keep all shaped lines and use the same geometry for painting and hit testing.
use crate::html::Style;
use gpui::{
    App, Bounds, Pixels, Point, ShapedLine, SharedString, TextAlign, TextRun, Window, WrappedLine,
    point, px,
};

pub(crate) struct ShapedText {
    lines: Vec<WrappedLine>,
    offsets: Vec<usize>,
    rows: Vec<usize>,
    boundaries: Vec<usize>,
    len: usize,
    overlays: Vec<(usize, i8, ShapedLine)>,
}

impl ShapedText {
    pub fn new(
        text: &str,
        size: f32,
        runs: &[TextRun],
        extras: &[Style],
        wrap: f32,
        window: &Window,
    ) -> Option<Self> {
        let mut plain = runs.to_vec();
        for (run, extra) in plain.iter_mut().zip(extras) {
            if extra.script != 0 {
                run.color.a = 0.0;
            }
        }
        let lines: Vec<_> = window
            .text_system()
            .shape_text(
                SharedString::from(text.to_owned()),
                px(size),
                &plain,
                Some(px(wrap)),
                None,
            )
            .ok()?
            .into_iter()
            .collect();
        if lines.is_empty() {
            return None;
        }
        let mut offsets = Vec::new();
        let mut rows = Vec::new();
        let mut boundaries = Vec::new();
        let mut offset = 0;
        let mut row = 0;
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                boundaries.push(offset);
            }
            offsets.push(offset);
            rows.push(row);
            boundaries.extend(
                line.wrap_boundaries()
                    .iter()
                    .map(|b| offset + line.runs()[b.run_ix].glyphs[b.glyph_ix].index),
            );
            offset += line.len() + 1;
            row += line.wrap_boundaries().len() + 1;
        }
        let mut overlays = Vec::new();
        let mut at = 0;
        for (run, extra) in runs.iter().zip(extras) {
            if extra.script != 0 {
                use unicode_segmentation::UnicodeSegmentation;
                for (index, glyph) in text[at..at + run.len].grapheme_indices(true) {
                    if glyph == "\n" {
                        continue;
                    }
                    let mut styled = run.clone();
                    styled.len = glyph.len();
                    let shaped = window.text_system().shape_line(
                        glyph.to_owned().into(),
                        px(size * 0.75),
                        &[styled],
                        None,
                    );
                    overlays.push((at + index, extra.script, shaped));
                }
            }
            at += run.len;
        }
        Some(Self {
            lines,
            offsets,
            rows,
            boundaries,
            len: text.len(),
            overlays,
        })
    }

    pub fn boundaries(&self) -> Vec<usize> {
        self.boundaries.clone()
    }
    pub fn visual_rows(&self) -> usize {
        self.boundaries.len() + 1
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn font_size(&self) -> Pixels {
        self.lines[0].font_size()
    }

    pub fn visual_width(&self, row: usize) -> f32 {
        let i = self
            .rows
            .partition_point(|&start| start <= row)
            .saturating_sub(1);
        let line = &self.lines[i];
        let local_row = row.saturating_sub(self.rows[i]);
        let end = line
            .wrap_boundaries()
            .get(local_row)
            .map(|b| line.runs()[b.run_ix].glyphs[b.glyph_ix].index)
            .unwrap_or(line.len());
        line.position_for_index(end, px(1.0))
            .map_or(0.0, |p| f32::from(p.x))
    }

    pub fn position_for_index(&self, index: usize, line_h: Pixels) -> Option<Point<Pixels>> {
        let index = index.min(self.len);
        let i = self
            .offsets
            .partition_point(|&offset| offset < index)
            .saturating_sub(1);
        let local = index
            .saturating_sub(self.offsets[i])
            .min(self.lines[i].len());
        let mut p = self.lines[i].position_for_index(local, line_h)?;
        p.y += line_h * self.rows[i] as f32;
        Some(p)
    }

    pub fn closest_index_for_position(
        &self,
        p: Point<Pixels>,
        line_h: Pixels,
    ) -> Result<usize, usize> {
        let row = (f32::from(p.y).max(0.0) / f32::from(line_h).max(1.0)) as usize;
        let i = self
            .rows
            .partition_point(|&start| start <= row)
            .saturating_sub(1);
        let local = point(p.x, p.y - line_h * self.rows[i] as f32);
        self.lines[i]
            .closest_index_for_position(local, line_h)
            .map(|n| n + self.offsets[i])
            .map_err(|n| n + self.offsets[i])
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint_background(
        &self,
        origin: Point<Pixels>,
        line_h: Pixels,
        align: TextAlign,
        bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Result<()> {
        for (i, line) in self.lines.iter().enumerate() {
            line.paint_background(
                origin + point(px(0.0), line_h * self.rows[i] as f32),
                line_h,
                align,
                bounds,
                window,
                cx,
            )?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &self,
        origin: Point<Pixels>,
        line_h: Pixels,
        align: TextAlign,
        bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut App,
    ) -> gpui::Result<()> {
        for (i, line) in self.lines.iter().enumerate() {
            line.paint(
                origin + point(px(0.0), line_h * self.rows[i] as f32),
                line_h,
                align,
                bounds,
                window,
                cx,
            )?;
        }
        for (index, direction, shaped) in &self.overlays {
            let i = self
                .offsets
                .partition_point(|&offset| offset <= *index)
                .saturating_sub(1);
            let line = &self.lines[i];
            let local = index - self.offsets[i];
            let mut p = line.position_for_index(local, line_h).unwrap_or_default();
            let boundaries: Vec<_> = line
                .wrap_boundaries()
                .iter()
                .map(|b| line.runs()[b.run_ix].glyphs[b.glyph_ix].index)
                .collect();
            let row = boundaries.partition_point(|&b| b <= local);
            if boundaries.contains(&local) {
                p = point(px(0.0), line_h * row as f32);
            }
            let end = boundaries.get(row).copied().unwrap_or(line.len());
            let right = line
                .position_for_index(end, line_h)
                .map_or(px(0.0), |p| p.x);
            let width = bounds
                .map(|b| b.size.width)
                .or(line.wrap_width)
                .unwrap_or(right);
            let factor = match align {
                TextAlign::Center => 0.5,
                TextAlign::Right => 1.0,
                _ => 0.0,
            };
            p.x += (width - right).max(px(0.0)) * factor;
            p.y += line_h * (self.rows[i] as f32 + if *direction < 0 { -0.16 } else { 0.16 });
            shaped.paint(origin + p, line_h, window, cx)?;
        }
        Ok(())
    }
}
