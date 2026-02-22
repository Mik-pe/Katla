//! Graph widget for real-time data visualization.

use katla_math::{Rect2D, Vec2};

use super::super::{GraphOptions, UiContext};

impl UiContext {
    /// Draw a real-time line graph.
    ///
    /// Values should be ordered oldest to newest (left to right).
    /// The graph will auto-scale if min/max not provided in options.
    pub fn graph(
        &mut self,
        id: &str,
        label: Option<&str>,
        values: &[f32],
        bounds: Rect2D,
        options: Option<GraphOptions>,
    ) {
        let _ = id; // ID reserved for future interactivity
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
        let label_height = if label.is_some() { 18.0 } else { 0.0 };
        let padding = 3.0;

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
        if let Some(grid_color) = opts.grid_color {
            if graph_bounds.height() > 0.0 && opts.grid_lines > 0 {
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
        }

        // Skip drawing if graph area is too small
        if graph_bounds.width() < 2.0 || graph_bounds.height() < 2.0 || values.len() < 2 {
            return;
        }

        // 4. Convert values to screen coordinates
        let points: Vec<Vec2> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let t = if values.len() > 1 {
                    i as f32 / (values.len() - 1) as f32
                } else {
                    0.5
                };
                let x = graph_bounds.min.x() + t * graph_bounds.width();
                let normalized = ((v - min_val) / range).clamp(0.0, 1.0);
                let y = graph_bounds.max.y() - normalized * graph_bounds.height();
                Vec2::new(x, y)
            })
            .collect();

        // 5. Draw filled area under the line (as vertical strips)
        if let Some(fill_color) = opts.fill_color {
            let bottom_y = graph_bounds.max.y();

            self.push_clip(graph_bounds);

            // Draw vertical quads between each pair of adjacent points
            for i in 0..points.len().saturating_sub(1) {
                let p0 = points[i];
                let p1 = points[i + 1];

                // Create a quad: top-left, top-right, bottom-right, bottom-left
                self.draw_list.add_convex_poly(
                    &[
                        Vec2::new(p0.x(), p0.y()),   // top-left
                        Vec2::new(p1.x(), p1.y()),   // top-right
                        Vec2::new(p1.x(), bottom_y), // bottom-right
                        Vec2::new(p0.x(), bottom_y), // bottom-left
                    ],
                    fill_color,
                );
            }

            self.pop_clip();
        }

        // 6. Draw the line segments
        self.push_clip(graph_bounds);
        for i in 0..points.len().saturating_sub(1) {
            self.draw_line(
                points[i],
                points[i + 1],
                opts.line_color,
                opts.line_thickness,
            );
        }
        self.pop_clip();

        // 7. Draw current value text
        if opts.show_value {
            if let Some(&last_val) = values.last() {
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
}
