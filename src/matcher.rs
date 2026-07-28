//! The unified body matcher: walk schema nodes against the [`Block`] tree with
//! a per-scope cursor, producing both validation [`Problem`]s and a binding
//! tree that records which document blocks each schema node claimed. `extract`
//! ([`to_json`]) and `edit` ([`find_span`]) both consume the same bindings, so
//! they can never disagree with `validate`.

use crate::doc::{Block, Span};
use crate::error::Problem;
use crate::schema::*;
use serde_json::{Map, Value};

/// A schema node paired with the document blocks it matched.
pub(crate) struct Bound<'a> {
    node: &'a Node,
    kind: BoundKind<'a>,
}

enum BoundKind<'a> {
    /// Matched sections, in document order (multiple only for repeated headings).
    Heading(Vec<Section<'a>>),
    /// The matched list, if any.
    List(Option<ListBound<'a>>),
    /// The matched paragraph block, if any.
    Prose(Option<&'a Block>),
}

struct Section<'a> {
    block: &'a Block,
    children: Vec<Bound<'a>>,
}

struct ListBound<'a> {
    items: Vec<ItemBound<'a>>,
}

struct ItemBound<'a> {
    /// Captured text (checkbox stripped for checklist nodes).
    text: String,
    checked: Option<bool>,
    /// Editable span of the item text.
    text_span: Span,
    children: Vec<Bound<'a>>,
}

/// Match a schema body against a document block tree. Returns the validation
/// problems and the binding tree (the latter is only meaningful when problems
/// is empty, but is always well-formed).
pub(crate) fn match_body<'a>(
    schema: &'a [Node],
    doc: &'a [Block],
    opts: SchemaOpts,
) -> (Vec<Problem>, Vec<Bound<'a>>) {
    let mut problems = Vec::new();
    let mut path = Vec::new();
    let bounds = match_scope(schema, doc, opts, &mut path, &mut problems);
    (problems, bounds)
}

fn match_scope<'a>(
    schema: &'a [Node],
    doc: &'a [Block],
    opts: SchemaOpts,
    path: &mut Vec<String>,
    problems: &mut Vec<Problem>,
) -> Vec<Bound<'a>> {
    let mut claimed = vec![false; doc.len()];
    let mut cursor = 0usize;
    let mut bounds = Vec::new();

    for node in schema {
        let start = if opts.ordered { cursor } else { 0 };
        match node {
            Node::Heading {
                level,
                title,
                head,
                children,
            } => {
                let idxs: Vec<usize> = if opts.ordered {
                    // Claim the contiguous run of matching sections from the
                    // first match onward; a later non-matching section belongs
                    // to a subsequent schema node, not to this one.
                    match (start..doc.len())
                        .find(|&i| !claimed[i] && is_section(&doc[i], *level, title))
                    {
                        Some(first) => {
                            let mut v = vec![first];
                            let mut i = first + 1;
                            while i < doc.len() && !claimed[i] && is_section(&doc[i], *level, title)
                            {
                                v.push(i);
                                i += 1;
                            }
                            v
                        }
                        None => Vec::new(),
                    }
                } else {
                    (start..doc.len())
                        .filter(|&i| !claimed[i] && is_section(&doc[i], *level, title))
                        .collect()
                };
                if let Some(msg) = card_problem(head.card, idxs.len(), "section") {
                    problems.push(problem(path, node, msg, None));
                }
                // A single-valued heading captures one section; claim only the
                // first so any duplicate is reported (under %strict) or errors
                // (Optional), rather than being silently swallowed.
                let claimed_idxs: &[usize] = if repeated(head.card) {
                    &idxs
                } else {
                    &idxs[..idxs.len().min(1)]
                };
                let mut sections = Vec::new();
                for &i in claimed_idxs {
                    claimed[i] = true;
                    if let Block::Section { children: sc, .. } = &doc[i] {
                        path.push(node.problem_alias());
                        let ch = match_scope(children, sc, opts, path, problems);
                        path.pop();
                        sections.push(Section {
                            block: &doc[i],
                            children: ch,
                        });
                    }
                }
                if opts.ordered {
                    if let Some(&last) = claimed_idxs.last() {
                        cursor = last + 1;
                    }
                }
                bounds.push(Bound {
                    node,
                    kind: BoundKind::Heading(sections),
                });
            }

            Node::List {
                style,
                item,
                head,
                children,
            } => {
                let found = (start..doc.len()).find(|&i| !claimed[i] && is_list(&doc[i], *style));
                let checklist = *style == ListStyle::Checklist;
                let mut lb = None;
                match found {
                    Some(i) => {
                        claimed[i] = true;
                        if let Block::List { items, .. } = &doc[i] {
                            if let Some(msg) = card_problem(head.card, items.len(), "item") {
                                problems.push(problem(path, node, msg, None));
                            }
                            let mut ibs = Vec::new();
                            for it in items {
                                let full_text = it.text_for(checklist);
                                let full_span = it.span_for(checklist);
                                if let Some(m) = item {
                                    if !matches_text(m, &full_text) {
                                        problems.push(problem(
                                            path,
                                            node,
                                            format!(
                                                "item does not match {}: {}",
                                                describe(m),
                                                full_text
                                            ),
                                            Some(full_span),
                                        ));
                                    }
                                }
                                // Capture text and the editable span *after* any
                                // literal label, so extract and edit agree.
                                let (text, text_span) = strip_label(item, &full_text, full_span);
                                let ch = if children.is_empty() {
                                    Vec::new()
                                } else {
                                    path.push(node.problem_alias());
                                    let c =
                                        match_scope(children, &it.children, opts, path, problems);
                                    path.pop();
                                    c
                                };
                                ibs.push(ItemBound {
                                    text,
                                    checked: it.checked,
                                    text_span,
                                    children: ch,
                                });
                            }
                            lb = Some(ListBound { items: ibs });
                        }
                        if opts.ordered {
                            cursor = i + 1;
                        }
                    }
                    None => {
                        if let Some(msg) = card_problem(head.card, 0, "item") {
                            problems.push(problem(path, node, msg, None));
                        }
                    }
                }
                bounds.push(Bound {
                    node,
                    kind: BoundKind::List(lb),
                });
            }

            Node::Prose { text, head } => {
                let found = (start..doc.len())
                    .find(|&i| !claimed[i] && matches!(doc[i], Block::Para { .. }));
                let mut para = None;
                match found {
                    Some(i) => {
                        claimed[i] = true;
                        if let Block::Para { text: t, span } = &doc[i] {
                            if let Some(m) = text {
                                if !matches_text(m, t) {
                                    problems.push(problem(
                                        path,
                                        node,
                                        format!("paragraph does not match {}", describe(m)),
                                        Some(*span),
                                    ));
                                }
                            }
                        }
                        para = Some(&doc[i]);
                        if opts.ordered {
                            cursor = i + 1;
                        }
                    }
                    None => {
                        if presence_required(head.card) {
                            problems.push(problem(
                                path,
                                node,
                                "missing required paragraph".into(),
                                None,
                            ));
                        }
                    }
                }
                bounds.push(Bound {
                    node,
                    kind: BoundKind::Prose(para),
                });
            }
        }
    }

    // Strict: any block no schema node claimed is unexpected.
    if opts.strict {
        for (i, b) in doc.iter().enumerate() {
            if !claimed[i] {
                let (label, span) = block_desc(b);
                problems.push(Problem {
                    path: path.clone(),
                    message: format!("unexpected {label}"),
                    span,
                });
            }
        }
    }

    bounds
}

// ---- extraction ----------------------------------------------------------

/// Walk the binding tree into a JSON object keyed by capture aliases.
pub(crate) fn to_json(bounds: &[Bound]) -> Map<String, Value> {
    let mut obj = Map::new();
    for b in bounds {
        let Some(key) = b.node.capture_alias() else {
            continue;
        };
        let value = match &b.kind {
            BoundKind::Heading(sections) => {
                let regex_title = matches!(
                    b.node,
                    Node::Heading {
                        title: Match::Regex(_),
                        ..
                    }
                );
                let make = |s: &Section| section_json(s, regex_title);
                if repeated(b.node.card()) {
                    Value::Array(sections.iter().map(make).collect())
                } else {
                    sections.first().map(make).unwrap_or(Value::Null)
                }
            }
            BoundKind::List(lb) => list_json(b.node, lb.as_ref()),
            BoundKind::Prose(p) => match p {
                Some(Block::Para { text, .. }) => Value::String(text.clone()),
                _ => Value::Null,
            },
        };
        obj.insert(key, value);
    }
    obj
}

fn section_json(s: &Section, regex_title: bool) -> Value {
    let mut m = to_json(&s.children);
    if regex_title {
        if let Block::Section { title, .. } = s.block {
            m.insert("title".into(), Value::String(title.clone()));
        }
    }
    Value::Object(m)
}

fn list_json(node: &Node, lb: Option<&ListBound>) -> Value {
    let Node::List {
        style,
        item,
        head,
        children,
    } = node
    else {
        return Value::Null;
    };
    let single = matches!(head.card, Card::Required | Card::Optional);
    let scalar = single && children.is_empty() && *style != ListStyle::Checklist;
    let Some(lb) = lb else {
        return if scalar {
            Value::Null
        } else {
            Value::Array(Vec::new())
        };
    };
    let _ = item;
    if scalar {
        return lb
            .items
            .first()
            .map(|ib| Value::String(ib.text.clone()))
            .unwrap_or(Value::Null);
    }
    Value::Array(
        lb.items
            .iter()
            .map(|ib| {
                let text = ib.text.clone();
                if children.is_empty() && *style != ListStyle::Checklist {
                    Value::String(text)
                } else {
                    let mut m = Map::new();
                    m.insert("text".into(), Value::String(text));
                    if *style == ListStyle::Checklist {
                        m.insert("checked".into(), Value::Bool(ib.checked.unwrap_or(false)));
                    }
                    for (k, v) in to_json(&ib.children) {
                        m.insert(k, v);
                    }
                    Value::Object(m)
                }
            })
            .collect(),
    )
}

/// Strip a literal label prefix (`Docs: foo` with label `Docs:` → `foo`) from an
/// item's text, returning the remaining text and the byte span that covers it
/// (so both extract and edit skip the label). Non-literal or unmatched labels
/// leave the text and span unchanged.
fn strip_label(item: &Option<Match>, text: &str, span: Span) -> (String, Span) {
    if let Some(Match::Literal(l)) = item {
        let trimmed = text.trim_start();
        let lead = text.len() - trimmed.len();
        if let Some(rest) = trimmed.strip_prefix(l.as_str()) {
            let stripped = rest.trim_start().to_string();
            // Advance the edit span past the label only when the source and the
            // flattened text line up byte-for-byte (no length-changing inline
            // markup in the item); otherwise keep the full span, so an edit
            // cannot splice inside markup and corrupt the item.
            let aligned = span.end.saturating_sub(span.start) == text.len();
            let span = if aligned {
                let after = rest.len() - rest.trim_start().len();
                Span {
                    start: span.start + lead + l.len() + after,
                    end: span.end,
                }
            } else {
                span
            };
            return (stripped, span);
        }
    }
    (text.to_string(), span)
}

/// A childless bare/`?` non-checklist list, which extracts as a single string
/// and is addressed for editing by its bare alias.
fn is_scalar_list(node: &Node) -> bool {
    matches!(
        node,
        Node::List { style, head, children, .. }
            if matches!(head.card, Card::Required | Card::Optional)
                && children.is_empty()
                && *style != ListStyle::Checklist
    )
}

// ---- edit navigation -----------------------------------------------------

/// Resolve a dotted path (segments already split) against the binding tree to
/// the byte span of an editable leaf. The path mirrors [`to_json`]'s shape:
/// object keys are aliases, array elements are numeric indexes.
///
/// Returns `Ok(None)` for a path that names a real node but not an editable
/// leaf; `Err((index, len))` for a numeric index out of range (so the caller
/// can report [`IndexOutOfRange`](crate::EditError::IndexOutOfRange)).
pub(crate) fn find_span(path: &[&str], bounds: &[Bound]) -> Result<Option<Span>, (usize, usize)> {
    let Some((key, rest)) = path.split_first() else {
        return Ok(None);
    };
    for b in bounds {
        if b.node.capture_alias().as_deref() != Some(*key) {
            continue;
        }
        return match &b.kind {
            BoundKind::Prose(p) => {
                if !rest.is_empty() {
                    return Ok(None);
                }
                Ok(match p {
                    Some(Block::Para { span, .. }) => Some(*span),
                    _ => None,
                })
            }
            BoundKind::Heading(sections) => {
                let (sec, rest2) = if repeated(b.node.card()) {
                    let Some((idx_s, r)) = rest.split_first() else {
                        return Ok(None);
                    };
                    let Ok(idx) = idx_s.parse::<usize>() else {
                        return Ok(None);
                    };
                    match sections.get(idx) {
                        Some(s) => (s, r),
                        None => return Err((idx, sections.len())),
                    }
                } else {
                    match sections.first() {
                        Some(s) => (s, rest),
                        None => return Ok(None),
                    }
                };
                if rest2.is_empty() {
                    Ok(None) // whole section not editable
                } else {
                    find_span(rest2, &sec.children)
                }
            }
            BoundKind::List(lb) => {
                let Some(lb) = lb else { return Ok(None) };
                let Some((idx_s, rest2)) = rest.split_first() else {
                    // A scalar list is editable by its bare alias (no index),
                    // mirroring its single-string extract shape.
                    return Ok(if is_scalar_list(b.node) {
                        lb.items.first().map(|ib| ib.text_span)
                    } else {
                        None
                    });
                };
                let Ok(idx) = idx_s.parse::<usize>() else {
                    return Ok(None);
                };
                let Some(ib) = lb.items.get(idx) else {
                    return Err((idx, lb.items.len()));
                };
                if rest2.is_empty() {
                    Ok(Some(ib.text_span))
                } else {
                    find_span(rest2, &ib.children)
                }
            }
        };
    }
    Ok(None)
}

// ---- block predicates and helpers ----------------------------------------

fn is_section(b: &Block, level: u8, title: &Match) -> bool {
    matches!(b, Block::Section { level: l, title: t, .. } if *l == level && matches_text(title, t))
}

fn is_list(b: &Block, style: ListStyle) -> bool {
    let Block::List {
        ordered,
        has_checkbox,
        ..
    } = b
    else {
        return false;
    };
    match style {
        ListStyle::Ordered => *ordered,
        ListStyle::Bullet => !*ordered,
        ListStyle::Checklist => !*ordered && *has_checkbox,
    }
}

fn matches_text(m: &Match, text: &str) -> bool {
    match m {
        Match::Literal(l) => text.trim_start().starts_with(l),
        Match::Regex(re) => re.is_match(text),
    }
}

fn describe(m: &Match) -> String {
    match m {
        Match::Literal(l) => format!("\"{l}\""),
        Match::Regex(re) => format!("/{}/", re.as_str()),
    }
}

fn block_desc(b: &Block) -> (&'static str, Option<Span>) {
    match b {
        Block::Section { title_span, .. } => ("section", Some(*title_span)),
        Block::List { .. } => ("list", None),
        Block::Para { span, .. } => ("paragraph", Some(*span)),
        Block::Other { span } => ("block", Some(*span)),
    }
}

/// `Some(message)` when `count` violates the cardinality. `unit` is "section"
/// or "item".
fn card_problem(card: Card, count: usize, unit: &str) -> Option<String> {
    let ok = match card {
        Card::Required | Card::Plus => count >= 1,
        Card::Optional => count <= 1,
        Card::Star => true,
        Card::Range(min, max) => {
            count >= min as usize && max.map(|m| count <= m as usize).unwrap_or(true)
        }
    };
    if ok {
        return None;
    }
    Some(match card {
        Card::Required | Card::Plus => format!("expected at least one {unit}, found {count}"),
        Card::Optional => format!("expected at most one {unit}, found {count}"),
        Card::Range(min, Some(max)) => format!("expected {min}..{max} {unit}(s), found {count}"),
        Card::Range(min, None) => format!("expected at least {min} {unit}(s), found {count}"),
        Card::Star => unreachable!(),
    })
}

fn presence_required(card: Card) -> bool {
    matches!(card, Card::Required | Card::Plus | Card::Range(1.., _))
}

fn repeated(card: Card) -> bool {
    matches!(card, Card::Plus | Card::Star | Card::Range(..))
}

fn problem(path: &[String], node: &Node, message: String, span: Option<Span>) -> Problem {
    let mut p = path.to_vec();
    p.push(node.problem_alias());
    // A node's description enriches the message, per the spec.
    let message = match node.desc() {
        Some(d) if !d.is_empty() => format!("{message} — \"{d}\""),
        _ => message,
    };
    Problem {
        path: p,
        message,
        span,
    }
}

/// The full alias path (root-to-node) of the single node capturing under
/// `alias`, or `None` if it is absent, ambiguous, or nested under an uncaptured
/// node. Used by [`Schema::edit`](crate::Schema::edit) to resolve a bare alias.
pub(crate) fn unique_alias_path(body: &[Node], alias: &str) -> Option<Vec<String>> {
    let mut found = Vec::new();
    collect_paths(body, alias, &mut Vec::new(), &mut found);
    if found.len() == 1 {
        found.pop()
    } else {
        None
    }
}

fn collect_paths(
    nodes: &[Node],
    alias: &str,
    prefix: &mut Vec<String>,
    out: &mut Vec<Vec<String>>,
) {
    for node in nodes {
        let Some(a) = node.capture_alias() else {
            continue; // uncaptured nodes are not path-addressable
        };
        prefix.push(a.clone());
        if a == alias {
            out.push(prefix.clone());
        }
        match node {
            Node::Heading { children, .. } | Node::List { children, .. } => {
                collect_paths(children, alias, prefix, out)
            }
            Node::Prose { .. } => {}
        }
        prefix.pop();
    }
}
