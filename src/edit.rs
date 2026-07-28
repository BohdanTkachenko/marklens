//! In-place editing: replace the text content of a named node identified by a
//! dot-separated alias path, leaving the rest of the document byte-for-byte
//! identical.
//!
//! Editing reuses the shared [`crate::doc`] block tree and [`crate::matcher`]
//! bindings, so the byte span it splices is exactly the one `validate`/`extract`
//! matched — no separate, divergent navigation.

use super::error::EditError;
use super::schema::Schema;
use super::{doc, matcher, render};

impl Schema {
    /// Replace the content of the node at `target` (a `.`-separated alias path,
    /// e.g. `"plan.cases.2"`) with `new_value` and return the modified markdown.
    ///
    /// The path mirrors [`Schema::extract`]'s output shape: object keys are
    /// aliases and array elements are numeric indexes, so a list item is
    /// `list.<i>`, a repeated heading's child is `heading.<i>.child`, and a
    /// nested item is `outer.<i>.inner.<j>`. The **first** segment may be a bare
    /// alias unique across the whole schema (e.g. `steps.1` for a `steps` list
    /// nested three levels deep); it is expanded to its full path automatically.
    /// Only leaf **string** nodes are editable — list items and prose
    /// paragraphs; whole headings and frontmatter keys are not. For list items
    /// the marker and any checkbox prefix are preserved, and `new_value` is
    /// escaped so it round-trips back through [`extract`](Schema::extract)
    /// unchanged — except for leading and trailing whitespace, which `extract`
    /// trims from every value.
    ///
    /// Returns [`EditError::TargetNotFound`] if the path does not resolve to an
    /// editable leaf, [`EditError::IndexOutOfRange`] if a numeric index exceeds
    /// its list, or [`EditError::InvalidValue`] if `new_value` contains a line
    /// break (which cannot be spliced as inline text).
    ///
    /// # Examples
    ///
    /// ```
    /// use marklens_core::parse_schema;
    ///
    /// let schema = parse_schema("## @plan Plan\n  - [ ] +@cases\n")?;
    /// let doc = "## Plan\n- [x] first\n- [ ] second\n";
    /// // `cases` is a schema-wide unique alias, so the leading path is optional.
    /// let updated = schema.edit(doc, "cases.0", "renamed")?;
    /// assert_eq!(updated, "## Plan\n- [x] renamed\n- [ ] second\n");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn edit(&self, md: &str, target: &str, new_value: &str) -> Result<String, EditError> {
        if new_value.contains(['\n', '\r']) {
            return Err(EditError::InvalidValue {
                reason: "contains a line break".into(),
            });
        }

        let (body, offset) = doc::strip_frontmatter(md);
        let blocks = doc::parse_blocks(body);
        let (_problems, bounds) = matcher::match_body(&self.body, &blocks, self.opts);

        // Resolve a bare first segment (a schema-wide unique alias) to its full
        // path; a segment that is already a top-level alias is left as-is.
        let raw: Vec<&str> = target.split('.').collect();
        let is_top = self
            .body
            .iter()
            .any(|n| n.capture_alias().as_deref() == raw.first().copied());
        let path_owned: Vec<String> = match (is_top, raw.first()) {
            (false, Some(first)) => match matcher::unique_alias_path(&self.body, first) {
                Some(mut full) => {
                    full.extend(raw[1..].iter().map(|s| s.to_string()));
                    full
                }
                None => raw.iter().map(|s| s.to_string()).collect(),
            },
            _ => raw.iter().map(|s| s.to_string()).collect(),
        };
        let path: Vec<&str> = path_owned.iter().map(String::as_str).collect();

        let span = match matcher::find_span(&path, &bounds) {
            Ok(Some(s)) => s,
            Ok(None) => return Err(EditError::TargetNotFound),
            Err((index, len)) => return Err(EditError::IndexOutOfRange { index, len }),
        };

        let abs_start = offset + span.start;
        let abs_end = offset + span.end;
        let escaped = render::esc_inline(new_value);

        // An empty checkbox item (`- [x]`) has an empty text span flush after
        // `]`. Insert a separator so the result is `- [x] value`, not
        // `- [x]value`. The empty-span guard keeps this from firing on a
        // non-empty item whose label happens to end in `]`.
        let sep = if abs_start == abs_end && abs_start > 0 && md.as_bytes()[abs_start - 1] == b']' {
            " "
        } else {
            ""
        };

        let mut out = String::with_capacity(md.len() + escaped.len() + sep.len());
        out.push_str(&md[..abs_start]);
        out.push_str(sep);
        out.push_str(&escaped);
        out.push_str(&md[abs_end..]);
        Ok(out)
    }
}
