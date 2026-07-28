//! The shared document model: one span-carrying block tree parsed from
//! markdown with comrak, used by [`validate`](crate::Schema::validate),
//! [`extract`](crate::Schema::extract), and [`edit`](crate::Schema::edit) alike
//! so all three agree on structure and byte positions.

use comrak::nodes::{AstNode, ListType, NodeValue};
use comrak::{parse_document, Arena, Options};

/// A byte range within a string: `start` inclusive, `end` exclusive.
///
/// Spans returned in [`Problem`](crate::Problem)s and used by
/// [`Schema::edit`](crate::Schema::edit) are offsets into the **original
/// document** (frontmatter included), not the body-only slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
}

impl Span {
    /// Shift both ends by `offset` (used to rebase body spans onto the original
    /// document once frontmatter has been stripped).
    pub(crate) fn shifted(self, offset: usize) -> Span {
        Span {
            start: self.start + offset,
            end: self.end + offset,
        }
    }
}

/// A simplified document block, always carrying its source span.
#[derive(Debug, Clone)]
pub(crate) enum Block {
    Section {
        level: u8,
        title: String,
        children: Vec<Block>,
        /// Span of the heading line's title text.
        title_span: Span,
    },
    List {
        ordered: bool,
        has_checkbox: bool,
        items: Vec<Item>,
    },
    Para {
        text: String,
        /// Span of the paragraph's inline extent.
        span: Span,
    },
    /// A block outside the schema vocabulary (table, code fence, blockquote,
    /// thematic break, raw HTML). Retained so `%strict` can flag it.
    Other { span: Span },
}

#[derive(Debug, Clone)]
pub(crate) struct Item {
    /// Full flattened inline text (checkbox prefix **not** removed).
    pub text: String,
    /// Span of the full item text (after the list marker).
    pub span: Span,
    /// `Some` if the item began with a `[ ]`/`[x]` checkbox.
    pub checked: Option<bool>,
    /// Byte length of the checkbox prefix within `text`/`span` (0 if none).
    pub checkbox_len: usize,
    pub children: Vec<Block>,
}

impl Item {
    /// The item text a schema node of the given style captures: checklist nodes
    /// drop the checkbox prefix; other styles keep the full text.
    pub(crate) fn text_for(&self, checklist: bool) -> String {
        if checklist {
            self.text[self.checkbox_len..].trim_start().to_string()
        } else {
            self.text.clone()
        }
    }

    /// The editable span for a schema node of the given style.
    pub(crate) fn span_for(&self, checklist: bool) -> Span {
        if checklist {
            Span {
                start: self.span.start + self.checkbox_len,
                end: self.span.end,
            }
        } else {
            self.span
        }
    }
}

/// Parse a markdown body into the block tree. `ls` is the line-start offset
/// table for the same body (see [`line_starts`]).
pub(crate) fn parse_blocks(body: &str) -> Vec<Block> {
    let ls = line_starts(body);
    let arena = Arena::new();
    let root = parse_document(&arena, body, &Options::default());
    build_blocks(root.children(), &ls, 0)
}

/// Maximum list-nesting depth, to bound recursion on adversarial input; deeper
/// nesting is truncated (so it fails to match rather than crashing).
const MAX_DEPTH: usize = 128;

/// One flat block before heading-nesting is reconstructed.
enum Flat {
    Heading(u8, String, Span),
    Block(Block),
}

fn build_blocks<'a>(
    nodes: impl Iterator<Item = &'a AstNode<'a>>,
    ls: &[usize],
    depth: usize,
) -> Vec<Block> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    let flats: Vec<Flat> = nodes.filter_map(|n| flat_of(n, ls, depth)).collect();
    let mut pos = 0;
    nest(&flats, &mut pos, 0)
}

fn flat_of<'a>(node: &'a AstNode<'a>, ls: &[usize], depth: usize) -> Option<Flat> {
    let sp = node_span(node, ls);
    match &node.data.borrow().value {
        NodeValue::Heading(h) => Some(Flat::Heading(
            h.level,
            text_of(node),
            first_inline_span(node, ls).unwrap_or(sp),
        )),
        NodeValue::List(nl) => Some(Flat::Block(doc_list(node, nl.list_type, ls, depth))),
        NodeValue::Paragraph => {
            let span = first_inline_span(node, ls).unwrap_or(sp);
            Some(Flat::Block(Block::Para {
                text: text_of(node),
                span,
            }))
        }
        // Everything else is opaque to the schema vocabulary but retained so
        // strict validation can see it.
        _ => Some(Flat::Block(Block::Other { span: sp })),
    }
}

/// Reconstruct heading nesting: a heading owns following blocks and deeper
/// headings until a heading of the same or higher rank.
fn nest(flats: &[Flat], pos: &mut usize, parent_level: usize) -> Vec<Block> {
    let mut out = Vec::new();
    while *pos < flats.len() {
        match &flats[*pos] {
            Flat::Heading(level, title, title_span) => {
                if (*level as usize) <= parent_level {
                    break;
                }
                let (level, title, title_span) = (*level, title.clone(), *title_span);
                *pos += 1;
                let children = nest(flats, pos, level as usize);
                out.push(Block::Section {
                    level,
                    title,
                    children,
                    title_span,
                });
            }
            Flat::Block(_) => {
                if let Flat::Block(b) = &flats[*pos] {
                    out.push(b.clone());
                }
                *pos += 1;
            }
        }
    }
    out
}

fn doc_list<'a>(list: &'a AstNode<'a>, list_type: ListType, ls: &[usize], depth: usize) -> Block {
    let mut items = Vec::new();
    let mut has_checkbox = false;
    for item in list.children() {
        let mut para_span: Option<Span> = None;
        let mut para_text = String::new();
        let mut child_nodes = Vec::new();
        for ch in item.children() {
            match &ch.data.borrow().value {
                NodeValue::Paragraph if para_span.is_none() => {
                    para_text = text_of(ch);
                    para_span = first_inline_span(ch, ls);
                }
                NodeValue::List(_) => child_nodes.push(ch),
                _ => {}
            }
        }
        // An item with no paragraph (an empty `- `) has no inline span; point
        // it at the item's content column so editing inserts after the marker
        // rather than at byte 0.
        let span = para_span.unwrap_or_else(|| item_content_span(item, ls));
        let (checked, checkbox_len) = checkbox_prefix(&para_text);
        if checked.is_some() {
            has_checkbox = true;
        }
        items.push(Item {
            text: para_text,
            span,
            checked,
            checkbox_len,
            children: build_blocks(child_nodes.into_iter(), ls, depth + 1),
        });
    }
    Block::List {
        ordered: matches!(list_type, ListType::Ordered),
        has_checkbox,
        items,
    }
}

/// A zero-width span at a list item's content column (after its marker), for an
/// item with no inline content of its own.
fn item_content_span<'a>(item: &'a AstNode<'a>, ls: &[usize]) -> Span {
    let data = item.data.borrow();
    let sp = data.sourcepos;
    let padding = match &data.value {
        NodeValue::Item(nl) => nl.padding,
        _ => 0,
    };
    let pos = offset(sp.start.line, sp.start.column + padding, ls);
    Span {
        start: pos,
        end: pos,
    }
}

/// Byte length of a leading `[ ]`/`[x]`/`[X]` checkbox prefix and its state.
fn checkbox_prefix(text: &str) -> (Option<bool>, usize) {
    for (marker, checked) in [("[ ] ", false), ("[x] ", true), ("[X] ", true)] {
        if text.starts_with(marker) {
            return (Some(checked), marker.len());
        }
    }
    for (marker, checked) in [("[ ]", false), ("[x]", true), ("[X]", true)] {
        if text == marker {
            return (Some(checked), marker.len());
        }
    }
    (None, 0)
}

/// Concatenate the inline text of a node (headings, paragraphs, item lines),
/// preserving raw code-span and inline-HTML literals.
pub(crate) fn text_of<'a>(node: &'a AstNode<'a>) -> String {
    let mut s = String::new();
    collect_text(node, &mut s);
    s.trim().to_string()
}

fn collect_text<'a>(node: &'a AstNode<'a>, out: &mut String) {
    for c in node.children() {
        match &c.data.borrow().value {
            NodeValue::Text(t) => out.push_str(t),
            NodeValue::Code(code) => out.push_str(&code.literal),
            // Comrak parses HTML-like text (`<uuid>`, `<ctrl>`) as raw inline
            // HTML; keep the source text or angle-bracket content is dropped.
            NodeValue::HtmlInline(html) => out.push_str(html),
            NodeValue::SoftBreak | NodeValue::LineBreak => out.push(' '),
            _ => collect_text(c, out),
        }
    }
}

/// The span covering a node, from its sourcepos.
fn node_span<'a>(node: &'a AstNode<'a>, ls: &[usize]) -> Span {
    let sp = node.data.borrow().sourcepos;
    Span {
        start: offset(sp.start.line, sp.start.column, ls),
        // comrak columns are 1-based byte columns, end-inclusive: the byte
        // after the last character is line-start + end.column.
        end: line_start(sp.end.line, ls) + sp.end.column,
    }
}

/// The span of the first inline descendant to the last — i.e. the text extent
/// of a paragraph or heading, excluding the marker. comrak reports the
/// paragraph/heading node's own sourcepos as this extent already.
fn first_inline_span<'a>(node: &'a AstNode<'a>, ls: &[usize]) -> Option<Span> {
    // The paragraph/heading sourcepos begins at the first inline column and
    // ends at the last, which is exactly the editable text extent.
    let sp = node.data.borrow().sourcepos;
    if sp.start.line == 0 {
        return None;
    }
    Some(Span {
        start: offset(sp.start.line, sp.start.column, ls),
        end: line_start(sp.end.line, ls) + sp.end.column,
    })
}

fn offset(line: usize, col: usize, ls: &[usize]) -> usize {
    line_start(line, ls) + col.saturating_sub(1)
}

fn line_start(line: usize, ls: &[usize]) -> usize {
    ls.get(line.saturating_sub(1)).copied().unwrap_or(0)
}

/// Byte offsets of the start of each line in `s` (index 0 = line 1).
pub(crate) fn line_starts(s: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, &b) in s.as_bytes().iter().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

// ---- frontmatter fences --------------------------------------------------

/// Strip a leading `--- … ---` frontmatter block, returning the body slice and
/// its byte offset within `md`. Handles LF and CRLF; the closing `---` must be
/// on its own line. If there is no well-formed block, returns `(md, 0)`.
pub(crate) fn strip_frontmatter(md: &str) -> (&str, usize) {
    match frontmatter_bounds(md) {
        Some((_, body_start)) => (&md[body_start..], body_start),
        None => (md, 0),
    }
}

/// The inner text of a leading frontmatter block (between the fences), if any.
pub(crate) fn frontmatter_block(md: &str) -> Option<&str> {
    let (inner, _) = frontmatter_bounds(md)?;
    Some(inner)
}

/// Returns `(inner_block, body_start_offset)` for a well-formed leading
/// frontmatter block.
fn frontmatter_bounds(md: &str) -> Option<(&str, usize)> {
    let first_nl = md.find('\n')?;
    let first = md[..first_nl].trim_end_matches('\r');
    if first != "---" {
        return None;
    }
    let inner_start = first_nl + 1;
    let mut pos = inner_start;
    loop {
        let rest = &md[pos..];
        let (line, next) = match rest.find('\n') {
            Some(n) => (&rest[..n], pos + n + 1),
            None => (rest, md.len()),
        };
        if line.trim_end_matches('\r') == "---" {
            let inner_end = pos.saturating_sub(1).max(inner_start);
            return Some((&md[inner_start..inner_end.min(pos)], next));
        }
        if next >= md.len() && rest.find('\n').is_none() {
            return None; // unterminated
        }
        pos = next;
    }
}
