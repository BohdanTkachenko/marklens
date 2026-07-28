//! Rendering: `scaffold` (schema → starter document) and `render` (data →
//! document), plus the inline escaper shared with [`edit`](Schema::edit).

use super::error::RenderError;
use super::frontmatter;
use super::schema::*;
use serde_json::Value;
use std::fmt::Write;

impl Schema {
    /// Emit a conforming starter document: required frontmatter keys with typed
    /// placeholders, headings, and placeholder items for required lists.
    /// Optional nodes (`?`/`*`/min-0) are omitted.
    ///
    /// The output validates against the schema for the common cases; a node
    /// constrained by a regex label may need hand-editing, since no placeholder
    /// can satisfy an arbitrary pattern.
    pub fn scaffold(&self) -> String {
        let mut out = String::new();

        if !self.frontmatter.is_empty() {
            out.push_str("---\n");
            for f in &self.frontmatter {
                if f.optional {
                    continue;
                }
                let _ = writeln!(out, "{}: {}", f.key, frontmatter::placeholder(&f.ty));
            }
            out.push_str("---\n\n");
        }

        scaffold_nodes(&self.body, &mut out, 0, false);
        normalize_blocks(&mut out);
        out
    }

    /// Render a conforming markdown document from `data` — the inverse of
    /// [`Schema::extract`]. `data` is a JSON object keyed by the capture aliases
    /// declared in the schema.
    ///
    /// Returns [`RenderError::MissingField`] if a required field is absent,
    /// [`RenderError::WrongType`] if a value has an incompatible JSON type, or
    /// [`RenderError::InvalidFrontmatter`] if a frontmatter value violates its
    /// declared type.
    ///
    /// # Examples
    ///
    /// ```
    /// use marklens_core::parse_schema;
    /// use serde_json::json;
    ///
    /// let schema = parse_schema("## @plan Plan\n  - +@cases\n")?;
    /// let md = schema.render(&json!({ "plan": { "cases": ["a", "b"] } }))?;
    /// assert_eq!(md, "## Plan\n\n- a\n- b\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn render(&self, data: &Value) -> Result<String, RenderError> {
        let mut out = frontmatter::render_frontmatter(&self.frontmatter, data)?;
        render_nodes(&self.body, data, &mut out, 0)?;
        normalize_blocks(&mut out);
        Ok(out)
    }
}

/// Whether `line` is a top-level ATX heading. Captured text is escaped before
/// it reaches the output, so a value that merely starts with `#` cannot match.
fn is_heading_line(line: &str) -> bool {
    let hashes = line.len() - line.trim_start_matches('#').len();
    (1..=6).contains(&hashes) && line[hashes..].starts_with(' ')
}

/// Normalize block separation so the output is well-formed prose markdown: one
/// blank line around every heading, never two blank lines in a row, and exactly
/// one trailing newline.
///
/// The emitter appends its separators locally, which leaves runs of two and
/// three blank lines between nested sections and no blank line after a heading
/// — legal markdown, but the kind a linter rejects. Only column-0 headings and
/// runs of blank lines are rewritten here; indented lines are passed through
/// untouched, since list nesting is indentation-sensitive.
fn normalize_blocks(out: &mut String) {
    let mut lines: Vec<&str> = Vec::new();
    for line in out.lines() {
        if line.trim().is_empty() {
            // Collapse a run of blanks, and drop any leading blank.
            if lines.last().is_none_or(|l: &&str| l.trim().is_empty()) {
                continue;
            }
            lines.push("");
        } else {
            let after_heading = lines.last().is_some_and(|l| is_heading_line(l));
            let before_heading =
                is_heading_line(line) && lines.last().is_some_and(|l: &&str| !l.trim().is_empty());
            if after_heading || before_heading {
                lines.push("");
            }
            lines.push(line);
        }
    }
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    *out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
}

// ---- render helpers ---------------------------------------------------------

fn render_nodes(
    nodes: &[Node],
    scope: &Value,
    out: &mut String,
    list_indent: usize,
) -> Result<(), RenderError> {
    for node in nodes {
        let Some(key) = node.capture_alias() else {
            continue;
        };
        let value = scope.get(&key).unwrap_or(&Value::Null);
        render_node(node, &key, value, out, list_indent)?;
    }
    Ok(())
}

fn render_node(
    node: &Node,
    key: &str,
    value: &Value,
    out: &mut String,
    list_indent: usize,
) -> Result<(), RenderError> {
    match node {
        Node::Heading {
            level,
            title,
            head,
            children,
        } => {
            if value.is_null() {
                if !omitted(head.card) {
                    return Err(RenderError::MissingField(key.to_string()));
                }
                return Ok(());
            }
            let hashes = "#".repeat(*level as usize);
            if is_repeated(head.card) {
                let arr = value.as_array().ok_or_else(|| RenderError::WrongType {
                    field: key.to_string(),
                    expected: "array",
                })?;
                for item in arr {
                    let t = heading_title(title, item, key)?;
                    let _ = writeln!(out, "{hashes} {t}");
                    render_nodes(children, item, out, 0)?;
                    out.push('\n');
                }
            } else {
                let t = heading_title(title, value, key)?;
                let _ = writeln!(out, "{hashes} {t}");
                render_nodes(children, value, out, 0)?;
                out.push('\n');
            }
        }

        Node::List {
            style,
            item: label,
            head,
            children,
        } => {
            if value.is_null() {
                if !omitted(head.card) {
                    return Err(RenderError::MissingField(key.to_string()));
                }
                return Ok(());
            }
            let single = matches!(head.card, Card::Required | Card::Optional);
            let scalar = single && children.is_empty() && *style != ListStyle::Checklist;
            let label_prefix = match label {
                Some(Match::Literal(l)) => format!("{l} "),
                _ => String::new(),
            };
            let content_indent = list_indent + list_content_offset(*style);

            if scalar {
                let text = value.as_str().ok_or_else(|| RenderError::WrongType {
                    field: key.to_string(),
                    expected: "string",
                })?;
                emit_item(
                    out,
                    list_indent,
                    *style,
                    None,
                    &label_prefix,
                    &esc_inline(text),
                );
            } else {
                let arr = as_items(value, key)?;
                for item_val in &arr {
                    let (text, checked, child_scope) = item_fields(item_val, key)?;
                    let text = esc_inline(&text);
                    emit_item(out, list_indent, *style, checked, &label_prefix, &text);
                    if !children.is_empty() {
                        render_nodes(children, child_scope, out, content_indent)?;
                    }
                }
            }
            // Separate a top-level list from whatever follows so it is not
            // swallowed by lazy continuation or merged with an adjacent list.
            if list_indent == 0 {
                out.push('\n');
            }
        }

        Node::Prose { head, .. } => {
            if value.is_null() {
                if !omitted(head.card) {
                    return Err(RenderError::MissingField(key.to_string()));
                }
                return Ok(());
            }
            let text = value.as_str().ok_or_else(|| RenderError::WrongType {
                field: key.to_string(),
                expected: "string",
            })?;
            let _ = writeln!(out, "{}", esc_inline(text));
            out.push('\n');
        }
    }
    Ok(())
}

/// The number of columns an item's content sits at after its marker, which is
/// where child blocks must be indented to nest.
fn list_content_offset(style: ListStyle) -> usize {
    match style {
        ListStyle::Ordered => 3,                       // "1. "
        ListStyle::Bullet | ListStyle::Checklist => 2, // "- "
    }
}

/// Coerce a value into a list of item values: an array as-is, or a lone string
/// as a single item.
fn as_items(value: &Value, key: &str) -> Result<Vec<Value>, RenderError> {
    match value {
        Value::Array(arr) => Ok(arr.clone()),
        Value::String(_) => Ok(vec![value.clone()]),
        _ => Err(RenderError::WrongType {
            field: key.to_string(),
            expected: "array or string",
        }),
    }
}

/// Pull `(text, checked, child_scope)` out of an item value, which is either a
/// bare string or a `{ "text", "checked", …children }` object. Errors on a
/// non-string value or a non-string `"text"` field rather than emitting `""`.
fn item_fields<'a>(
    item: &'a Value,
    key: &str,
) -> Result<(String, Option<bool>, &'a Value), RenderError> {
    let wrong = || RenderError::WrongType {
        field: key.to_string(),
        expected: "string",
    };
    match item {
        Value::String(s) => Ok((s.clone(), None, item)),
        Value::Object(_) => {
            let text = match item.get("text") {
                Some(Value::String(s)) => s.clone(),
                None => String::new(),
                Some(_) => return Err(wrong()),
            };
            let checked = item.get("checked").and_then(Value::as_bool);
            Ok((text, checked, item))
        }
        _ => Err(wrong()),
    }
}

fn emit_item(
    out: &mut String,
    indent: usize,
    style: ListStyle,
    checked: Option<bool>,
    label_prefix: &str,
    text: &str,
) {
    let pad = " ".repeat(indent);
    let marker = match style {
        ListStyle::Bullet => "- ",
        ListStyle::Ordered => "1. ",
        ListStyle::Checklist if checked.unwrap_or(false) => "- [x] ",
        ListStyle::Checklist => "- [ ] ",
    };
    let _ = writeln!(out, "{pad}{marker}{label_prefix}{text}");
}

/// Escape captured text so re-parsing it (extract) yields the identical string
/// — the codec must satisfy `extract(render(x)) == x`. Backslash-escapes the
/// inline-active markdown characters, plus block markers when they lead the
/// value (where the text sits at the start of a list item or paragraph).
pub(crate) fn esc_inline(s: &str) -> String {
    // A leading ordered-list marker: digits then '.' or ')'.
    let lead_ord = {
        let d = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        if d > 0 && matches!(s.as_bytes().get(d), Some(b'.') | Some(b')')) {
            Some(d)
        } else {
            None
        }
    };
    let mut out = String::with_capacity(s.len() + 8);
    for (i, c) in s.char_indices() {
        let start = i == 0;
        let escape = matches!(
            c,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '&' | '~' | '|' | '!'
        ) || (start && matches!(c, '#' | '-' | '+' | '='))
            || Some(i) == lead_ord;
        if escape {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The literal title to use for a heading: the schema literal (trusted), or the
/// escaped `value["title"]` for regex headings (so a title with markdown
/// metacharacters round-trips). Errors if a regex heading's `title` is present
/// but not a string.
fn heading_title(title: &Match, value: &Value, key: &str) -> Result<String, RenderError> {
    match title {
        Match::Literal(s) => Ok(s.clone()),
        Match::Regex(_) => match value.get("title") {
            Some(Value::String(s)) => Ok(esc_inline(s)),
            None | Some(Value::Null) => Ok(String::new()),
            Some(_) => Err(RenderError::WrongType {
                field: format!("{key}.title"),
                expected: "string",
            }),
        },
    }
}

/// A node is omitted from scaffold/render when it isn't required to be present.
fn omitted(card: Card) -> bool {
    matches!(card, Card::Optional | Card::Star | Card::Range(0, _))
}

/// Cardinality that produces multiple values (array in extracted data).
fn is_repeated(card: Card) -> bool {
    matches!(card, Card::Plus | Card::Star | Card::Range(..))
}

// ---- scaffold helpers -------------------------------------------------------

/// The number of placeholder items to emit for a required list.
fn scaffold_count(card: Card) -> usize {
    match card {
        Card::Range(m, _) => (m.max(1)) as usize,
        _ => 1,
    }
}

/// `force_text` requests a non-empty placeholder for items in this scope, used
/// for the children of a checklist item (empty items do not nest under a
/// checkbox reliably).
fn scaffold_nodes(nodes: &[Node], out: &mut String, indent: usize, force_text: bool) {
    for node in nodes {
        match node {
            Node::Heading {
                level,
                title,
                head,
                children,
            } => {
                if omitted(head.card) {
                    continue;
                }
                let hashes = "#".repeat(*level as usize);
                let _ = writeln!(out, "{hashes} {}", literal_of(title));
                scaffold_nodes(children, out, 0, false);
                out.push('\n');
            }
            Node::List {
                style,
                item,
                head,
                children,
            } => {
                if omitted(head.card) {
                    continue;
                }
                let label = item.as_ref().map(literal_of).unwrap_or_default();
                let label_prefix = if label.is_empty() {
                    String::new()
                } else {
                    format!("{label} ")
                };
                // An item needs non-empty text when a child block will be
                // emitted under it (so the nesting anchors) or when its own
                // parent item is non-empty — an empty item only nests under an
                // empty one, so once an item is filled its children must be too.
                let emits_children = children.iter().any(|c| !omitted(c.card()));
                let text_nonempty = emits_children || force_text;
                let text = if text_nonempty { "..." } else { "" };
                let child_force = text_nonempty;
                for _ in 0..scaffold_count(head.card) {
                    emit_item(out, indent, *style, Some(false), &label_prefix, text);
                    scaffold_nodes(
                        children,
                        out,
                        indent + list_content_offset(*style),
                        child_force,
                    );
                }
                if indent == 0 {
                    out.push('\n');
                }
            }
            Node::Prose { head, text } => {
                if omitted(head.card) {
                    continue;
                }
                let lbl = text.as_ref().map(literal_of).unwrap_or_default();
                let line = if lbl.is_empty() { "TODO" } else { &lbl };
                let _ = writeln!(out, "{line}");
                out.push('\n');
            }
        }
    }
}

/// The literal text of a match for scaffolding; a regex contributes no text.
fn literal_of(m: &Match) -> String {
    match m {
        Match::Literal(s) => s.clone(),
        Match::Regex(_) => String::new(),
    }
}
