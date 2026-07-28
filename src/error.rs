//! Error types: schema-parse errors (located) and document-validation problems.

/// A failure while parsing the DSL schema source.
///
/// Displays as `schema:{line}:{col}: {message}`. `line` is 1-based and points
/// at the offending line (for an unterminated frontmatter block it reports the
/// last line of the input). `col` is 1-based; body-marker errors point at the
/// marker column, and other errors fall back to column 1.
///
/// Produced for: a `%directive` without `=` or with an unknown key/value; a
/// frontmatter line without `key: type` or without a key; an unknown or
/// unclosed field type; an **invalid regex** label or type; a missing closing
/// `---`; an unrecognized body marker; a schema line indented under a `>` prose
/// node; and two nodes sharing a capture alias in one scope.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("schema:{line}:{col}: {message}")]
pub struct SchemaError {
    /// 1-based line of the offending schema source line.
    pub line: usize,
    /// 1-based column; the marker column for body-node errors, else `1`.
    pub col: usize,
    /// Human-readable description of the failure.
    pub message: String,
}

impl SchemaError {
    pub(crate) fn new(line: usize, col: usize, message: impl Into<String>) -> Self {
        SchemaError {
            line,
            col,
            message: message.into(),
        }
    }
}

/// One way a document failed to conform, addressed by the breadcrumb `path`
/// (alias chain) of the schema node that was unmet.
///
/// `path` holds capture aliases from the schema root down to the unmet node:
/// explicit `@name`s, auto-derived slugs for literal-titled headings, and the
/// literal string `"block"` for unnamed lists/prose and for regex-titled
/// headings without `@name`. [`Display`](std::fmt::Display) joins the path with
/// ` › `, e.g. `plan › cases: expected at least one item, found 0`.
///
/// [`span`](Problem::span) carries the byte range of the offending block when
/// one can be pinpointed (label mismatch, unexpected block); it is `None` for
/// "missing"/cardinality problems.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// Breadcrumb of capture aliases from the schema root to the unmet node.
    pub path: Vec<String>,
    /// Human-readable description, e.g. `expected at least one item, found 0`.
    pub message: String,
    /// Byte span (into the original document) of the offending block, if one
    /// can be pinpointed.
    pub span: Option<crate::Span>,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path.join(" › "), self.message)
        }
    }
}

impl std::error::Error for Problem {}

/// A failure while rendering markdown from data via
/// [`Schema::render`](crate::Schema::render).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// A required body node's value was absent or JSON `null`, or a required
    /// frontmatter key was absent or explicitly `null`. Carries the capture
    /// alias.
    #[error("required field '{0}' is missing from the data")]
    MissingField(String),
    /// A value had a JSON type the schema node cannot render (e.g. an object
    /// where a string is expected, or a scalar where an array is expected).
    #[error("field '{field}' has wrong type: expected {expected}")]
    WrongType {
        /// Capture alias of the offending value.
        field: String,
        /// What the renderer needed, e.g. `"string"`, `"array"`, `"object"`.
        expected: &'static str,
    },
    /// A frontmatter value did not satisfy its declared
    /// [`FieldType`](crate::FieldType) (e.g. an `enum` value not in the set).
    #[error("frontmatter key '{key}' has an invalid value: {reason}")]
    InvalidFrontmatter {
        /// The frontmatter key.
        key: String,
        /// Why the value was rejected.
        reason: String,
    },
}

/// A failure while editing a document in-place via
/// [`Schema::edit`](crate::Schema::edit).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// The `target` path did not resolve to an editable leaf: an unknown alias,
    /// a wrong path shape, or an unsupported target kind (whole headings and
    /// frontmatter keys are not editable).
    #[error("target path does not resolve to an editable node")]
    TargetNotFound,
    /// A numeric path segment addressed a list index that does not exist.
    #[error("index {index} is out of range (list has {len} items)")]
    IndexOutOfRange {
        /// The requested 0-based index.
        index: usize,
        /// The actual item count of the list.
        len: usize,
    },
    /// The replacement value cannot be spliced as inline text (it contains a
    /// line break, which would restructure the document).
    #[error("replacement value is not valid inline text: {reason}")]
    InvalidValue {
        /// Why the value was rejected.
        reason: String,
    },
}
