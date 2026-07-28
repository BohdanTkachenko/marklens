//! Validation and extraction: check a markdown document against the schema and,
//! when it conforms, capture it as a typed JSON object.
//!
//! Both go through one shared pipeline: strip frontmatter, parse the body into
//! the [`crate::doc`] block tree, type-check the frontmatter, then run the
//! unified [`crate::matcher`] against the body. `validate` returns the problems;
//! `extract` additionally walks the resulting bindings into JSON.

use super::error::Problem;
use super::schema::Schema;
use super::{doc, frontmatter, matcher};
use serde_json::{Map, Value};

impl Schema {
    /// Validate a markdown document against the schema. An empty result means it
    /// conforms.
    ///
    /// Checks, in document order: frontmatter keys (presence, types, and — unless
    /// `%frontmatter = open` — the closed key set), then the body (section and
    /// list structure, cardinality, labels, ordering under `%ordered`, and —
    /// under `%strict` — the absence of unexpected blocks).
    pub fn validate(&self, markdown: &str) -> Vec<Problem> {
        self.check(markdown).0
    }

    /// Parse a **conforming** document into a typed JSON object keyed by the
    /// schema's capture aliases, or return the validation problems if it does
    /// not conform.
    ///
    /// Frontmatter fields are captured under their aliases with their declared
    /// types (int→number, bool→bool, list→array, the rest→string). Body shape
    /// follows [`Card`](crate::Card): a named heading becomes an object;
    /// repeated headings and multi-item lists become arrays; a childless
    /// bare/`?` list becomes a single string (its text after any literal label);
    /// checklist items become `{ "text", "checked" }` objects.
    ///
    /// # Examples
    ///
    /// ```
    /// use marklens_core::parse_schema;
    ///
    /// let schema = parse_schema("1. +@steps\n  - ?@note\n")?;
    /// let data = schema
    ///     .extract("1. open a tab\n   - use staging\n2. run it\n")
    ///     .expect("conforms");
    /// assert_eq!(data["steps"][0]["text"], "open a tab");
    /// assert_eq!(data["steps"][0]["note"], "use staging");
    /// assert_eq!(data["steps"][1]["note"], serde_json::Value::Null);
    /// # Ok::<(), marklens_core::SchemaError>(())
    /// ```
    pub fn extract(&self, markdown: &str) -> Result<Value, Vec<Problem>> {
        let (problems, value) = self.check(markdown);
        match value {
            Some(v) => Ok(v),
            None => Err(problems),
        }
    }

    /// Shared pipeline: returns the problems and, when there are none, the
    /// captured value.
    fn check(&self, markdown: &str) -> (Vec<Problem>, Option<Value>) {
        let (body, offset) = doc::strip_frontmatter(markdown);
        let blocks = doc::parse_blocks(body);

        let mut problems = Vec::new();
        let mut obj = Map::new();
        frontmatter::extract_frontmatter(
            &self.frontmatter,
            markdown,
            self.opts,
            &mut problems,
            &mut obj,
        );

        let (mut body_problems, bounds) = matcher::match_body(&self.body, &blocks, self.opts);
        for p in &mut body_problems {
            p.span = p.span.map(|s| s.shifted(offset));
        }
        problems.extend(body_problems);

        if problems.is_empty() {
            for (k, v) in matcher::to_json(&bounds) {
                obj.insert(k, v);
            }
            (problems, Some(Value::Object(obj)))
        } else {
            (problems, None)
        }
    }
}
