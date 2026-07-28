//! Parser: DSL source → [`Schema`]. Line-oriented; headings nest by level and
//! lists/prose nest by indentation. See `docs/structure-dsl-spec.md` for the
//! grammar.

use super::error::SchemaError;
use super::schema::*;
use std::collections::HashMap;

/// Maximum body-nesting depth, to bound recursion on adversarial input.
const MAX_DEPTH: usize = 128;

/// Parse a schema from DSL source.
///
/// Phases: leading `%`-directives, an optional `--- … ---` frontmatter block,
/// then the body. Fails with a located [`SchemaError`] on a malformed
/// directive, field, or marker; an invalid regex label/type; a schema line
/// indented under a `>` prose node; or two nodes sharing a capture alias in one
/// scope.
///
/// # Examples
///
/// ```
/// use marklens_core::parse_schema;
///
/// let schema = parse_schema("## @plan Plan\n  - +@cases\n")?;
/// assert!(schema.validate("## Plan\n- one\n- two\n").is_empty());
/// # Ok::<(), marklens_core::SchemaError>(())
/// ```
pub fn parse_schema(src: &str) -> Result<Schema, SchemaError> {
    let lines: Vec<&str> = src.lines().collect();
    let mut opts = SchemaOpts::default();
    let mut i = 0;

    // Phase 1: leading `%`-directives and `<? … ?>` / blank comment lines.
    while i < lines.len() {
        let (content, _) = split_desc(lines[i]);
        let t = content.trim();
        if t.is_empty() {
            i += 1;
        } else if let Some(rest) = t.strip_prefix('%') {
            apply_directive(rest, i + 1, &mut opts)?;
            i += 1;
        } else {
            break;
        }
    }

    // Phase 2: optional frontmatter block.
    let mut frontmatter = Vec::new();
    if i < lines.len() && split_desc(lines[i]).0.trim() == "---" {
        i += 1;
        loop {
            let line = lines.get(i).ok_or_else(|| {
                SchemaError::new(i, 1, "unterminated frontmatter (missing `---`)")
            })?;
            let (content, desc) = split_desc(line);
            if content.trim() == "---" {
                i += 1;
                break;
            }
            if !content.trim().is_empty() {
                let mut f = parse_field(&content, i + 1)?;
                f.desc = desc;
                frontmatter.push(f);
            }
            i += 1;
        }
    }

    // Phase 3: body nodes, flat with indents, then assembled into a tree.
    let mut flat: Vec<Raw> = Vec::new();
    while i < lines.len() {
        let line = lines[i];
        let (content, desc) = split_desc(line);
        let (indent, rest) = measure_indent(&content);
        if rest.trim().is_empty() {
            i += 1;
            continue;
        }
        let node = parse_node(rest, desc, i + 1, indent)?;
        flat.push(Raw {
            indent,
            line: i + 1,
            node,
        });
        i += 1;
    }
    let mut pos = 0;
    let trees = build_tree(&flat, &mut pos, -1, 0)?;

    // Alias uniqueness: frontmatter keys share the top-level object with body
    // captures, so they must not collide either.
    let mut top: HashMap<String, usize> = HashMap::new();
    for f in &frontmatter {
        if top.insert(f.alias.clone(), 0).is_some() {
            return Err(SchemaError::new(
                0,
                1,
                format!("duplicate frontmatter alias `{}`", f.alias),
            ));
        }
    }
    check_unique(&trees, &mut top)?;
    check_adjacent_lists(&trees)?;

    let body = trees.into_iter().map(finalize).collect();
    Ok(Schema {
        opts,
        frontmatter,
        body,
    })
}

// ---- directives ----------------------------------------------------------

fn apply_directive(rest: &str, line: usize, opts: &mut SchemaOpts) -> Result<(), SchemaError> {
    let (k, v) = rest
        .split_once('=')
        .ok_or_else(|| SchemaError::new(line, 1, "directive needs `key = value`"))?;
    let (k, v) = (k.trim(), v.trim());
    match (k, v) {
        ("ordered", "true") => opts.ordered = true,
        ("ordered", "false") => opts.ordered = false,
        ("strict", "true") => opts.strict = true,
        ("strict", "false") => opts.strict = false,
        ("frontmatter", "open") => opts.frontmatter_open = true,
        ("frontmatter", "closed") => opts.frontmatter_open = false,
        _ => {
            return Err(SchemaError::new(
                line,
                1,
                format!("unknown directive `%{rest}`"),
            ))
        }
    }
    Ok(())
}

// ---- frontmatter ---------------------------------------------------------

fn parse_field(content: &str, line: usize) -> Result<FieldSchema, SchemaError> {
    let (left, ty_src) = content
        .split_once(':')
        .ok_or_else(|| SchemaError::new(line, 1, "frontmatter field needs `key: type`"))?;

    // left = key[?] [@alias]
    let left = left.trim();
    let key_end = left
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(left.len());
    if key_end == 0 {
        return Err(SchemaError::new(line, 1, "frontmatter field needs a key"));
    }
    let key = left[..key_end].to_string();
    let mut rest = left[key_end..].trim_start();
    let optional = if let Some(r) = rest.strip_prefix('?') {
        rest = r.trim_start();
        true
    } else {
        false
    };
    let alias = if let Some(r) = rest.strip_prefix('@') {
        let a = r.trim();
        if a.is_empty() {
            return Err(SchemaError::new(
                line,
                1,
                "empty `@alias` on frontmatter key",
            ));
        }
        a.to_string()
    } else {
        key.clone()
    };

    let ty = parse_field_type(ty_src.trim(), line)?;
    Ok(FieldSchema {
        key,
        alias,
        optional,
        ty,
        desc: None,
    })
}

fn parse_field_type(s: &str, line: usize) -> Result<FieldType, SchemaError> {
    match s {
        "string" => return Ok(FieldType::Str),
        "int" => return Ok(FieldType::Int),
        "bool" => return Ok(FieldType::Bool),
        "date" => return Ok(FieldType::Date),
        _ => {}
    }
    if let Some(inner) = s.strip_prefix("enum(").and_then(|s| s.strip_suffix(')')) {
        let vals = inner
            .split(',')
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect();
        return Ok(FieldType::Enum(vals));
    }
    if let Some(inner) = s.strip_prefix('[') {
        // `[T]` — the matching `]` must close the whole type.
        let close = inner
            .rfind(']')
            .ok_or_else(|| SchemaError::new(line, 1, "list type missing `]`"))?;
        let trailing = inner[close + 1..].trim();
        if !trailing.is_empty() {
            return Err(SchemaError::new(
                line,
                1,
                format!("unexpected `{trailing}` after list type"),
            ));
        }
        let elem = parse_field_type(inner[..close].trim(), line)?;
        if matches!(elem, FieldType::List(_)) {
            return Err(SchemaError::new(
                line,
                1,
                "nested list types (`[[T]]`) are not supported",
            ));
        }
        return Ok(FieldType::List(Box::new(elem)));
    }
    if s.starts_with('/') {
        let (pat, _) = parse_regex(s);
        let re = Regexp::new(pat)
            .map_err(|e| SchemaError::new(line, 1, format!("invalid regex type: {e}")))?;
        return Ok(FieldType::Regex(re));
    }
    Err(SchemaError::new(line, 1, format!("unknown type `{s}`")))
}

// ---- body nodes ----------------------------------------------------------

fn parse_node(
    content: &str,
    desc: Option<String>,
    line: usize,
    indent: usize,
) -> Result<Node, SchemaError> {
    let col = indent + 1;
    let (kind, head_src) = detect_marker(content)
        .ok_or_else(|| SchemaError::new(line, col, format!("unrecognized marker: `{content}`")))?;

    let (card, rest) = parse_card(head_src);
    let (name, rest) = parse_name(rest);
    let label = parse_label(rest.trim(), line, col)?;
    let head = Head { name, card, desc };

    Ok(match kind {
        MarkerKind::Heading(level) => Node::Heading {
            level,
            title: label.unwrap_or_else(|| Match::Regex(any_regex())),
            head,
            children: Vec::new(),
        },
        MarkerKind::Prose => Node::Prose { text: label, head },
        MarkerKind::Bullet => list(ListStyle::Bullet, label, head),
        MarkerKind::Ordered => list(ListStyle::Ordered, label, head),
        MarkerKind::Checklist => list(ListStyle::Checklist, label, head),
    })
}

fn any_regex() -> Regexp {
    Regexp::new(".*").expect("`.*` is a valid regex")
}

fn list(style: ListStyle, item: Option<Match>, head: Head) -> Node {
    Node::List {
        style,
        item,
        head,
        children: Vec::new(),
    }
}

enum MarkerKind {
    Heading(u8),
    Bullet,
    Ordered,
    Checklist,
    Prose,
}

/// Detect the leading marker and return the kind + the remaining "head" text
/// (everything after the marker and its following spaces).
fn detect_marker(s: &str) -> Option<(MarkerKind, &str)> {
    if let Some(rest) = s.strip_prefix("- [ ]") {
        return Some((MarkerKind::Checklist, rest.trim_start()));
    }
    if s.starts_with('#') {
        let level = s.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&level) {
            let after = &s[level..];
            // require a space (or end) after the hashes
            if after.is_empty() || after.starts_with(' ') {
                return Some((MarkerKind::Heading(level as u8), after.trim_start()));
            }
        }
        return None;
    }
    if let Some(rest) = s.strip_prefix("- ") {
        return Some((MarkerKind::Bullet, rest.trim_start()));
    }
    if s == "-" {
        return Some((MarkerKind::Bullet, ""));
    }
    // ordered: digits then '.'
    let digits = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && s[digits..].starts_with('.') {
        return Some((MarkerKind::Ordered, s[digits + 1..].trim_start()));
    }
    if let Some(rest) = s.strip_prefix('>') {
        return Some((MarkerKind::Prose, rest.trim_start()));
    }
    None
}

fn parse_card(s: &str) -> (Card, &str) {
    match s.as_bytes().first() {
        Some(b'+') => (Card::Plus, &s[1..]),
        Some(b'*') => (Card::Star, &s[1..]),
        Some(b'?') => (Card::Optional, &s[1..]),
        Some(b'{') => parse_range(s),
        _ => (Card::Required, s),
    }
}

fn parse_range(s: &str) -> (Card, &str) {
    let Some(close) = s.find('}') else {
        return (Card::Required, s);
    };
    let body = &s[1..close];
    let (min_s, max_s) = match body.split_once(',') {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => (body.trim(), None),
    };
    let Ok(min) = min_s.parse::<u32>() else {
        return (Card::Required, s);
    };
    let max = match max_s {
        None => Some(min),
        Some("") => None,
        Some(m) => match m.parse::<u32>() {
            Ok(v) => Some(v),
            Err(_) => return (Card::Required, s),
        },
    };
    (Card::Range(min, max), &s[close + 1..])
}

fn parse_name(s: &str) -> (Option<String>, &str) {
    let Some(rest) = s.strip_prefix('@') else {
        return (None, s);
    };
    let end = rest
        .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
        .unwrap_or(rest.len());
    if end == 0 {
        return (None, s);
    }
    (Some(rest[..end].to_string()), &rest[end..])
}

fn parse_label(s: &str, line: usize, col: usize) -> Result<Option<Match>, SchemaError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    if s.starts_with('/') {
        let (pat, _) = parse_regex(s);
        let re = Regexp::new(pat)
            .map_err(|e| SchemaError::new(line, col, format!("invalid regex label: {e}")))?;
        return Ok(Some(Match::Regex(re)));
    }
    if let Some(rest) = s.strip_prefix('"') {
        let lit = match rest.rsplit_once('"') {
            Some((inner, _)) => inner,
            None => rest,
        };
        return Ok(Some(Match::Literal(unescape(lit))));
    }
    Ok(Some(Match::Literal(unescape(s))))
}

/// Parse `/pattern/flags` → a regex pattern string (flags folded into `(?…)`).
/// Returns `(pattern, rest_after)`.
fn parse_regex(s: &str) -> (String, &str) {
    debug_assert!(s.starts_with('/'));
    let body = &s[1..];
    // find the closing unescaped '/'
    let mut end = None;
    let bytes = body.as_bytes();
    let mut k = 0;
    while k < bytes.len() {
        if bytes[k] == b'\\' {
            k += 2;
            continue;
        }
        if bytes[k] == b'/' {
            end = Some(k);
            break;
        }
        k += 1;
    }
    let Some(end) = end else {
        return (body.to_string(), "");
    };
    let pat = &body[..end];
    let after = &body[end + 1..];
    let flag_len = after
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .count();
    let flags = &after[..flag_len];
    let pattern = if flags.is_empty() {
        pat.to_string()
    } else {
        format!("(?{flags}){pat}")
    };
    (pattern, &after[flag_len..])
}

/// Resolve backslash escapes: `\x` → `x`.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ---- description / comment splitting -------------------------------------

/// Split a line into its content and an optional `<? … ?>` description. The
/// first unescaped `<?` opens the description; the next unescaped `?>` closes
/// it (or end-of-line if unclosed). Content before `<?` and after `?>` is
/// joined back together; `\?>` and `\\` inside the description are unescaped.
fn split_desc(line: &str) -> (String, Option<String>) {
    let bytes = line.as_bytes();
    let mut k = 0;
    let open = loop {
        if k + 1 >= bytes.len() {
            return (line.to_string(), None);
        }
        if bytes[k] == b'\\' {
            k += 2;
            continue;
        }
        if bytes[k] == b'<' && bytes[k + 1] == b'?' {
            break k;
        }
        k += 1;
    };
    let before = &line[..open];
    let after_open = &line[open + 2..];
    // find unescaped `?>`
    let ab = after_open.as_bytes();
    let mut j = 0;
    let mut close = None;
    while j + 1 < ab.len() {
        if ab[j] == b'\\' {
            j += 2;
            continue;
        }
        if ab[j] == b'?' && ab[j + 1] == b'>' {
            close = Some(j);
            break;
        }
        j += 1;
    }
    let (desc_raw, tail) = match close {
        Some(c) => (&after_open[..c], &after_open[c + 2..]),
        None => (after_open, ""),
    };
    let desc = desc_raw.replace("\\?>", "?>").replace("\\\\", "\\");
    let content = format!("{before}{tail}");
    (content, Some(desc.trim().to_string()))
}

// ---- indentation ---------------------------------------------------------

/// Measure the leading indentation of a line as a visual column count (tabs
/// advance to the next multiple of 4) and return `(indent, rest)`.
fn measure_indent(line: &str) -> (usize, &str) {
    let mut col = 0;
    for (i, c) in line.char_indices() {
        match c {
            ' ' => col += 1,
            '\t' => col += 4 - (col % 4),
            _ => return (col, &line[i..]),
        }
    }
    (col, "")
}

// ---- tree assembly -------------------------------------------------------

/// A body node before its children are attached, with source metadata.
struct Raw {
    indent: usize,
    line: usize,
    node: Node,
}

/// A node plus the still-unattached children collected under it.
struct RawTree {
    line: usize,
    node: Node,
    children: Vec<RawTree>,
}

/// Assemble the flat `Raw` list into a tree. Headings nest by level (a child
/// heading's level must exceed its parent's); lists and prose nest by
/// indentation. Rejects any node placed under a prose (`>`) node.
fn build_tree(
    flat: &[Raw],
    pos: &mut usize,
    parent: isize,
    depth: usize,
) -> Result<Vec<RawTree>, SchemaError> {
    // `parent` encodes the current container: for a heading scope it is the
    // heading level (0..=6); for an indentation scope it is `100 + indent` so
    // the two never collide. The root scope is -1.
    if depth > MAX_DEPTH {
        let line = flat.get(*pos).map(|r| r.line).unwrap_or(0);
        return Err(SchemaError::new(line, 1, "schema nesting is too deep"));
    }
    let mut out: Vec<RawTree> = Vec::new();
    while *pos < flat.len() {
        let raw = &flat[*pos];
        let level = heading_level(&raw.node);
        let contained = match (parent, level) {
            (-1, _) => true,
            // inside a heading scope of level L:
            (p, Some(l)) if p <= 6 => (l as isize) > p, // deeper heading nests
            (p, None) if p <= 6 => true,                // any content nests
            // inside an indentation scope: nest iff strictly more indented
            (p, _) => (raw.indent as isize) > (p - 100),
        };
        if !contained {
            break;
        }
        *pos += 1;
        let is_prose = matches!(raw.node, Node::Prose { .. });
        let child_scope = match level {
            Some(l) => l as isize,
            None => 100 + raw.indent as isize,
        };
        let children = build_tree(flat, pos, child_scope, depth + 1)?;
        if is_prose && !children.is_empty() {
            return Err(SchemaError::new(
                children[0].line,
                1,
                "a `>` prose node cannot have child nodes",
            ));
        }
        out.push(RawTree {
            line: raw.line,
            node: raw.node.clone(),
            children,
        });
    }
    Ok(out)
}

fn heading_level(node: &Node) -> Option<u8> {
    match node {
        Node::Heading { level, .. } => Some(*level),
        _ => None,
    }
}

/// Attach children and drop the source metadata.
fn finalize(mut t: RawTree) -> Node {
    let kids = t.children.into_iter().map(finalize).collect();
    t.node.set_children(kids);
    t.node
}

/// Reject two adjacent same-marker list nodes in one scope: markdown would
/// merge them into a single list, so no document could satisfy both. Lists of
/// different markers (`-` vs `1.`), or lists separated by a heading or prose,
/// are fine.
fn check_adjacent_lists(siblings: &[RawTree]) -> Result<(), SchemaError> {
    let mut prev: Option<u8> = None;
    for t in siblings {
        let group = match &t.node {
            Node::List { style, .. } => Some(list_marker_group(*style)),
            _ => None,
        };
        if let (Some(p), Some(g)) = (prev, group) {
            if p == g {
                return Err(SchemaError::new(
                    t.line,
                    1,
                    "adjacent lists of the same marker cannot be told apart in a document; \
                     separate them with a heading or prose",
                ));
            }
        }
        prev = group; // a heading or prose (None) breaks the adjacency
        check_adjacent_lists(&t.children)?;
    }
    Ok(())
}

/// The marker family a list renders with: `-` bullets and checklists collide;
/// ordered lists are their own family.
fn list_marker_group(style: ListStyle) -> u8 {
    match style {
        ListStyle::Bullet | ListStyle::Checklist => 0,
        ListStyle::Ordered => 1,
    }
}

/// Reject two nodes that capture under the same alias in one scope (they would
/// overwrite each other in `extract`). `seen` seeds the top-level scope with
/// frontmatter aliases.
fn check_unique(
    siblings: &[RawTree],
    seen: &mut HashMap<String, usize>,
) -> Result<(), SchemaError> {
    for t in siblings {
        if let Some(alias) = t.node.capture_alias() {
            if seen.insert(alias.clone(), t.line).is_some() {
                return Err(SchemaError::new(
                    t.line,
                    1,
                    format!("duplicate capture alias `{alias}` in this scope"),
                ));
            }
        }
        let mut child_scope = HashMap::new();
        check_unique(&t.children, &mut child_scope)?;
    }
    Ok(())
}
