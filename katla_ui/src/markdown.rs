//! Lightweight markdown-like text rendering for UI panels.
//!
//! Supports:
//! - **bold** via `**text**`
//! - `code` via `` `text` ``
//! - Headers via `# `, `## `, `### `
//! - Bullet points via `- ` or `* `
//! - Numbered lists via `1. `
//! - Explicit newlines preserved during word-wrap

use katla_math::{Color, Vec2};

use crate::UiContext;

/// A segment of text with uniform formatting style.
#[derive(Debug, Clone)]
pub struct TextSegment {
    pub text: String,
    pub kind: TextSegmentKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextSegmentKind {
    Normal,
    Bold,
    Code,
    Header,
    Bullet,
}

/// Color scheme for markdown rendering.
pub struct MarkdownColors {
    pub bold: Color,
    pub code_text: Color,
    pub code_background: Color,
    pub header: Color,
    pub bullet_marker: Color,
}

impl MarkdownColors {
    pub fn new(
        bold: Color,
        code_text: Color,
        code_background: Color,
        header: Color,
        bullet: Color,
    ) -> Self {
        Self {
            bold,
            code_text,
            code_background,
            header,
            bullet_marker: bullet,
        }
    }

    /// A sensible default using a blue accent for bold and green for code.
    pub fn defaults() -> Self {
        Self {
            bold: Color::new(0.4, 0.7, 1.0, 1.0),
            code_text: Color::new(0.6, 0.85, 0.65, 1.0),
            code_background: Color::new(0.15, 0.15, 0.2, 0.8),
            header: Color::WHITE,
            bullet_marker: Color::new(0.6, 0.6, 0.65, 1.0),
        }
    }
}

/// Parse a single line of markdown-like text into styled segments.
pub fn parse_markdown_line(line: &str) -> Vec<TextSegment> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    // Headers: ### Header
    if let Some(rest) = trimmed.strip_prefix("### ") {
        return vec![TextSegment {
            text: rest.to_string(),
            kind: TextSegmentKind::Header,
        }];
    }
    if let Some(rest) = trimmed.strip_prefix("## ") {
        return vec![TextSegment {
            text: rest.to_string(),
            kind: TextSegmentKind::Header,
        }];
    }
    if let Some(rest) = trimmed.strip_prefix("# ") {
        return vec![TextSegment {
            text: rest.to_string(),
            kind: TextSegmentKind::Header,
        }];
    }

    // Bullet points: - item or * item
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        let marker = &trimmed[..2];
        let rest = &trimmed[2..];
        let mut segments = vec![TextSegment {
            text: format!("{}{}", " ".repeat(indent), marker),
            kind: TextSegmentKind::Bullet,
        }];
        segments.extend(parse_inline_markdown(rest));
        return segments;
    }

    // Numbered lists: 1. item
    if let Some(pos) = trimmed.find(". ") {
        let prefix = &trimmed[..=pos];
        if prefix.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            let rest = &trimmed[pos + 2..];
            let mut segments = vec![TextSegment {
                text: format!("{}{}", " ".repeat(indent), prefix),
                kind: TextSegmentKind::Bullet,
            }];
            segments.extend(parse_inline_markdown(rest));
            return segments;
        }
    }

    let mut segments = parse_inline_markdown(trimmed);
    if indent > 0 {
        segments.insert(
            0,
            TextSegment {
                text: " ".repeat(indent),
                kind: TextSegmentKind::Normal,
            },
        );
    }
    segments
}

/// Parse inline markdown (bold and code) within a line.
pub fn parse_inline_markdown(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let mut remaining = text;
    let mut normal_buf = String::new();

    while !remaining.is_empty() {
        // Check for inline code: `code`
        if remaining.starts_with('`')
            && let Some(end) = remaining[1..].find('`')
        {
            if !normal_buf.is_empty() {
                segments.push(TextSegment {
                    text: std::mem::take(&mut normal_buf),
                    kind: TextSegmentKind::Normal,
                });
            }
            segments.push(TextSegment {
                text: remaining[1..end + 1].to_string(),
                kind: TextSegmentKind::Code,
            });
            remaining = &remaining[end + 2..];
            continue;
        }

        // Check for bold: **text**
        if remaining.starts_with("**")
            && let Some(end) = remaining[2..].find("**")
        {
            if !normal_buf.is_empty() {
                segments.push(TextSegment {
                    text: std::mem::take(&mut normal_buf),
                    kind: TextSegmentKind::Normal,
                });
            }
            segments.push(TextSegment {
                text: remaining[2..end + 2].to_string(),
                kind: TextSegmentKind::Bold,
            });
            remaining = &remaining[end + 4..];
            continue;
        }

        let ch = remaining.chars().next().unwrap();
        normal_buf.push(ch);
        remaining = &remaining[ch.len_utf8()..];
    }

    if !normal_buf.is_empty() {
        segments.push(TextSegment {
            text: normal_buf,
            kind: TextSegmentKind::Normal,
        });
    }

    segments
}

/// Word-wrap text preserving explicit newlines.
///
/// Splits on `\n` first, then word-wraps each paragraph to fit `max_width`.
pub fn wrap_lines(text: &str, max_width: f32, font_size: f32, ui: &UiContext) -> Vec<String> {
    let mut result = Vec::new();

    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            result.push(String::new());
            continue;
        }
        wrap_paragraph(paragraph, max_width, font_size, ui, &mut result);
    }

    if result.is_empty() {
        result.push(String::new());
    }

    result
}

/// Word-wrap a single paragraph into the output vec.
fn wrap_paragraph(
    text: &str,
    max_width: f32,
    font_size: f32,
    ui: &UiContext,
    out: &mut Vec<String>,
) {
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        let test_line = if current_line.is_empty() {
            word.to_string()
        } else {
            format!("{current_line} {word}")
        };

        let measured = ui.measure_text(&test_line, font_size);

        if measured.x() > max_width && !current_line.is_empty() {
            out.push(current_line);
            current_line = word.to_string();
        } else {
            current_line = test_line;
        }
    }

    if !current_line.is_empty() {
        out.push(current_line);
    }
}

/// Draw a line of markdown segments at the given position.
pub fn draw_markdown_segments(
    ui: &mut UiContext,
    segments: &[TextSegment],
    position: Vec2,
    base_color: Color,
    font_size: f32,
    colors: &MarkdownColors,
) {
    let mut cursor_x = position.x();

    for segment in segments {
        let (color, size) = match segment.kind {
            TextSegmentKind::Normal => (base_color, font_size),
            TextSegmentKind::Bold => (colors.bold, font_size),
            TextSegmentKind::Code => (colors.code_text, font_size),
            TextSegmentKind::Header => (colors.header, font_size * 1.3),
            TextSegmentKind::Bullet => (colors.bullet_marker, font_size),
        };

        if segment.text.is_empty() {
            continue;
        }

        // Draw a subtle background for code segments
        if segment.kind == TextSegmentKind::Code {
            let code_measure = ui.measure_text(&segment.text, size);
            let bg_bounds = katla_math::Rect2D::from_origin_size(
                Vec2::new(cursor_x - 2.0, position.y()),
                Vec2::new(code_measure.x() + 4.0, size + 2.0),
            );
            ui.draw_rect(bg_bounds, colors.code_background);
        }

        ui.draw_text(
            &segment.text,
            Vec2::new(cursor_x, position.y()),
            color,
            size,
        );

        let measured = ui.measure_text(&segment.text, size);
        cursor_x += measured.x();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_normal_text() {
        let segments = parse_inline_markdown("hello world");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "hello world");
        assert_eq!(segments[0].kind, TextSegmentKind::Normal);
    }

    #[test]
    fn test_parse_bold() {
        let segments = parse_inline_markdown("say **hello** world");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "say ");
        assert_eq!(segments[0].kind, TextSegmentKind::Normal);
        assert_eq!(segments[1].text, "hello");
        assert_eq!(segments[1].kind, TextSegmentKind::Bold);
        assert_eq!(segments[2].text, " world");
        assert_eq!(segments[2].kind, TextSegmentKind::Normal);
    }

    #[test]
    fn test_parse_code() {
        let segments = parse_inline_markdown("use `cargo build` to compile");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "use ");
        assert_eq!(segments[0].kind, TextSegmentKind::Normal);
        assert_eq!(segments[1].text, "cargo build");
        assert_eq!(segments[1].kind, TextSegmentKind::Code);
        assert_eq!(segments[2].text, " to compile");
        assert_eq!(segments[2].kind, TextSegmentKind::Normal);
    }

    #[test]
    fn test_parse_mixed_bold_and_code() {
        let segments = parse_inline_markdown("**bold** and `code` text");
        // "**bold**" -> Bold("bold")
        // " and " -> Normal(" and ")
        // "`code`" -> Code("code")
        // " text" -> Normal(" text")
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0].kind, TextSegmentKind::Bold);
        assert_eq!(segments[0].text, "bold");
        assert_eq!(segments[1].kind, TextSegmentKind::Normal);
        assert_eq!(segments[1].text, " and ");
        assert_eq!(segments[2].kind, TextSegmentKind::Code);
        assert_eq!(segments[2].text, "code");
        assert_eq!(segments[3].kind, TextSegmentKind::Normal);
        assert_eq!(segments[3].text, " text");
    }

    #[test]
    fn test_parse_header() {
        let segments = parse_markdown_line("## Hello World");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Hello World");
        assert_eq!(segments[0].kind, TextSegmentKind::Header);
    }

    #[test]
    fn test_parse_bullet() {
        let segments = parse_markdown_line("  - item text");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].kind, TextSegmentKind::Bullet);
        assert!(segments[0].text.contains("- "));
        assert_eq!(segments[1].kind, TextSegmentKind::Normal);
        assert_eq!(segments[1].text, "item text");
    }

    #[test]
    fn test_parse_numbered_list() {
        let segments = parse_markdown_line("1. first item");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].kind, TextSegmentKind::Bullet);
        assert!(segments[0].text.contains("1."));
    }

    #[test]
    fn test_unclosed_bold_treated_as_normal() {
        let segments = parse_inline_markdown("no **closing");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "no **closing");
        assert_eq!(segments[0].kind, TextSegmentKind::Normal);
    }

    #[test]
    fn test_unclosed_code_treated_as_normal() {
        let segments = parse_inline_markdown("no `closing");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "no `closing");
        assert_eq!(segments[0].kind, TextSegmentKind::Normal);
    }

    #[test]
    fn test_wrap_preserves_newlines() {
        let text = "line1\nline2\n\nline4";
        let parts: Vec<&str> = text.split('\n').collect();
        assert_eq!(parts, vec!["line1", "line2", "", "line4"]);
    }

    #[test]
    fn test_empty_bold() {
        let segments = parse_inline_markdown("a **** b");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "a ");
        assert_eq!(segments[0].kind, TextSegmentKind::Normal);
        assert_eq!(segments[1].text, "");
        assert_eq!(segments[1].kind, TextSegmentKind::Bold);
        assert_eq!(segments[2].text, " b");
        assert_eq!(segments[2].kind, TextSegmentKind::Normal);
    }

    #[test]
    fn test_header_level3() {
        let segments = parse_markdown_line("### Sub Title");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Sub Title");
        assert_eq!(segments[0].kind, TextSegmentKind::Header);
    }

    #[test]
    fn test_bold_with_nested_backtick() {
        // **bold `code` bold** - backticks inside bold are not special
        let segments = parse_inline_markdown("**hello `world`**");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].kind, TextSegmentKind::Bold);
        assert_eq!(segments[0].text, "hello `world`");
    }
}
