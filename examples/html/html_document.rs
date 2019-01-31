use html5ever::{local_name, namespace_url, ns};

use html5ever::{
    rcdom::{Handle, NodeData},
    tendril::TendrilSink,
};

#[derive(Debug)]
pub enum Block {
    Flowing {
        class: BlockClass,
        content: Vec<Span>,
    },
    Image {
        source: String,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum BlockClass {
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Heading6,
    ListItem,
    Paragraph,
    Preformatted,
}

#[derive(Debug)]
pub enum Span {
    LineBreak,
    Text { class: SpanClass, text: String },
}

#[derive(Debug)]
pub enum SpanClass {
    Bold,
    BoldItalic,
    BoldItalicLink,
    BoldLink,
    Code,
    Italic,
    ItalicLink,
    Link,
    Regular,
}

struct State {
    blocks: Vec<Block>,
    current_block_class: BlockClass,
    current_spans: Option<Vec<Span>>,
    current_text: Option<String>,
    style: Style,
}

struct Style {
    bold: bool,
    code: bool,
    italic: bool,
    link: bool,
}

pub fn parse(html: &str) -> Vec<Block> {
    let mut state = State {
        blocks: vec![],
        current_block_class: BlockClass::Paragraph,
        current_spans: None,
        current_text: None,
        style: Style {
            bold: false,
            code: false,
            italic: false,
            link: false,
        },
    };
    let document = html5ever::parse_fragment(
        html5ever::rcdom::RcDom::default(),
        html5ever::driver::ParseOpts::default(),
        html5ever::QualName {
            local: local_name!("div"),
            ns: ns!(html),
            prefix: None,
        },
        vec![],
    )
    .one(html)
    .document;
    deep_iter(&document, &mut state);
    flush_block(&mut state);
    state.blocks
}

fn deep_iter(node: &Handle, state: &mut State) {
    for node in node.children.borrow().iter() {
        match node.data {
            NodeData::Element {
                ref attrs,
                ref name,
                ..
            } => {
                if name.ns == ns!(html) {
                    match name.local {
                        local_name!("a") => {
                            if state.style.link
                                || !attrs.borrow().iter().any(|ref attribute| {
                                    attribute.name.ns == ns!()
                                        && attribute.name.local == local_name!("href")
                                })
                            {
                                deep_iter(node, state);
                            } else {
                                flush_span(state);
                                state.style.link = true;
                                deep_iter(node, state);
                                flush_span(state);
                                state.style.link = false;
                            }
                        }
                        local_name!("b") | local_name!("strong") if !state.style.bold => {
                            flush_span(state);
                            state.style.bold = true;
                            deep_iter(node, state);
                            flush_span(state);
                            state.style.bold = false;
                        }
                        local_name!("br") => {
                            flush_span(state);
                            if let Some(ref mut spans) = state.current_spans {
                                spans.push(Span::LineBreak);
                            }
                        }
                        local_name!("code") | local_name!("tt") if !state.style.code => {
                            flush_span(state);
                            state.style.code = true;
                            deep_iter(node, state);
                            flush_span(state);
                            state.style.code = false;
                        }
                        local_name!("div") | local_name!("p") => {
                            flush_block(state);
                            deep_iter(node, state);
                            flush_block(state);
                        }
                        local_name!("em") | local_name!("i") if !state.style.italic => {
                            flush_span(state);
                            state.style.italic = true;
                            deep_iter(node, state);
                            flush_span(state);
                            state.style.italic = false;
                        }
                        local_name!("h1") => {
                            handle_heading(node, state, BlockClass::Heading1);
                        }
                        local_name!("h2") => {
                            handle_heading(node, state, BlockClass::Heading2);
                        }
                        local_name!("h3") => {
                            handle_heading(node, state, BlockClass::Heading3);
                        }
                        local_name!("h4") => {
                            handle_heading(node, state, BlockClass::Heading4);
                        }
                        local_name!("h5") => {
                            handle_heading(node, state, BlockClass::Heading5);
                        }
                        local_name!("h6") => {
                            handle_heading(node, state, BlockClass::Heading6);
                        }
                        local_name!("img") => {
                            for attribute in attrs.borrow().iter() {
                                if attribute.name.ns == ns!()
                                    && attribute.name.local == local_name!("src")
                                {
                                    flush_block(state);
                                    state.blocks.push(Block::Image {
                                        source: attribute.value.to_string(),
                                    });
                                    break;
                                }
                            }
                        }
                        local_name!("li") => {
                            flush_block(state);
                            let previous_block_class = state.current_block_class;
                            state.current_block_class = BlockClass::ListItem;
                            deep_iter(node, state);
                            flush_block(state);
                            state.current_block_class = previous_block_class;
                        }
                        local_name!("pre") => {
                            flush_block(state);
                            let previous_block_class = state.current_block_class;
                            state.current_block_class = BlockClass::Preformatted;
                            state.style.code = true;
                            deep_iter(node, state);
                            flush_block(state);
                            state.current_block_class = previous_block_class;
                            state.style.code = false;
                        }
                        _ => {
                            deep_iter(node, state);
                        }
                    }
                }
            }
            NodeData::Text { ref contents } => match state.current_text {
                None => {
                    state.current_text = Some(contents.borrow().to_string());
                }
                Some(ref mut text) => {
                    text.push_str(&contents.borrow());
                }
            },
            _ => {}
        }
    }
}

fn flush_block(state: &mut State) {
    if let Some(text) = state.current_text.take() {
        match state.current_spans {
            None => {
                let text = text.trim();
                if !text.is_empty() {
                    state.current_spans = Some(vec![Span::Text {
                        class: get_span_class(&state.style),
                        text: text.to_owned(),
                    }]);
                }
            }
            Some(ref mut spans) => {
                let text = text.trim_right();
                if !text.is_empty() {
                    spans.push(Span::Text {
                        class: get_span_class(&state.style),
                        text: text.to_owned(),
                    });
                }
            }
        }
    }
    if let Some(content) = state.current_spans.take() {
        state.blocks.push(Block::Flowing {
            class: state.current_block_class,
            content,
        });
    }
}

fn flush_span(state: &mut State) {
    if let Some(text) = state.current_text.take() {
        match state.current_spans {
            None => {
                let text = text.trim_left();
                if !text.is_empty() {
                    state.current_spans = Some(vec![Span::Text {
                        class: get_span_class(&state.style),
                        text: text.to_owned(),
                    }]);
                }
            }
            Some(ref mut spans) => {
                spans.push(Span::Text {
                    class: get_span_class(&state.style),
                    text,
                });
            }
        }
    }
}

fn get_span_class(style: &Style) -> SpanClass {
    if style.code {
        SpanClass::Code
    } else {
        match (style.bold, style.italic, style.link) {
            (false, false, false) => SpanClass::Regular,
            (false, false, true) => SpanClass::Link,
            (false, true, false) => SpanClass::Italic,
            (false, true, true) => SpanClass::ItalicLink,
            (true, false, false) => SpanClass::Bold,
            (true, false, true) => SpanClass::BoldLink,
            (true, true, false) => SpanClass::BoldItalic,
            (true, true, true) => SpanClass::BoldItalicLink,
        }
    }
}

fn handle_heading(node: &Handle, state: &mut State, block_class: BlockClass) {
    flush_block(state);
    let previous_block_class = state.current_block_class;
    state.current_block_class = block_class;
    state.style.bold = true;
    deep_iter(node, state);
    flush_block(state);
    state.current_block_class = previous_block_class;
    state.style.bold = false;
}
