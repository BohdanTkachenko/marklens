//! Data-format codecs for the CLI: convert `serde_json::Value` to and from
//! JSON, YAML, TOML, and a typed XML dialect. Format handling lives here (in the
//! binary), so the library keeps a serde_json-only surface.

use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use serde_json::{Map, Value};

/// A data serialization format for `extract` output and `render` input.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Format {
    Json,
    Yaml,
    Toml,
    Xml,
}

impl Format {
    /// Guess a format from a file extension, for `render` input.
    pub fn from_ext(path: &Path) -> Option<Format> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "json" => Some(Format::Json),
            "yaml" | "yml" => Some(Format::Yaml),
            "toml" => Some(Format::Toml),
            "xml" => Some(Format::Xml),
            _ => None,
        }
    }

    /// Serialize a value, always terminated by a newline. `compact` only
    /// affects JSON.
    pub fn dump(self, value: &Value, compact: bool) -> Result<String, Box<dyn Error>> {
        let mut s = match self {
            Format::Json if compact => serde_json::to_string(value)?,
            Format::Json => serde_json::to_string_pretty(value)?,
            Format::Yaml => serde_yaml::to_string(value)?,
            // TOML has no null; drop null-valued keys (an absent optional field).
            Format::Toml => toml::to_string_pretty(&strip_nulls(value))?,
            Format::Xml => to_xml(value),
        };
        if !s.ends_with('\n') {
            s.push('\n');
        }
        Ok(s)
    }

    /// Parse a value from text in this format.
    pub fn parse(self, text: &str) -> Result<Value, Box<dyn Error>> {
        Ok(match self {
            Format::Json => serde_json::from_str(text)?,
            Format::Yaml => serde_yaml::from_str(text)?,
            Format::Toml => toml::from_str(text)?,
            Format::Xml => from_xml(text)?,
        })
    }
}

/// Recursively drop null-valued object entries (TOML cannot represent null).
fn strip_nulls(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(_, val)| !val.is_null())
                .map(|(k, val)| (k.clone(), strip_nulls(val)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(strip_nulls).collect()),
        other => other.clone(),
    }
}

// ---- typed XML -----------------------------------------------------------
//
// Objects carry `type="object"`, arrays `type="array"` (children named
// `<item>`), scalars carry `type="bool"|"number"` (strings and the default are
// untyped), and null is a self-closing `type="null"` element. The `type`
// attributes make the mapping a clean inverse of JSON.

fn to_xml(value: &Value) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    write_xml(&mut out, "marklens", value, 0);
    out
}

fn write_xml(out: &mut String, tag: &str, v: &Value, depth: usize) {
    let pad = "  ".repeat(depth);
    match v {
        Value::Null => {
            let _ = writeln!(out, "{pad}<{tag} type=\"null\"/>");
        }
        Value::Bool(b) => {
            let _ = writeln!(out, "{pad}<{tag} type=\"bool\">{b}</{tag}>");
        }
        Value::Number(n) => {
            let _ = writeln!(out, "{pad}<{tag} type=\"number\">{n}</{tag}>");
        }
        Value::String(s) => {
            let _ = writeln!(out, "{pad}<{tag}>{}</{tag}>", xml_escape(s));
        }
        Value::Array(a) => {
            let _ = writeln!(out, "{pad}<{tag} type=\"array\">");
            for item in a {
                write_xml(out, "item", item, depth + 1);
            }
            let _ = writeln!(out, "{pad}</{tag}>");
        }
        Value::Object(m) => {
            let _ = writeln!(out, "{pad}<{tag} type=\"object\">");
            for (k, val) in m {
                write_xml(out, k, val, depth + 1);
            }
            let _ = writeln!(out, "{pad}</{tag}>");
        }
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // XML 1.0 forbids C0 control characters except tab/LF/CR, and they
            // cannot be numerically escaped; drop them so output stays well-formed.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

// ---- XML parsing (quick-xml) ---------------------------------------------

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

fn from_xml(text: &str) -> Result<Value, Box<dyn Error>> {
    let mut reader = Reader::from_str(text);
    loop {
        match reader.read_event()? {
            Event::Start(e) => return parse_start(&mut reader, &e),
            Event::Empty(e) => return Ok(scalar_from_empty(&e)),
            Event::Eof => return Err("empty XML document".into()),
            _ => {}
        }
    }
}

fn node_type(e: &BytesStart) -> String {
    e.try_get_attribute("type")
        .ok()
        .flatten()
        .and_then(|a| a.normalized_value(XmlVersion::Explicit1_0).ok())
        .map(|c| c.into_owned())
        .unwrap_or_else(|| "string".to_string())
}

fn parse_start(reader: &mut Reader<&[u8]>, e: &BytesStart) -> Result<Value, Box<dyn Error>> {
    match node_type(e).as_str() {
        "object" => {
            let mut map = Map::new();
            loop {
                match reader.read_event()? {
                    Event::Start(child) => {
                        let key = local_name(&child);
                        map.insert(key, parse_start(reader, &child)?);
                    }
                    Event::Empty(child) => {
                        map.insert(local_name(&child), scalar_from_empty(&child));
                    }
                    Event::End(_) | Event::Eof => break,
                    _ => {}
                }
            }
            Ok(Value::Object(map))
        }
        "array" => {
            let mut arr = Vec::new();
            loop {
                match reader.read_event()? {
                    Event::Start(child) => arr.push(parse_start(reader, &child)?),
                    Event::Empty(child) => arr.push(scalar_from_empty(&child)),
                    Event::End(_) | Event::Eof => break,
                    _ => {}
                }
            }
            Ok(Value::Array(arr))
        }
        "null" => {
            read_to_end(reader)?;
            Ok(Value::Null)
        }
        ty => {
            let text = read_text(reader)?;
            Ok(scalar(ty, &text))
        }
    }
}

fn local_name(e: &BytesStart) -> String {
    String::from_utf8_lossy(e.name().as_ref()).into_owned()
}

fn scalar(ty: &str, text: &str) -> Value {
    match ty {
        "bool" => Value::Bool(text == "true"),
        "number" => serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string())),
        _ => Value::String(text.to_string()),
    }
}

fn scalar_from_empty(e: &BytesStart) -> Value {
    match node_type(e).as_str() {
        "null" => Value::Null,
        "bool" => Value::Bool(false),
        "number" => Value::Number(0.into()),
        _ => Value::String(String::new()),
    }
}

fn read_text(reader: &mut Reader<&[u8]>) -> Result<String, Box<dyn Error>> {
    let mut s = String::new();
    loop {
        match reader.read_event()? {
            // In quick-xml 0.41 entity references are their own events; plain
            // Text/CData no longer contain them.
            Event::Text(t) => s.push_str(&t.decode()?),
            Event::CData(t) => s.push_str(&t.decode()?),
            Event::GeneralRef(e) => {
                if let Some(ch) = e.resolve_char_ref()? {
                    s.push(ch); // numeric &#..;
                } else if let Some(txt) = resolve_predefined_entity(&e.decode()?) {
                    s.push_str(txt); // &amp; &lt; &gt; &quot; &apos;
                }
            }
            Event::End(_) | Event::Eof => break,
            _ => {}
        }
    }
    Ok(s)
}

fn read_to_end(reader: &mut Reader<&[u8]>) -> Result<(), Box<dyn Error>> {
    loop {
        match reader.read_event()? {
            Event::End(_) | Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}
