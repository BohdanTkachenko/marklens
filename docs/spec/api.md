# Public API

```rust
pub fn parse_schema(src: &str) -> Result<Schema, SchemaError>;

impl Schema {
    pub fn validate(&self, md: &str) -> Vec<Problem>;
    /// Parse a conforming doc into the typed object (errors if non-conforming).
    pub fn extract(&self, md: &str) -> Result<serde_json::Value, Vec<Problem>>;
    /// Render a data object into a conforming markdown document (inverse of extract).
    pub fn render(&self, data: &serde_json::Value) -> Result<String, RenderError>;
    /// Render with placeholder/default values — a starter document.
    pub fn scaffold(&self) -> String;
    // NOTE: `query` (a jq selector over the extracted object) was dropped — see Operations.
    /// Replace the node addressed by a capture alias or dotted path; returns new source.
    pub fn edit(&self, md: &str, target: &str, value: &str) -> Result<String, EditError>;
}

pub struct Problem     { pub path: Vec<String>, pub message: String, pub span: Option<Span> }
pub struct SchemaError { pub line: usize, pub col: usize, pub message: String }
pub struct Span        { pub start: usize, pub end: usize }
```

`Problem.span` is the byte range (into the original document, frontmatter
included) of the offending block when one can be pinpointed — a label mismatch,
an unexpected block — and `None` for "missing"/cardinality problems.
`SchemaError.col` is the marker column for body-node errors (indent + 1) and `1`
for frontmatter/directive errors; `line` is always accurate.

## Embedding in a host application

A host application uses marklens for the parts of its documents that have
structure, keeping its own frontmatter/field validation, ID, and relation logic.
A typical loop over a stored schema per structured section:

```markdown
### Setup
  - +@items
### Procedure
  1. +@steps
```

Note the space after `-`: the bullet marker is `- ` (dash + space), and the card
glues after it (`- +`, `- +@items`) — `-+` is an unrecognized marker.

- A **check/verify** step extracts a section body and calls `validate`, prefixing
  each problem with the document's id and heading. With `%strict` (the default),
  `validate` rejects blocks the schema does not declare.
- A **new-document** step calls `scaffold()` to seed a section body.
- A **precise-edit** command resolves a capture path and calls `edit` — the "set
  `procedure.steps.1` to …" use case, with no full-file rewrite. Dotted index
  form; bracket `steps[1]` syntax is not implemented.

## Open decisions

1. **Exact annotation syntax** — `marker card?@name? label? <? desc ?>` with
   backslash/quote escaping ([Escaping](./grammar.md#escaping)).
   *(Shipped as specified.)*
2. **Query engine** — *resolved: dropped* (see
   [Edit in-place](./operations.md#edit-in-place)); no jq engine is bundled.
3. **Markdown library** — *resolved: `comrak`* (AST + sourcepos).

(Crate name **`marklens`** is **resolved**. Parser-is-a-library, strict ordering,
strict matching, and closed frontmatter are **resolved and enforced** per
[Resolved defaults](./defaults.md).)

## Phasing

Status of the design against the implementation:

1. Crate + workspace layout: library `marklens-core` + CLI `marklens`. *(done)*
2. **Validate + scaffold:** frontmatter + body grammar, `@name`/desc, located
   errors, spans on problems, frontmatter checking, `%strict` enforcement.
   *(done)*
3. **Extract:** schema → typed JSON object via capture aliases (body +
   frontmatter). *(done)*
4. **Render:** data object → markdown (inverse of extract); scaffold as a thin
   wrapper over it. *(done — inverse for plain-text values, per [Capabilities](../structure-dsl-spec.md#capabilities))*
5. **Query:** *(dropped)*
6. **Edit in-place:** alias/path → span → byte-accurate splice, including
   bare-alias addressing. *(done; bracket paths pending)*
7. Docs + publish + IDE integration groundwork. *(docs current; publish and
   IDE hover/completion pending)*
