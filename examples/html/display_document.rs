use crate::html_document::{Block, BlockClass, Span, SpanClass};

const COLOR_CODE: [f32; 4] = [0.2, 0.2, 0.2, 1.];
const COLOR_IMAGE_PLACEHOLDER: [f32; 4] = [0.8, 0., 0., 1.];
const COLOR_LINK: [f32; 4] = [
    0.09803921568627451,
    0.4627450980392157,
    0.8235294117647058,
    1.,
];
const COLOR_REGULAR: [f32; 4] = [0., 0., 0., 1.];
const FONT_SIZE_REGULAR: f32 = 18.;
const INDENT: f32 = 32.;
const PARAGRAPH_SPACING: f32 = 14.;
const WIDTH: f32 = 540.;

pub struct Display<'font> {
    glyphs: Vec<gfx_glyph::LayoutGlyph<'font>>,
    lines: Vec<Line>,
}

struct Line {
    bounds_y: std::ops::Range<f32>,
    glyphs: std::ops::Range<usize>,
}

impl<'font> Display<'font> {
    pub fn bound_y_max(&self) -> f32 {
        match self.lines.last() {
            None => 0.,
            Some(line) => line.bounds_y.end,
        }
    }

    pub fn clip(&self, bound_y_min: f32, bound_y_max: f32) -> &[gfx_glyph::LayoutGlyph] {
        let end_line_index = self
            .lines
            .binary_search_by(|line| {
                if line.bounds_y.start < bound_y_max {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            })
            .unwrap_err();
        let start_line_index = self.lines[..end_line_index]
            .binary_search_by(|line| {
                if line.bounds_y.end > bound_y_min {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Less
                }
            })
            .unwrap_err();
        if end_line_index > start_line_index {
            let start_glyph_index = self.lines[start_line_index].glyphs.start;
            let end_glyph_index = self.lines[end_line_index - 1].glyphs.end;
            &self.glyphs[start_glyph_index..end_glyph_index]
        } else {
            &[]
        }
    }
}

pub fn display<'font>(
    document: &Vec<Block>,
    fonts: &[gfx_glyph::Font<'font>],
    position_x: f32,
    mut position_y: f32,
    scale: f32,
) -> Display<'font> {
    let mut display = Display {
        glyphs: vec![],
        lines: vec![],
    };
    let v_metrics = fonts[0].v_metrics(gfx_glyph::Scale::uniform(1.));
    for block in document.iter() {
        match block {
            Block::Flowing { class, content } => {
                let block_scale = match class {
                    BlockClass::Heading1 => 2.,
                    BlockClass::Heading2 => 1.5,
                    BlockClass::Heading3 => 1.17,
                    BlockClass::Heading4 => 1.,
                    BlockClass::Heading5 => 0.83,
                    BlockClass::Heading6 => 0.67,
                    BlockClass::ListItem => 1.,
                    BlockClass::Paragraph => 1.,
                    BlockClass::Preformatted => 1.,
                } * FONT_SIZE_REGULAR
                    * scale;
                let mut content = &content[..];
                let left_margin = match class {
                    BlockClass::ListItem | BlockClass::Preformatted => INDENT * scale,
                    _ => 0.,
                };
                let mut display_bullet = match class {
                    BlockClass::ListItem => true,
                    _ => false,
                };
                let mut start_point = 0;
                loop {
                    let mut break_point = (0, 0);
                    let mut caret_position_x = left_margin;
                    let mut last_font_id = 0;
                    let mut last_glyph = None;
                    let mut wrap = false;
                    'a: for (span_index, span) in content.iter().enumerate() {
                        match span {
                            Span::LineBreak => {
                                break_point = (span_index + 1, 0);
                                wrap = true;
                                break;
                            }
                            Span::Text { class, text } => {
                                let text = if span_index == 0 {
                                    &text[start_point..]
                                } else {
                                    &text[..]
                                };
                                let font_id = match class {
                                    SpanClass::Bold | SpanClass::BoldLink => 1,
                                    SpanClass::BoldItalic | SpanClass::BoldItalicLink => 2,
                                    SpanClass::Code => 4,
                                    SpanClass::Italic | SpanClass::ItalicLink => 3,
                                    SpanClass::Link | SpanClass::Regular => 0,
                                };
                                if font_id != last_font_id {
                                    last_glyph = None;
                                    last_font_id = font_id;
                                }
                                let font = &fonts[font_id];
                                for (character_position, character) in text.char_indices() {
                                    if character.is_whitespace() {
                                        break_point = (span_index, character_position);
                                    }
                                    let glyph = font
                                        .glyph(map_character(character))
                                        .scaled(gfx_glyph::Scale::uniform(block_scale));
                                    if let Some(last_glyph) = last_glyph {
                                        caret_position_x += font.pair_kerning(
                                            gfx_glyph::Scale::uniform(block_scale),
                                            last_glyph,
                                            glyph.id(),
                                        );
                                    }
                                    caret_position_x += glyph.h_metrics().advance_width;
                                    if caret_position_x > WIDTH {
                                        if break_point == (0, 0) {
                                            break_point = (span_index, character_position);
                                            if break_point != (0, 0) {
                                                wrap = true;
                                                break 'a;
                                            }
                                        } else {
                                            wrap = true;
                                            break 'a;
                                        }
                                    }
                                    last_glyph = Some(glyph.id());
                                }
                            }
                        }
                    }
                    if !wrap {
                        break_point = (content.len() + 1, 0);
                    }
                    let baseline_position_y = position_y + (block_scale * v_metrics.ascent).ceil();
                    let mut caret_position_x = position_x + left_margin;
                    let glyph_count_before_line = display.glyphs.len();
                    if display_bullet {
                        display_bullet = false;
                        display.glyphs.push(gfx_glyph::LayoutGlyph {
                            color: COLOR_REGULAR,
                            font_id: 0,
                            glyph: fonts[0]
                                .glyph('•')
                                .scaled(gfx_glyph::Scale::uniform(block_scale))
                                .positioned(gfx_glyph::Point {
                                    x: position_x + 0.5 * INDENT,
                                    y: baseline_position_y,
                                }),
                        });
                    }
                    let mut last_font_id = 0;
                    let mut last_glyph = None;
                    for (span_index, span) in content.iter().enumerate() {
                        match span {
                            Span::LineBreak => break,
                            Span::Text { class, text } => {
                                let text = if span_index == 0 {
                                    &text[start_point..]
                                } else {
                                    &text[..]
                                };
                                let text = if span_index == break_point.0 {
                                    &text[..break_point.1]
                                } else {
                                    text
                                };
                                let color = match class {
                                    SpanClass::Bold
                                    | SpanClass::BoldItalic
                                    | SpanClass::Italic
                                    | SpanClass::Regular => COLOR_REGULAR,
                                    SpanClass::BoldLink
                                    | SpanClass::BoldItalicLink
                                    | SpanClass::ItalicLink
                                    | SpanClass::Link => COLOR_LINK,
                                    SpanClass::Code => COLOR_CODE,
                                };
                                let font_id = match class {
                                    SpanClass::Bold | SpanClass::BoldLink => 1,
                                    SpanClass::BoldItalic | SpanClass::BoldItalicLink => 2,
                                    SpanClass::Code => 4,
                                    SpanClass::Italic | SpanClass::ItalicLink => 3,
                                    SpanClass::Link | SpanClass::Regular => 0,
                                };
                                if font_id != last_font_id {
                                    last_glyph = None;
                                    last_font_id = font_id;
                                }
                                let font = &fonts[font_id];
                                for character in text.chars() {
                                    let glyph = font
                                        .glyph(map_character(character))
                                        .scaled(gfx_glyph::Scale::uniform(block_scale));
                                    if let Some(last_glyph) = last_glyph {
                                        caret_position_x += font.pair_kerning(
                                            gfx_glyph::Scale::uniform(block_scale),
                                            last_glyph,
                                            glyph.id(),
                                        );
                                    }
                                    last_glyph = Some(glyph.id());
                                    let glyph_position_x = caret_position_x;
                                    caret_position_x += glyph.h_metrics().advance_width;
                                    display.glyphs.push(gfx_glyph::LayoutGlyph {
                                        color,
                                        font_id,
                                        glyph: glyph.positioned(gfx_glyph::Point {
                                            x: glyph_position_x,
                                            y: baseline_position_y,
                                        }),
                                    });
                                }
                                if span_index == break_point.0 {
                                    break;
                                }
                            }
                        }
                    }
                    display.lines.push(Line {
                        bounds_y: position_y..baseline_position_y - block_scale * v_metrics.descent,
                        glyphs: glyph_count_before_line..display.glyphs.len(),
                    });
                    position_y +=
                        block_scale * (v_metrics.ascent - v_metrics.descent + v_metrics.line_gap);
                    if !wrap {
                        break;
                    }
                    if break_point.0 == 0 {
                        start_point += break_point.1;
                    } else {
                        content = &content[break_point.0..];
                        start_point = break_point.1;
                    }
                    while let Some(Span::Text { class: _, text }) = content.first() {
                        let trimmed_text = &text[start_point..].trim_left();
                        if !trimmed_text.is_empty() {
                            start_point = text.len() - trimmed_text.len();
                            break;
                        }
                        content = &content[1..];
                        start_point = 0;
                    }
                }
                position_y += PARAGRAPH_SPACING * scale;
            }
            Block::Image { source } => {
                let glyph_count_before_line = display.glyphs.len();
                let mut last_glyph = None;
                let mut caret_position_x = position_x;
                let baseline_position_y =
                    position_y + (FONT_SIZE_REGULAR * scale * v_metrics.ascent).ceil();
                let font = &fonts[0];
                let block_scale = gfx_glyph::Scale::uniform(FONT_SIZE_REGULAR);
                for character in "Image: ".chars().chain(source.chars().map(map_character)) {
                    let glyph = font.glyph(character).scaled(block_scale);
                    if let Some(last_glyph) = last_glyph {
                        caret_position_x += font.pair_kerning(block_scale, last_glyph, glyph.id());
                    }
                    last_glyph = Some(glyph.id());
                    let glyph_position_x = caret_position_x;
                    caret_position_x += glyph.h_metrics().advance_width;
                    display.glyphs.push(gfx_glyph::LayoutGlyph {
                        color: COLOR_IMAGE_PLACEHOLDER,
                        font_id: 0,
                        glyph: glyph.positioned(gfx_glyph::Point {
                            x: glyph_position_x,
                            y: baseline_position_y,
                        }),
                    });
                }
                display.lines.push(Line {
                    bounds_y: position_y
                        ..baseline_position_y - FONT_SIZE_REGULAR * v_metrics.descent,
                    glyphs: glyph_count_before_line..display.glyphs.len(),
                });
                position_y += FONT_SIZE_REGULAR
                    * (v_metrics.ascent - v_metrics.descent + v_metrics.line_gap)
                    + PARAGRAPH_SPACING * scale;
            }
        }
    }
    display
}

fn map_character(character: char) -> char {
    if character == '\n' {
        ' '
    } else {
        character
    }
}
