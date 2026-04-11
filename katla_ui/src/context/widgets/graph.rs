//! Internal graph widget implementation.
//!
//! This module contains rendering logic for real-time data visualization graphs.
//! This is a private implementation detail.

use katla_math::{Rect2D, Vec2};

use super::super::{GraphOptions, UiContext};

impl UiContext {
    pub fn graph(
        &mut self,
        label: Option<&str>,
        values: &[f32],
        bounds: Rect2D,
        options: Option<GraphOptions>,
    ) {
        let opts = options.unwrap_or_default();

        // Handle empty values case
        if values.is_empty() {
            self.draw_rect(bounds, opts.bg_color);
            if let Some(label_text) = label {
                let text_pos = Vec2::new(bounds.min.x() + 5.0, bounds.min.y() + 5.0);
                self.draw_text(
                    label_text,
                    text_pos,
                    self.style.text_color,
                    self.style.font_size,
                );
            }
            return;
        }

        // Calculate min/max
        let min_val = opts
            .min_value
            .unwrap_or_else(|| values.iter().cloned().fold(f32::INFINITY, f32::min));
        let max_val = opts
            .max_value
            .unwrap_or_else(|| values.iter().cloned().fold(f32::NEG_INFINITY, f32::max));

        // Ensure we have a valid range
        let range = if (max_val - min_val).abs() < 0.001 {
            1.0 // Avoid division by zero
        } else {
            max_val - min_val
        };

        // Layout: label area at top, graph area below
        let label_height = if label.is_some() {
            self.style.graph_label_height
        } else {
            0.0
        };
        let padding = self.style.graph_padding;

        let graph_bounds = Rect2D::new(
            Vec2::new(
                bounds.min.x() + padding,
                bounds.min.y() + label_height + padding,
            ),
            Vec2::new(bounds.max.x() - padding, bounds.max.y() - padding),
        );

        // 1. Draw background
        self.draw_rect(bounds, opts.bg_color);

        // 2. Draw label if provided
        if let Some(label_text) = label {
            let text_pos = Vec2::new(bounds.min.x() + 5.0, bounds.min.y() + 2.0);
            self.draw_text(
                label_text,
                text_pos,
                self.style.text_color,
                self.style.font_size,
            );
        }

        // 3. Draw grid lines (horizontal)
        if let Some(grid_color) = opts.grid_color
            && graph_bounds.height() > 0.0
            && opts.grid_lines > 0
        {
            for i in 1..opts.grid_lines {
                let t = i as f32 / opts.grid_lines as f32;
                let y = graph_bounds.max.y() - t * graph_bounds.height();
                self.draw_line(
                    Vec2::new(graph_bounds.min.x(), y),
                    Vec2::new(graph_bounds.max.x(), y),
                    grid_color,
                    1.0,
                );
            }
        }

        // Skip drawing if graph area is too small
        if graph_bounds.width() < 2.0 || graph_bounds.height() < 2.0 || values.len() < 2 {
            return;
        }

        // 4. Convert values to screen coordinates (reusing scratch buffer)
        self.scratch_points.clear();
        self.scratch_points.reserve(values.len());
        for (i, &v) in values.iter().enumerate() {
            let t = if values.len() > 1 {
                i as f32 / (values.len() - 1) as f32
            } else {
                0.5
            };
            let x = graph_bounds.min.x() + t * graph_bounds.width();
            let normalized = ((v - min_val) / range).clamp(0.0, 1.0);
            let y = graph_bounds.max.y() - normalized * graph_bounds.height();
            self.scratch_points.push(Vec2::new(x, y));
        }

        // 5. Draw filled area under the line (as vertical strips)
        if let Some(fill_color) = opts.fill_color {
            let bottom_y = graph_bounds.max.y();

            self.push_clip(graph_bounds);

            for i in 0..self.scratch_points.len().saturating_sub(1) {
                let p0 = self.scratch_points[i];
                let p1 = self.scratch_points[i + 1];

                self.draw_list.add_convex_poly(
                    &[
                        Vec2::new(p0.x(), p0.y()),
                        Vec2::new(p1.x(), p1.y()),
                        Vec2::new(p1.x(), bottom_y),
                        Vec2::new(p0.x(), bottom_y),
                    ],
                    fill_color,
                );
            }

            self.pop_clip();
        }

        // 6. Draw the line segments
        self.push_clip(graph_bounds);
        for i in 0..self.scratch_points.len().saturating_sub(1) {
            self.draw_line(
                self.scratch_points[i],
                self.scratch_points[i + 1],
                opts.line_color,
                opts.line_thickness,
            );
        }
        self.pop_clip();

        // 7. Draw current value text
        if opts.show_value
            && let Some(&last_val) = values.last()
        {
            let value_text = format!("{:.1}", last_val);
            let text_size = self.measure_text(&value_text, self.style.font_size);
            let text_pos = Vec2::new(
                graph_bounds.max.x() - text_size.x() - 5.0,
                graph_bounds.min.y() + 2.0,
            );
            self.draw_text(&value_text, text_pos, opts.line_color, self.style.font_size);
        }
    }
}
