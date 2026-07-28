//! Frontmatter: type-checking, extraction, and rendering of the `--- … ---`
//! YAML block against the schema's [`FieldSchema`]s.
//!
//! A deliberately small YAML subset is supported, chosen so that
//! [`render_frontmatter`] and [`extract_frontmatter`] invert each other:
//! `key: scalar` lines, double-quoted scalars, and flow lists `[a, b]`. Nested
//! maps and block sequences are not supported.

use crate::doc::frontmatter_block;
use crate::error::{Problem, RenderError};
use crate::schema::{FieldSchema, FieldType, SchemaOpts};
use serde_json::{Map, Value};
use std::fmt::Write as _;

/// A raw frontmatter value before type coercion.
enum Raw {
    Scalar(String),
    List(Vec<String>),
}

/// Type-check a document's frontmatter and, if it conforms, capture its values
/// keyed by alias. Appends any problems to `problems`.
///
/// A schema that declares no frontmatter fields ignores a document's
/// frontmatter entirely (a body-only schema does not constrain it).
pub(crate) fn extract_frontmatter(
    fields: &[FieldSchema],
    md: &str,
    opts: SchemaOpts,
    problems: &mut Vec<Problem>,
    out: &mut Map<String, Value>,
) {
    if fields.is_empty() {
        return;
    }
    let raw = parse_block(frontmatter_block(md).unwrap_or(""));

    for f in fields {
        match raw.iter().find(|(k, _)| k == &f.key) {
            None => {
                if !f.optional {
                    problems.push(fm_problem(
                        &f.alias,
                        format!("missing required frontmatter key `{}`", f.key),
                    ));
                }
            }
            Some((_, val)) => match coerce(val, &f.ty) {
                Ok(v) => {
                    out.insert(f.alias.clone(), v);
                }
                Err(reason) => problems.push(fm_problem(
                    &f.alias,
                    format!("frontmatter key `{}` {reason}", f.key),
                )),
            },
        }
    }

    if !opts.frontmatter_open {
        for (k, _) in &raw {
            if !fields.iter().any(|f| &f.key == k) {
                problems.push(fm_problem(k, format!("unexpected frontmatter key `{k}`")));
            }
        }
    }
}

fn fm_problem(alias: &str, message: String) -> Problem {
    Problem {
        path: vec![alias.to_string()],
        message,
        span: None,
    }
}

fn coerce(raw: &Raw, ty: &FieldType) -> Result<Value, String> {
    match ty {
        FieldType::Str => match raw {
            Raw::Scalar(s) => Ok(Value::String(s.clone())),
            Raw::List(_) => Err("expected a string, found a list".into()),
        },
        FieldType::Int => match raw {
            Raw::Scalar(s) => s
                .parse::<i64>()
                .map(|n| Value::Number(n.into()))
                .map_err(|_| format!("is not an integer: `{s}`")),
            Raw::List(_) => Err("expected an integer, found a list".into()),
        },
        FieldType::Bool => match raw {
            Raw::Scalar(s) if s == "true" => Ok(Value::Bool(true)),
            Raw::Scalar(s) if s == "false" => Ok(Value::Bool(false)),
            Raw::Scalar(s) => Err(format!("is not a bool: `{s}`")),
            Raw::List(_) => Err("expected a bool, found a list".into()),
        },
        FieldType::Date => match raw {
            Raw::Scalar(s) if is_date(s) => Ok(Value::String(s.clone())),
            Raw::Scalar(s) => Err(format!("is not a YYYY-MM-DD date: `{s}`")),
            Raw::List(_) => Err("expected a date, found a list".into()),
        },
        FieldType::Enum(vals) => match raw {
            Raw::Scalar(s) if vals.iter().any(|v| v == s) => Ok(Value::String(s.clone())),
            Raw::Scalar(s) => Err(format!("`{s}` is not one of {}", vals.join(", "))),
            Raw::List(_) => Err("expected an enum value, found a list".into()),
        },
        FieldType::Regex(re) => match raw {
            Raw::Scalar(s) if re.is_match(s) => Ok(Value::String(s.clone())),
            Raw::Scalar(s) => Err(format!("`{s}` does not match /{}/", re.as_str())),
            Raw::List(_) => Err("expected a string, found a list".into()),
        },
        FieldType::List(inner) => {
            let items = match raw {
                Raw::List(items) => items.clone(),
                Raw::Scalar(s) if s.is_empty() => Vec::new(),
                Raw::Scalar(_) => return Err("expected a list".into()),
            };
            let mut arr = Vec::with_capacity(items.len());
            for it in items {
                arr.push(coerce(&Raw::Scalar(it), inner)?);
            }
            Ok(Value::Array(arr))
        }
    }
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    if !b[..4]
        .iter()
        .chain(&b[5..7])
        .chain(&b[8..])
        .all(u8::is_ascii_digit)
    {
        return false;
    }
    let year: u32 = s[0..4].parse().unwrap_or(0);
    let month: u32 = s[5..7].parse().unwrap_or(0);
    let day: u32 = s[8..10].parse().unwrap_or(0);
    if !(1..=12).contains(&month) {
        return false;
    }
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    };
    (1..=max_day).contains(&day)
}

// ---- raw parsing ---------------------------------------------------------

fn parse_block(inner: &str) -> Vec<(String, Raw)> {
    let mut out = Vec::new();
    for line in inner.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let Some((k, v)) = t.split_once(':') else {
            continue;
        };
        let (k, v) = (k.trim().to_string(), v.trim());
        let raw = if v.is_empty() {
            Raw::Scalar(String::new())
        } else if v.starts_with('[') {
            Raw::List(parse_flow(v))
        } else {
            Raw::Scalar(unquote(v))
        };
        out.push((k, raw));
    }
    out
}

/// Split a flow list `[a, b, "c, d"]` into unquoted elements.
fn parse_flow(v: &str) -> Vec<String> {
    let inner = v
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(v);
    let inner = inner.trim();
    if inner.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            '\\' if in_quote => {
                cur.push(c);
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            ',' if !in_quote => {
                out.push(unquote(cur.trim()));
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() || !out.is_empty() {
        out.push(unquote(cur.trim()));
    }
    out
}

/// Strip and unescape a double-quoted scalar; return non-quoted input verbatim.
fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let inner = &s[1..s.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some(other) => out.push(other),
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        out
    } else {
        s.to_string()
    }
}

// ---- rendering -----------------------------------------------------------

/// Render the frontmatter block (with fences and a trailing blank line), or an
/// empty string if there are no fields.
pub(crate) fn render_frontmatter(
    fields: &[FieldSchema],
    data: &Value,
) -> Result<String, RenderError> {
    if fields.is_empty() {
        return Ok(String::new());
    }
    let mut out = String::from("---\n");
    let obj = data.as_object();
    for f in fields {
        let v = obj.and_then(|o| o.get(&f.alias));
        match v {
            Some(v) if !v.is_null() => {
                let _ = writeln!(out, "{}: {}", f.key, render_value(&f.key, v, &f.ty)?);
            }
            _ => {
                if !f.optional {
                    return Err(RenderError::MissingField(f.alias.clone()));
                }
            }
        }
    }
    out.push_str("---\n\n");
    Ok(out)
}

fn render_value(key: &str, v: &Value, ty: &FieldType) -> Result<String, RenderError> {
    let wrong = |expected| RenderError::WrongType {
        field: key.to_string(),
        expected,
    };
    match ty {
        FieldType::Str | FieldType::Date | FieldType::Regex(_) => {
            v.as_str().map(quote_scalar).ok_or_else(|| wrong("string"))
        }
        FieldType::Int => v
            .as_i64()
            .map(|n| n.to_string())
            .ok_or_else(|| wrong("integer")),
        FieldType::Bool => v
            .as_bool()
            .map(|b| b.to_string())
            .ok_or_else(|| wrong("bool")),
        FieldType::Enum(vals) => {
            let s = v.as_str().ok_or_else(|| wrong("string"))?;
            if vals.iter().any(|x| x == s) {
                Ok(quote_scalar(s))
            } else {
                Err(RenderError::InvalidFrontmatter {
                    key: key.to_string(),
                    reason: format!("`{s}` is not one of {}", vals.join(", ")),
                })
            }
        }
        FieldType::List(inner) => {
            let arr = v.as_array().ok_or_else(|| wrong("array"))?;
            let mut parts = Vec::with_capacity(arr.len());
            for elem in arr {
                parts.push(render_value(key, elem, inner)?);
            }
            Ok(format!("[{}]", parts.join(", ")))
        }
    }
}

/// Quote a scalar if it would otherwise reparse differently.
fn quote_scalar(s: &str) -> String {
    if needs_quote(s) {
        let escaped = s
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\t', "\\t");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn needs_quote(s: &str) -> bool {
    if s.is_empty() {
        // Emit `""` so an empty string is distinct from an omitted value and a
        // one-element list `[""]` is distinct from an empty list `[]`.
        return true;
    }
    let first = s.as_bytes()[0];
    s != s.trim()
        || s.contains(['\n', '\r', '"', ',', '[', ']', '#', ':'])
        || matches!(
            first,
            b'[' | b'{'
                | b'\''
                | b'>'
                | b'|'
                | b'&'
                | b'*'
                | b'!'
                | b'%'
                | b'@'
                | b'`'
                | b'-'
                | b'?'
        )
}

// ---- scaffolding ---------------------------------------------------------

/// A type-valid placeholder for scaffolding a required key.
pub(crate) fn placeholder(ty: &FieldType) -> String {
    match ty {
        FieldType::Str => "\"\"".to_string(),
        FieldType::Int => "0".to_string(),
        FieldType::Bool => "false".to_string(),
        FieldType::Date => "1970-01-01".to_string(),
        FieldType::Enum(vals) => vals.first().cloned().unwrap_or_default(),
        FieldType::List(_) => "[]".to_string(),
        // A regex has no universal satisfying value; emit an empty string.
        FieldType::Regex(_) => "\"\"".to_string(),
    }
}
