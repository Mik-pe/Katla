//! Lightweight markdown-like text rendering for UI panels.
//!
//! Supports:
//! - **bold** via `**text**`
//! - *italic* via `*text*`
//! - `code` via `` `text` ``
//! - ``` ``` ``` fenced code blocks with preserved whitespace
//! - Headers via `# `, `## `, `### `, `#### ` with size hierarchy
//! - Bullet points via `- ` or `* `
//! - Numbered lists via `1. `
//! - Links via `[text](url)`
//! - Blockquotes via `> `
//! - Horizontal rules via `---` or `***`
//! - Explicit newlines preserved during word-wrap

use katla_math::{Color, Rect2D, Vec2};

use crate::{
    UiContext,
    style::{FontSize, UiStyle},
};

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
    Italic,
    Code,
    CodeBlock,
    Header(HeaderLevel),
    Bullet,
    Link,
    Blockquote,
    HRule,
}

/// Heading level for hierarchical sizing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HeaderLevel {
    H1,
    H2,
    H3,
    H4,
}

impl HeaderLevel {
    pub fn font_size(self) -> FontSize {
        match self {
            HeaderLevel::H1 => FontSize::XLarge,
            HeaderLevel::H2 => FontSize::Large,
            HeaderLevel::H3 => FontSize::Medium,
            HeaderLevel::H4 => FontSize::Small,
        }
    }

    pub fn spacing_above(self) -> f32 {
        match self {
            HeaderLevel::H1 => 12.0,
            HeaderLevel::H2 => 10.0,
            HeaderLevel::H3 => 8.0,
            HeaderLevel::H4 => 6.0,
        }
    }

    pub fn spacing_below(self) -> f32 {
        match self {
            HeaderLevel::H1 => 6.0,
            HeaderLevel::H2 => 4.0,
            HeaderLevel::H3 => 4.0,
            HeaderLevel::H4 => 2.0,
        }
    }
}

/// Color scheme for markdown rendering.
pub struct MarkdownColors {
    pub bold: Color,
    pub italic: Color,
    pub code_text: Color,
    pub code_background: Color,
    pub header: Color,
    pub bullet_marker: Color,
    pub link_color: Color,
    pub quote_border: Color,
    pub quote_text: Color,
    pub hrule_color: Color,
}

impl MarkdownColors {
    pub fn from_style(style: &UiStyle) -> Self {
        Self {
            bold: style.input_border_focused,
            italic: style.text_disabled,
            code_text: style.slider_grab,
            code_background: style.input_bg,
            header: style.text_color,
            bullet_marker: style.text_disabled,
            link_color: style.slider_grab,
            quote_border: style.text_hint,
            quote_text: style.text_disabled,
            hrule_color: style.separator,
        }
    }
}

/// A parsed markdown block representing a logical unit of content.
#[derive(Debug, Clone)]
pub enum MarkdownBlock {
    /// A line of inline segments (normal text, bold, italic, code, etc.)
    Line(Vec<TextSegment>),
    /// A fenced code block with multiple lines of preformatted text.
    CodeBlock { lines: Vec<String> },
    /// A horizontal rule.
    HRule,
}

/// Parse a complete markdown text into blocks, handling fenced code blocks
/// and line-by-line parsing for everything else.
pub fn parse_markdown_blocks(text: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let lines = text.split('\n');
    let mut in_code_block = false;
    let mut code_block_lines: Vec<String> = Vec::new();

    for line in lines {
        // Check for fenced code block delimiters
        if line.trim_start().starts_with("```") {
            if in_code_block {
                // End of code block
                in_code_block = false;
                blocks.push(MarkdownBlock::CodeBlock {
                    lines: std::mem::take(&mut code_block_lines),
                });
            } else {
                // Start of code block
                in_code_block = true;
                code_block_lines.clear();
            }
            continue;
        }

        if in_code_block {
            code_block_lines.push(line.to_string());
            continue;
        }

        // Check for horizontal rule: --- or *** (must be only that on the line)
        let trimmed = line.trim();
        if (trimmed.starts_with("---") && trimmed.chars().all(|c| c == '-'))
            || (trimmed.starts_with("***") && trimmed.chars().all(|c| c == '*'))
        {
            blocks.push(MarkdownBlock::HRule);
            continue;
        }

        // Parse as regular line
        let segments = parse_markdown_line(line);
        blocks.push(MarkdownBlock::Line(segments));
    }

    // Handle unclosed code block
    if in_code_block && !code_block_lines.is_empty() {
        blocks.push(MarkdownBlock::CodeBlock {
            lines: code_block_lines,
        });
    }

    blocks
}

/// Parse a single line of markdown-like text into styled segments.
pub fn parse_markdown_line(line: &str) -> Vec<TextSegment> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    // Blockquote: > text
    if let Some(rest) = trimmed.strip_prefix("> ") {
        let mut segments = vec![TextSegment {
            text: rest.to_string(),
            kind: TextSegmentKind::Blockquote,
        }];
        // Also parse inline markdown within the blockquote content
        let inline = parse_inline_markdown(rest);
        if inline.len() > 1 || (inline.len() == 1 && inline[0].kind != TextSegmentKind::Normal) {
            segments = inline;
        }
        // Ensure the blockquote sentinel is present for the renderer to
        // detect this line as a blockquote (draws left border + indentation).
        if !segments
            .iter()
            .any(|s| s.kind == TextSegmentKind::Blockquote)
        {
            segments.insert(
                0,
                TextSegment {
                    text: String::new(),
                    kind: TextSegmentKind::Blockquote,
                },
            );
        }
        return segments;
    }
    // Standalone > (empty blockquote line)
    if trimmed == ">" {
        return vec![TextSegment {
            text: String::new(),
            kind: TextSegmentKind::Blockquote,
        }];
    }

    // Headers: #### Header, ### Header, ## Header, # Header
    if let Some(rest) = trimmed.strip_prefix("#### ") {
        return vec![TextSegment {
            text: rest.to_string(),
            kind: TextSegmentKind::Header(HeaderLevel::H4),
        }];
    }
    if let Some(rest) = trimmed.strip_prefix("### ") {
        return vec![TextSegment {
            text: rest.to_string(),
            kind: TextSegmentKind::Header(HeaderLevel::H3),
        }];
    }
    if let Some(rest) = trimmed.strip_prefix("## ") {
        return vec![TextSegment {
            text: rest.to_string(),
            kind: TextSegmentKind::Header(HeaderLevel::H2),
        }];
    }
    if let Some(rest) = trimmed.strip_prefix("# ") {
        return vec![TextSegment {
            text: rest.to_string(),
            kind: TextSegmentKind::Header(HeaderLevel::H1),
        }];
    }

    // Bullet points: - item (but not --- which is a horizontal rule)
    if let Some(rest) = trimmed.strip_prefix("- ") {
        let mut segments = vec![TextSegment {
            text: format!("{}{}", " ".repeat(indent), "- "),
            kind: TextSegmentKind::Bullet,
        }];
        segments.extend(parse_inline_markdown(rest));
        return segments;
    }

    // Bullet points: * item (single asterisk, distinguish from ** bold)
    if trimmed.starts_with("* ")
        && !trimmed.starts_with("** ")
        && !trimmed.chars().all(|c| c == '*')
    {
        let rest = &trimmed[2..];
        let mut segments = vec![TextSegment {
            text: format!("{}{}", " ".repeat(indent), "* "),
            kind: TextSegmentKind::Bullet,
        }];
        segments.extend(parse_inline_markdown(rest));
        return segments;
    }

    // Numbered lists: 1. item (prefix must be all digits followed by ". ")
    if let Some(pos) = trimmed.find(". ") {
        let prefix = &trimmed[..pos];
        if !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_digit()) {
            let rest = &trimmed[pos + 2..];
            let mut segments = vec![TextSegment {
                text: format!("{}{}{}. ", " ".repeat(indent), prefix, "."),
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

/// Parse inline markdown (bold, italic, code, links) within a line.
pub fn parse_inline_markdown(text: &str) -> Vec<TextSegment> {
    let mut segments = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut normal_buf = String::new();
    // Track the "virtual" previous char: when we skip chars (e.g. after bold **),
    // the real prev char in the array isn't representative of what was consumed.
    let mut prev_consumed_was_asterisk = false;

    while i < len {
        // Check for inline code: `code`
        if chars[i] == '`'
            && let Some(end) = chars[i + 1..].iter().position(|&c| c == '`')
        {
            if !normal_buf.is_empty() {
                segments.push(TextSegment {
                    text: std::mem::take(&mut normal_buf),
                    kind: TextSegmentKind::Normal,
                });
            }
            let code_text: String = chars[i + 1..i + 1 + end].iter().collect();
            segments.push(TextSegment {
                text: code_text,
                kind: TextSegmentKind::Code,
            });
            i = i + 1 + end + 1;
            prev_consumed_was_asterisk = false;
            continue;
        }

        // Check for bold: **text**
        if chars[i] == '*'
            && i + 1 < len
            && chars[i + 1] == '*'
            && let Some(end) = find_closing_marker(&chars, i + 2, '*', '*')
        {
            if !normal_buf.is_empty() {
                segments.push(TextSegment {
                    text: std::mem::take(&mut normal_buf),
                    kind: TextSegmentKind::Normal,
                });
            }
            let bold_text: String = chars[i + 2..end].iter().collect();
            segments.push(TextSegment {
                text: bold_text,
                kind: TextSegmentKind::Bold,
            });
            i = end + 2;
            continue;
        }

        // Check for italic: *text* (single asterisk)
        // Only match if: not preceded by asterisk (either real or virtual)
        // and not followed by another asterisk
        if chars[i] == '*'
            && (i + 1 >= len || chars[i + 1] != '*')
            && !prev_consumed_was_asterisk
            && let Some(end) = find_closing_marker(&chars, i + 1, '*', '\0')
        {
            if !normal_buf.is_empty() {
                segments.push(TextSegment {
                    text: std::mem::take(&mut normal_buf),
                    kind: TextSegmentKind::Normal,
                });
            }
            let italic_text: String = chars[i + 1..end].iter().collect();
            segments.push(TextSegment {
                text: italic_text,
                kind: TextSegmentKind::Italic,
            });
            i = end + 1;
            prev_consumed_was_asterisk = true;
            continue;
        }

        // Check for link: [text](url)
        if chars[i] == '['
            && let Some(bracket_end) = chars[i + 1..].iter().position(|&c| c == ']')
        {
            let url_start = i + 1 + bracket_end + 1;
            if url_start < len
                && chars[url_start] == '('
                && let Some(paren_end) = chars[url_start + 1..].iter().position(|&c| c == ')')
            {
                if !normal_buf.is_empty() {
                    segments.push(TextSegment {
                        text: std::mem::take(&mut normal_buf),
                        kind: TextSegmentKind::Normal,
                    });
                }
                let link_text: String = chars[i + 1..i + 1 + bracket_end].iter().collect();
                let url: String = chars[url_start + 1..url_start + 1 + paren_end]
                    .iter()
                    .collect();
                segments.push(TextSegment {
                    text: format!("{}|{}", link_text, url),
                    kind: TextSegmentKind::Link,
                });
                i = url_start + 1 + paren_end + 1;
                prev_consumed_was_asterisk = false;
                continue;
            }
        }

        prev_consumed_was_asterisk = chars[i] == '*';
        normal_buf.push(chars[i]);
        i += 1;
    }

    if !normal_buf.is_empty() {
        segments.push(TextSegment {
            text: normal_buf,
            kind: TextSegmentKind::Normal,
        });
    }

    segments
}

/// Find the closing marker position in a char slice starting from `start`.
/// For bold: looks for two consecutive `marker1` chars.
/// For italic: looks for single `marker1` char (marker2 is '\0').
fn find_closing_marker(
    chars: &[char],
    start: usize,
    marker1: char,
    marker2: char,
) -> Option<usize> {
    for i in start..chars.len() {
        if marker2 != '\0' {
            // Bold: need two consecutive markers
            if chars[i] == marker1 && i + 1 < chars.len() && chars[i + 1] == marker2 {
                return Some(i);
            }
        } else {
            // Italic: single marker, not adjacent to another marker
            if chars[i] == marker1 && (i + 1 >= chars.len() || chars[i + 1] != marker1) {
                return Some(i);
            }
        }
    }
    None
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
    let mut current_width = 0.0f32;
    let space_width = ui.measure_text(" ", font_size).x();

    for word in text.split_whitespace() {
        let word_width = ui.measure_text(word, font_size).x();

        if current_line.is_empty() {
            current_line.push_str(word);
            current_width = word_width;
        } else {
            let new_width = current_width + space_width + word_width;

            if new_width > max_width {
                out.push(current_line);
                current_line = word.to_string();
                current_width = word_width;
            } else {
                current_line.push(' ');
                current_line.push_str(word);
                current_width = new_width;
            }
        }
    }

    if !current_line.is_empty() {
        out.push(current_line);
    }
}

/// Spacing constants for markdown layout.
pub mod spacing {
    pub const AFTER_HEADING: f32 = 6.0;
    pub const AFTER_PARAGRAPH: f32 = 4.0;
    pub const AFTER_LIST_ITEM: f32 = 2.0;
    pub const AFTER_CODE_BLOCK: f32 = 6.0;
    pub const AFTER_HRULE: f32 = 6.0;
    pub const BLOCKQUOTE_INDENT: f32 = 12.0;
    pub const BLOCKQUOTE_BORDER_WIDTH: f32 = 3.0;
}

/// Render a complete markdown text into a UI context with proper block-level layout.
///
/// Returns the total height consumed.
pub fn render_markdown(
    ui: &mut UiContext,
    text: &str,
    position: Vec2,
    max_width: f32,
    base_color: Color,
    font_size: f32,
    colors: &MarkdownColors,
) -> f32 {
    let blocks = parse_markdown_blocks(text);
    let mut y = position.y();

    for block in &blocks {
        match block {
            MarkdownBlock::Line(segments) => {
                if segments.is_empty() {
                    y += font_size;
                    continue;
                }

                // Determine if this is a blockquote
                let is_blockquote = segments
                    .iter()
                    .any(|s| s.kind == TextSegmentKind::Blockquote);

                // Determine header level for spacing
                let header_level = segments.iter().find_map(|s| {
                    if let TextSegmentKind::Header(level) = s.kind {
                        Some(level)
                    } else {
                        None
                    }
                });

                // Add spacing above headers
                if let Some(level) = header_level {
                    y += level.spacing_above();
                }

                let draw_x = if is_blockquote {
                    // Draw blockquote left border
                    let border_rect = Rect2D::from_origin_size(
                        Vec2::new(position.x(), y),
                        Vec2::new(spacing::BLOCKQUOTE_BORDER_WIDTH, font_size),
                    );
                    ui.draw_rect(border_rect, colors.quote_border);
                    position.x() + spacing::BLOCKQUOTE_INDENT
                } else {
                    position.x()
                };

                draw_markdown_segments(
                    ui,
                    segments,
                    Vec2::new(draw_x, y),
                    base_color,
                    font_size,
                    colors,
                );

                // Determine the actual line height used
                let line_height = if let Some(level) = header_level {
                    ui.scaled_font_size(level.font_size())
                } else {
                    font_size
                };

                y += line_height;

                // Add spacing below headers
                if let Some(level) = header_level {
                    y += level.spacing_below();
                } else if is_blockquote {
                    y += spacing::AFTER_PARAGRAPH;
                }
            }

            MarkdownBlock::CodeBlock { lines } => {
                let code_font_size = font_size;
                let line_height = code_font_size + 2.0;
                let block_height = lines.len() as f32 * line_height;

                // Draw background for the entire code block
                let bg_rect = Rect2D::from_origin_size(
                    Vec2::new(position.x(), y),
                    Vec2::new(max_width, block_height + 4.0),
                );
                ui.draw_rect(bg_rect, colors.code_background);

                // Draw each line of the code block
                let mut code_y = y + 2.0;
                for code_line in lines {
                    if !code_line.is_empty() {
                        ui.draw_text(
                            code_line,
                            Vec2::new(position.x() + 4.0, code_y),
                            colors.code_text,
                            code_font_size,
                        );
                    }
                    code_y += line_height;
                }

                y += block_height + 4.0 + spacing::AFTER_CODE_BLOCK;
            }

            MarkdownBlock::HRule => {
                // Draw a horizontal rule line
                let rule_rect = Rect2D::from_origin_size(
                    Vec2::new(position.x(), y + font_size * 0.4),
                    Vec2::new(max_width, 1.0),
                );
                ui.draw_rect(rule_rect, colors.hrule_color);
                y += font_size + spacing::AFTER_HRULE;
            }
        }
    }

    y - position.y()
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
            TextSegmentKind::Italic => (colors.italic, font_size),
            TextSegmentKind::Code => (colors.code_text, font_size),
            TextSegmentKind::CodeBlock => (colors.code_text, font_size),
            TextSegmentKind::Header(level) => {
                let fs = ui.scaled_font_size(level.font_size());
                (colors.header, fs)
            }
            TextSegmentKind::Bullet => (colors.bullet_marker, font_size),
            TextSegmentKind::Link => (colors.link_color, font_size),
            TextSegmentKind::Blockquote => (colors.quote_text, font_size),
            TextSegmentKind::HRule => (colors.hrule_color, font_size),
        };

        if segment.text.is_empty() {
            continue;
        }

        // Draw a background for inline code segments
        if segment.kind == TextSegmentKind::Code {
            let code_measure = ui.measure_text(&segment.text, size);
            let bg_bounds = Rect2D::from_origin_size(
                Vec2::new(cursor_x - 2.0, position.y()),
                Vec2::new(code_measure.x() + 4.0, size + 2.0),
            );
            ui.draw_rect(bg_bounds, colors.code_background);
        }

        // For links, display just the text portion (before the |)
        let display_text = if segment.kind == TextSegmentKind::Link {
            segment.text.split('|').next().unwrap_or(&segment.text)
        } else {
            &segment.text
        };

        if !display_text.is_empty() {
            ui.draw_text(display_text, Vec2::new(cursor_x, position.y()), color, size);

            let measured = ui.measure_text(display_text, size);
            cursor_x += measured.x();
        }
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
    fn test_parse_italic() {
        let segments = parse_inline_markdown("say *hello* world");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "say ");
        assert_eq!(segments[0].kind, TextSegmentKind::Normal);
        assert_eq!(segments[1].text, "hello");
        assert_eq!(segments[1].kind, TextSegmentKind::Italic);
        assert_eq!(segments[2].text, " world");
        assert_eq!(segments[2].kind, TextSegmentKind::Normal);
    }

    #[test]
    fn test_italic_vs_bold_disambiguation() {
        // **bold** should be Bold, not Italic; *italic* should be Italic
        let segments = parse_inline_markdown("**bold** and *italic*");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].kind, TextSegmentKind::Bold);
        assert_eq!(segments[0].text, "bold");
        assert_eq!(segments[1].kind, TextSegmentKind::Normal);
        assert_eq!(segments[1].text, " and ");
        assert_eq!(segments[2].kind, TextSegmentKind::Italic);
        assert_eq!(segments[2].text, "italic");
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
    fn test_parse_link() {
        let segments = parse_inline_markdown("click [here](https://example.com) now");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "click ");
        assert_eq!(segments[0].kind, TextSegmentKind::Normal);
        assert_eq!(segments[1].text, "here|https://example.com");
        assert_eq!(segments[1].kind, TextSegmentKind::Link);
        assert_eq!(segments[2].text, " now");
        assert_eq!(segments[2].kind, TextSegmentKind::Normal);
    }

    #[test]
    fn test_parse_blockquote() {
        let segments = parse_markdown_line("> this is a quote");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].kind, TextSegmentKind::Blockquote);
    }

    #[test]
    fn test_parse_hrule_dashes() {
        let blocks = parse_markdown_blocks("---");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], MarkdownBlock::HRule));
    }

    #[test]
    fn test_parse_hrule_stars() {
        let blocks = parse_markdown_blocks("***");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], MarkdownBlock::HRule));
    }

    #[test]
    fn test_parse_code_block() {
        let input = "before\n```\nlet x = 1;\nlet y = 2;\n```\nafter";
        let blocks = parse_markdown_blocks(input);
        assert_eq!(blocks.len(), 3);
        assert!(matches!(&blocks[0], MarkdownBlock::Line(_)));
        assert!(matches!(&blocks[1], MarkdownBlock::CodeBlock { .. }));
        assert!(matches!(&blocks[2], MarkdownBlock::Line(_)));

        if let MarkdownBlock::CodeBlock { lines } = &blocks[1] {
            assert_eq!(lines.len(), 2);
            assert_eq!(lines[0], "let x = 1;");
            assert_eq!(lines[1], "let y = 2;");
        }
    }

    #[test]
    fn test_parse_mixed_bold_and_code() {
        let segments = parse_inline_markdown("**bold** and `code` text");
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
    fn test_parse_header_levels() {
        let segs = parse_markdown_line("# H1");
        assert_eq!(segs[0].kind, TextSegmentKind::Header(HeaderLevel::H1));

        let segs = parse_markdown_line("## H2");
        assert_eq!(segs[0].kind, TextSegmentKind::Header(HeaderLevel::H2));

        let segs = parse_markdown_line("### H3");
        assert_eq!(segs[0].kind, TextSegmentKind::Header(HeaderLevel::H3));

        let segs = parse_markdown_line("#### H4");
        assert_eq!(segs[0].kind, TextSegmentKind::Header(HeaderLevel::H4));
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
    fn test_unclosed_italic_treated_as_normal() {
        let segments = parse_inline_markdown("no *closing");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "no *closing");
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
    fn test_bold_with_nested_backtick() {
        let segments = parse_inline_markdown("**hello `world`**");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].kind, TextSegmentKind::Bold);
        assert_eq!(segments[0].text, "hello `world`");
    }

    #[test]
    fn test_header_size_hierarchy() {
        assert_eq!(HeaderLevel::H1.font_size(), FontSize::XLarge);
        assert_eq!(HeaderLevel::H2.font_size(), FontSize::Large);
        assert_eq!(HeaderLevel::H3.font_size(), FontSize::Medium);
        assert_eq!(HeaderLevel::H4.font_size(), FontSize::Small);
    }

    #[test]
    fn test_header_spacing() {
        assert_eq!(HeaderLevel::H1.spacing_above(), 12.0);
        assert_eq!(HeaderLevel::H1.spacing_below(), 6.0);
        assert_eq!(HeaderLevel::H2.spacing_above(), 10.0);
        assert_eq!(HeaderLevel::H2.spacing_below(), 4.0);
        assert_eq!(HeaderLevel::H3.spacing_above(), 8.0);
        assert_eq!(HeaderLevel::H3.spacing_below(), 4.0);
        assert_eq!(HeaderLevel::H4.spacing_above(), 6.0);
        assert_eq!(HeaderLevel::H4.spacing_below(), 2.0);
    }

    #[test]
    fn test_hrule_not_dash_list() {
        // --- should be HRule, not a bullet list item
        let blocks = parse_markdown_blocks("---\n- item");
        assert_eq!(blocks.len(), 2);
        assert!(matches!(blocks[0], MarkdownBlock::HRule));
        if let MarkdownBlock::Line(segs) = &blocks[1] {
            assert_eq!(segs[0].kind, TextSegmentKind::Bullet);
        }
    }

    #[test]
    fn test_unclosed_link_bracket_as_normal() {
        let segments = parse_inline_markdown("no [closing");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "no [closing");
        assert_eq!(segments[0].kind, TextSegmentKind::Normal);
    }

    #[test]
    fn test_unclosed_link_paren_as_normal() {
        let segments = parse_inline_markdown("no [text](unclosed");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "no [text](unclosed");
        assert_eq!(segments[0].kind, TextSegmentKind::Normal);
    }
}
