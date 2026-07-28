//! The parsed schema data model — the in-memory form of the DSL.
//!
//! Everything here is `Clone`/`PartialEq`/`Debug` and `Send + Sync`. Regex
//! labels and frontmatter regex types are compiled and validated once, at
//! [`parse_schema`](crate::parse_schema) time, and stored compiled (see
//! [`Regexp`]); they are never recompiled at match time. See
//! `docs/structure-dsl-spec.md` for the language this represents.

use regex::Regex;

/// A parsed schema: per-schema options, the frontmatter field schemas, and the
/// body node tree.
///
/// Produced by [`parse_schema`](crate::parse_schema). All fields are public,
/// but the parser establishes invariants that direct mutation can break:
/// capture aliases are **unique within each scope** (the parser rejects
/// duplicate `@name`s and colliding auto-derived slugs), [`Node::Prose`] never
/// has children (a schema line indented under a `>` line is a parse error), and
/// every [`Regexp`] is already compiled.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    /// The `%`-directive options.
    pub opts: SchemaOpts,
    /// Declared frontmatter fields, in declaration order. Type-checked by
    /// [`validate`](Schema::validate)/[`extract`](Schema::extract), captured by
    /// `extract`, and emitted by [`render`](Schema::render) and
    /// [`scaffold`](Schema::scaffold).
    pub frontmatter: Vec<FieldSchema>,
    /// The body node tree. Headings nest by their level (mirroring the
    /// documents they match); lists and prose nest by source indentation.
    pub body: Vec<Node>,
}

/// The `%`-directive options. Defaults match the spec: ordered matching,
/// strict matching, closed frontmatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaOpts {
    /// `%ordered` (default `true`): body nodes must appear in declared order.
    /// Each schema node searches document blocks from a per-scope cursor, so an
    /// out-of-order section is reported as *missing*; when `false` every node
    /// searches from the start of its scope.
    pub ordered: bool,
    /// `%strict` (default `true`): a document block not claimed by any schema
    /// node is reported as an unexpected-block problem. When `false`, undeclared
    /// blocks are ignored.
    pub strict: bool,
    /// `%frontmatter = open` (default closed): allow frontmatter keys not
    /// declared in the schema. When closed, an undeclared key is a problem.
    pub frontmatter_open: bool,
}

impl Default for SchemaOpts {
    fn default() -> Self {
        SchemaOpts {
            ordered: true,
            strict: true,
            frontmatter_open: false,
        }
    }
}

/// One typed frontmatter key, parsed from a `key[?] [@alias]: type` line.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldSchema {
    /// The YAML key, the leading `[A-Za-z0-9_]` run of the line.
    pub key: String,
    /// Capture alias; defaults to `key` unless renamed with `@name`.
    pub alias: String,
    /// `true` when the key is marked `key?` (may be absent from the document).
    pub optional: bool,
    /// The declared value type.
    pub ty: FieldType,
    /// The `<? … ?>` description, if any.
    pub desc: Option<String>,
}

/// A frontmatter value type. Values in a document's YAML block are checked
/// against it by [`validate`](Schema::validate)/[`extract`](Schema::extract)
/// and it selects [`scaffold`](Schema::scaffold) placeholders.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    /// `string` — any scalar.
    Str,
    /// `int` — a base-10 integer (captured as a JSON number).
    Int,
    /// `bool` — `true`/`false` (captured as a JSON bool).
    Bool,
    /// `date` — an `RFC 3339`-style `YYYY-MM-DD` calendar date (captured as a
    /// string; the day/month ranges are checked).
    Date,
    /// `enum(a, b, …)`. The value must equal one of the listed identifiers.
    /// May be empty (`enum()` parses to zero values and matches nothing).
    Enum(Vec<String>),
    /// `[T]` — a list whose every element has type `T` (a scalar). Nested
    /// `[[T]]` is rejected at parse time, as is anything after the matching `]`.
    List(Box<FieldType>),
    /// `/pattern/flags` — a string matching the (unanchored) pattern.
    Regex(Regexp),
}

/// A compiled, validated regular expression that stays `Clone`/`PartialEq`/
/// `Debug` by carrying its source text alongside the compiled [`Regex`].
///
/// Equality and `Debug` use the source pattern; the compiled form is shared
/// cheaply on `Clone`. Built by [`parse_schema`](crate::parse_schema), so an
/// invalid pattern is a schema error rather than a silent never-match.
#[derive(Clone)]
pub struct Regexp {
    src: String,
    re: Regex,
}

impl Regexp {
    /// Compile `src` (already including any `(?flags)` prefix).
    pub(crate) fn new(src: impl Into<String>) -> Result<Self, regex::Error> {
        let src = src.into();
        let re = Regex::new(&src)?;
        Ok(Regexp { src, re })
    }

    /// `true` if the pattern matches anywhere in `text` (unanchored).
    pub fn is_match(&self, text: &str) -> bool {
        self.re.is_match(text)
    }

    /// The pattern source (with any folded `(?flags)` prefix).
    pub fn as_str(&self) -> &str {
        &self.src
    }
}

impl PartialEq for Regexp {
    fn eq(&self, other: &Self) -> bool {
        self.src == other.src
    }
}

impl Eq for Regexp {}

impl std::fmt::Debug for Regexp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Regexp({:?})", self.src)
    }
}

/// A body node: a heading (with children), a list (with a per-item child
/// schema), or a paragraph.
///
/// Headings nest by level; lists and prose nest by source indentation.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A `#`…`######` heading and the section it owns.
    Heading {
        /// Heading level, 1–6 (the number of `#`s).
        level: u8,
        /// The title pattern. Defaults to `Match::Regex(".*")` (any title)
        /// when the schema line carries no label.
        title: Match,
        /// Alias, cardinality, description.
        head: Head,
        /// Child nodes of this section. A child heading nests under this one
        /// when its `level` is strictly greater.
        children: Vec<Node>,
    },
    /// A list: `-` bullet, `1.` ordered, or `- [ ]` checklist.
    List {
        /// Which document list styles this node matches.
        style: ListStyle,
        /// Optional label that every item of the matched list must satisfy.
        item: Option<Match>,
        /// Alias, cardinality (bounds the item count), description.
        head: Head,
        /// A per-item child schema, matched against each item's nested blocks.
        children: Vec<Node>,
    },
    /// A `>` paragraph requirement. Prose nodes never have children (a schema
    /// line indented under a `>` line is a parse error).
    Prose {
        /// Optional pattern the paragraph text must match.
        text: Option<Match>,
        /// Alias, cardinality (presence-only), description.
        head: Head,
    },
}

impl Node {
    pub(crate) fn set_children(&mut self, kids: Vec<Node>) {
        match self {
            Node::Heading { children, .. } | Node::List { children, .. } => *children = kids,
            // Prose cannot hold children; the parser rejects indented prose
            // before this is ever reached, so `kids` is always empty here.
            Node::Prose { .. } => debug_assert!(kids.is_empty()),
        }
    }

    /// This node's cardinality.
    pub(crate) fn card(&self) -> Card {
        match self {
            Node::Heading { head, .. } | Node::List { head, .. } | Node::Prose { head, .. } => {
                head.card
            }
        }
    }

    /// The alias this node captures under in `extract` output, or `None` if it
    /// is not captured (a regex-titled heading, or a list/prose with no
    /// `@name`).
    pub(crate) fn capture_alias(&self) -> Option<String> {
        match self {
            Node::Heading { head, title, .. } => head.name.clone().or_else(|| match title {
                Match::Literal(t) => Some(slug(t)),
                Match::Regex(_) => None,
            }),
            Node::List { head, .. } | Node::Prose { head, .. } => head.name.clone(),
        }
    }

    /// The breadcrumb label for this node in [`Problem`] paths: its capture
    /// alias, or the literal `"block"` for uncaptured nodes.
    pub(crate) fn problem_alias(&self) -> String {
        self.capture_alias().unwrap_or_else(|| "block".to_string())
    }

    /// This node's `<? … ?>` description, if any.
    pub(crate) fn desc(&self) -> Option<&str> {
        match self {
            Node::Heading { head, .. } | Node::List { head, .. } | Node::Prose { head, .. } => {
                head.desc.as_deref()
            }
        }
    }
}

/// Slug a heading title into an auto-derived alias: ASCII-lowercase, every
/// other character becomes `_` (runs not collapsed), ends trimmed.
pub(crate) fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// The shared annotation head of every node: alias, cardinality, description.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Head {
    /// Explicit `@name` capture alias.
    ///
    /// `None` means:
    /// * for a **literal-titled heading**, auto-derive the alias by slugging
    ///   the schema title — ASCII-lowercase, every other character becomes
    ///   `_` (runs not collapsed), ends trimmed: `Trade-offs: speed` →
    ///   `trade_offs__speed`;
    /// * for regex-titled headings, lists, and prose, the node is validated
    ///   but **not captured** (and appears in [`Problem`](crate::Problem)
    ///   paths as the literal `"block"`).
    ///
    /// The parser rejects a schema whose scope has two nodes with the same
    /// effective alias, so `extract` never overwrites a capture.
    pub name: Option<String>,
    /// Cardinality (bare = [`Card::Required`]).
    pub card: Card,
    /// The `<? … ?>` description. Included in validation problem messages when
    /// present.
    pub desc: Option<String>,
}

/// Which document lists a [`Node::List`] matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListStyle {
    /// An unordered list (schema marker `- `).
    Bullet,
    /// An ordered list (schema marker `1.`).
    Ordered,
    /// An unordered list where at least one item starts with a checkbox
    /// (schema marker `- [ ]`).
    ///
    /// Checkboxes are hand-detected (the GFM tasklist extension is off): item
    /// text begins with `[ ] `, `[x] `, or `[X] `, or is exactly `[ ]`/`[x]`
    /// (an empty checkbox item). Each item captures its checked state under
    /// `"checked"`, so `extract`→`render` preserves `[x]`/`[ ]`.
    Checklist,
}

/// A label: a literal the text must start with, or a regex the text must match.
/// A bare heading title is a `Literal`.
///
/// Both variants compare against comrak-*flattened* document text: emphasis and
/// code-span markers are removed and link text is kept without its URL, so
/// patterns targeting raw markdown syntax (e.g. `^\[.+\]\(.+\)$` for links)
/// cannot match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Match {
    /// A **case-sensitive prefix** match (`starts_with`) on the trimmed text:
    /// `Setup` matches `Setup and teardown`. Bare labels are
    /// backslash-unescaped at parse time (`\x` → `x`), so a literal backslash
    /// must be written `\\`.
    Literal(String),
    /// A compiled regex ([`Regexp`]), with `/pat/flags` flags pre-folded into a
    /// leading `(?flags)` group. Matching is unanchored `is_match`.
    Regex(Regexp),
}

/// Cardinality. On a list it bounds the item count of the matched list; on a
/// heading it counts matching sibling sections; on prose it is presence-only.
/// `Required` is the bare default.
///
/// | Variant | Syntax | Validation rule |
/// |---|---|---|
/// | `Required` | (bare) | count ≥ 1 |
/// | `Optional` | `?` | count ≤ 1 |
/// | `Plus` | `+` | count ≥ 1 |
/// | `Star` | `*` | any count |
/// | `Range(m, Some(n))` | `{m,n}` or `{m}` | m ≤ count ≤ n |
/// | `Range(m, None)` | `{m,}` | count ≥ m |
///
/// For extraction and rendering of headings and *childless* lists,
/// `Required`/`Optional` yield a single value while `Plus`/`Star`/`Range`
/// yield an array — so `{1}` extracts an array of one where bare extracts a
/// scalar. A list *with* child nodes always yields an array of objects
/// regardless of cardinality, and prose always yields a single string.
/// Parsing fallback: a `{…}` token that does not parse as a range (e.g.
/// `{a,b}`) is silently treated as the start of a literal label; escape a
/// genuine leading brace as `\{`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Card {
    /// Bare default: at least one.
    #[default]
    Required,
    /// `?`: at most one.
    Optional,
    /// `*`: any number.
    Star,
    /// `+`: at least one; captured as an array on headings and childless lists
    /// (prose still captures a single string).
    Plus,
    /// `{m}`, `{m,}`, `{m,n}`: an inclusive count range (`None` = no upper
    /// bound).
    Range(u32, Option<u32>),
}
