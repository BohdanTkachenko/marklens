# Operations

## Edit in-place

`edit(md, target, new_value)` resolves `target` against the same bindings
`validate`/`extract` produced → the matched byte span → splices the escaped
`new_value`, re-rendering only that node. Everything else is byte-preserved.

- **Path shape:** dotted, mirroring `extract`'s output — object
  keys are aliases and array elements are numeric indexes. A list item is
  `list.<i>`, a repeated heading's child is `heading.<i>.child`, a nested item is
  `outer.<i>.inner.<j>`. For an item *with* children, `list.<i>` targets its
  `text`; `list.<i>.text` is **not** a path. The **first segment** may be a bare
  alias that is unique across the whole schema (e.g. `steps.1` for a `steps` list
  nested several levels deep) — it expands to its full path automatically.
- **Editable leaves:** list items and prose paragraphs only — not whole headings
  or frontmatter keys. For list items the marker and any checkbox prefix are
  preserved, and `new_value` is escaped so it round-trips back through `extract`.
- **Errors:** `EditError::TargetNotFound` (path resolves to nothing editable),
  `EditError::IndexOutOfRange { index, len }` (numeric index past the end — this
  *is* now returned), `EditError::InvalidValue` (a `new_value` with a line
  break, which cannot splice as inline text).
- **Bracket paths (`steps[1]`) are not implemented** — use dotted `steps.1`.
- Resolution is positional within a scope: the document should conform to the
  schema (optional nodes present in schema order) so indexes land on the intended
  item.

**Custom extraction templates (future, host-exposed):** because captures are
named, a consumer can ship its own schemas purely to *extract* data — "give me
`.manual.steps` from every doc" — without that schema being the validation
schema. A host application can expose this.

## Scaffolding

`Schema::scaffold()` walks the tree: required frontmatter keys with typed
placeholder values (`""` string, `0` int, `false` bool, `1970-01-01` date, first
enum value, `[]` list), headings verbatim, one placeholder item per required list
(one per `{m,…}` minimum), labels as literal prefixes, and `?`/`*`/min-0 nodes
omitted. Prose scaffolds as a `TODO` line. The schema and the new-document
template are one artifact.

Scaffold output **validates against its own schema for the common case** — a
schema of literal-titled headings and plain lists scaffolds to a conforming
document. It does **not** validate when a node has no satisfying placeholder: a
regex-titled heading/subsection scaffolds empty text (fails `/.+/`), a required
prose node with no literal label scaffolds `TODO`, and a required checklist
scaffolds a bare `- [ ] ` — treat those as a human starting point. Descriptions
are not emitted as guiding comments (left open by the design as a "may").

## Markdown parsing

A real parser is required (nesting, lists, headings) **and it must expose source
positions** for the in-place-edit feature ([Edit in-place](#edit-in-place)).

- **Recommended: `comrak`** — CommonMark, a real AST, and a `sourcepos`
  extension giving line/col spans per node. Best fit for the tree + spans we
  need.
- **Alternative: `pulldown-cmark`** — fast, byte **offsets** via
  `into_offset_iter`, but event-stream (we'd rebuild a tree).

Decision: the implementation uses **comrak** with default options — no GFM
extensions enabled; checkboxes are recognized by hand from item text (`[ ]`,
`[x]`, `[X]`). This pulls comrak into the dependency tree of any consumer.
