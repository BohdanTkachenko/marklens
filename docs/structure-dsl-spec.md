# marklens — design spec

*Markdown ⇄ data, via a template.*

**Status:** language and API reference for the crate. The core is
**implemented**: all five operations work, frontmatter is typed and captured,
`%strict`/`%ordered`/`%frontmatter` are enforced, and `extract`/`render` are
inverses for plain-text values (with the documented trimming/flattening
caveats). The [implementation status](#implementation-status) table below marks
each major feature, and the few genuinely unbuilt items (`query`, the `*`
freeform wildcard, inline-vocabulary markers) are flagged both there and in their
sections. The [worked example](./marklens-reference.md) exercises every artifact
against the implementation.

A standalone Rust crate defining a **bidirectional mapping** between a markdown
document (YAML frontmatter + body) and a typed data object, from a single compact
schema. It can **validate**, **extract** (parse → data), **render** (data →
markdown), **scaffold**, and **edit in-place**. The schema *looks like a skeleton
of the document it describes*. It is useful anywhere structured markdown is
authored or generated — runbooks, ADRs, postmortems, release notes, LLM output,
config-driven document sections.

This page is the overview; the rest of the reference is split into the focused
pages listed under [Read on](#read-on) (and in the sidebar of the rendered book).

---

## Implementation status

Legend: **yes** = works as specified · **partial** = works with a noted
limitation · **planned** = designed, not built.

| Spec feature | Status | Notes |
| --- | --- | --- |
| Body DSL parser: markers `#{1..6}` `-` `1.` `- [ ]` `>`, glued `card?@name?`, literal/`/regex/`/quoted labels, `<? ?>` descriptions ([Grammar](./spec/grammar.md)) | yes | regexes compiled and validated at parse time — an invalid pattern is a schema error |
| Headings nest **by level**; lists/prose nest **by indentation** ([Body structure](./spec/grammar.md#body-structure)) | yes | a flat `#`/`##` schema matches a nested document |
| `%ordered` ([Resolved defaults](./spec/defaults.md)) | yes | default true; a per-scope cursor enforces declared order |
| `%strict` — unexpected blocks are errors ([Resolved defaults](./spec/defaults.md)) | yes | default true; `%strict = false` ignores extras |
| `%frontmatter = open \| closed` ([Resolved defaults](./spec/defaults.md)) | yes | default closed; an undeclared key is a problem unless `open` |
| Cardinality `+ * ? {m} {m,} {m,n}` ([Body structure](./spec/grammar.md#body-structure)) | yes | bounds item/section counts; drives array-vs-scalar extraction |
| Auto-derived heading aliases ([Body structure](./spec/grammar.md#body-structure)) | partial | slug maps every non-ASCII-alphanumeric char to `_` without collapsing runs (`Trade-offs: speed` → `trade_offs__speed`); non-ASCII titles can slug to `""` |
| Scope-unique `@name` enforcement ([Body structure](./spec/grammar.md#body-structure)) | yes | duplicate aliases (and colliding auto-slugs) are a parse error |
| Frontmatter **grammar** `key?[@alias]: type` ([Frontmatter schema](./spec/grammar.md#frontmatter-schema)) | yes | drives validate/extract/render/scaffold |
| Frontmatter **validation & extraction** — typing, required keys, closed set ([Frontmatter schema](./spec/grammar.md#frontmatter-schema), [the data model](./spec/data-model.md)) | yes | `int`→number, `bool`→bool, `[T]`→array, others→string; a small YAML subset (scalars, quoted scalars, flow lists) |
| Extraction shapes: heading→object, list→array/scalar/objects, checklist→`{text,checked}`, prose→string, regex-title→`"title"` ([the data model](./spec/data-model.md)) | yes | a unified matcher gives all three verbs one per-scope cursor |
| `render` + `RenderError::{MissingField, WrongType, InvalidFrontmatter}` ([Public API](./spec/api.md)) | yes | inverse of `extract` for plain-text values; escapes metacharacters, re-validates its output |
| `scaffold` ([Scaffolding](./spec/operations.md#scaffolding)) | partial | validates for schemas without a regex-titled/empty-required-prose root; a regex title has no satisfying placeholder |
| `edit` by dotted path with numeric indexes; bare-unique-alias first segment ([Edit in-place](./spec/operations.md#edit-in-place)) | yes | full item span spliced and escaped; list items and prose only |
| Escaping: `\x` literal anywhere, position-only specials, quotes ([Escaping](./spec/grammar.md#escaping)) | yes | quotes coexist with `<? ?>`; `\?>`/`\\` unescape inside a description |
| `SchemaError { line, col, message }` ([Public API](./spec/api.md)) | yes | `col` is the marker column for body-node errors, `1` for frontmatter/directive errors |
| `Problem { path, message, span }` ([Public API](./spec/api.md)) | yes | `span` carries the offending block's byte range when one can be pinpointed |
| Public `Span`; `Regexp` compiled-regex type ([the data model](./spec/data-model.md), [Public API](./spec/api.md)) | yes | `Match::Regex(Regexp)`, `FieldType::Regex(Regexp)` |
| Bare-alias addressing + name→node resolution for `edit` ([Edit in-place](./spec/operations.md#edit-in-place)) | yes | first path segment may be a schema-wide-unique alias |
| Node `<? … ?>` descriptions in error messages ([Descriptions](./spec/defaults.md#descriptions)) | yes | appended to validation problem messages |
| Nested frontmatter list types `[[T]]` ([Frontmatter schema](./spec/grammar.md#frontmatter-schema)) | yes | recursive; text after the matching `]` (e.g. `[string]+`) is a **parse error** |
| Bracket index syntax `steps[1]` ([Edit in-place](./spec/operations.md#edit-in-place)) | planned | dotted numeric segments only (`steps.1`) |
| `Captured` struct in the public API ([the data model](./spec/data-model.md)) | planned | not a public type — `Problem.span` + public `Span` cover source locations |
| `query` (jq) verb | dropped | run your own queries over `extract`'s JSON |
| `*` / `*+` freeform wildcard marker ([Markdown coverage](./spec/grammar.md#markdown-coverage)) | planned | not a recognized marker; the parser rejects a leading `*` |
| Inline-vocabulary markers: tables, code fences, blockquotes ([Markdown coverage](./spec/grammar.md#markdown-coverage)) | planned | out-of-vocabulary blocks are opaque (flagged under `%strict`) |

One structural fact that shapes how you write schemas:

- **Schema headings nest by level, documents nest by heading level — they
  agree.** A schema writing `#` and an unindented `##` declares the `##` as a
  *child* of the `#`, exactly as a document nests its H2 under its H1. Indenting
  sub-headings is optional and only cosmetic. (Lists and prose still nest by
  indentation.)

And one caveat that remains:

- **Two adjacent same-marker lists in one scope are rejected at parse time.**
  Consecutive `- ` items are one CommonMark list, so a second sibling `- ` node
  could never match; `parse_schema` returns a `SchemaError` rather than deferring
  to a confusing validation failure. Separate the lists with a heading or prose,
  or use a different marker (`-` vs `1.`).

---

## Capabilities

One schema defines a **bidirectional mapping** between a markdown document and a
typed data object (think *serde, for documents*). From it:

1. **Validate** — does a document conform? Returns [`Problem`]s carrying an
   alias-path breadcrumb, a message (enriched with the node's `<? … ?>`
   description), and a source `span` when one can be pinpointed.
2. **Extract** (parse) — markdown → typed **data object** (JSON), keyed by the
   schema's capture **aliases**. Frontmatter values are typed
   (int/bool/list/string); body values are strings (trimmed, inline markup
   flattened).
3. **Render** (generate) — data object → markdown. Take a template (the
   schema) + variables (the data) and produce a conforming file.
   Metacharacters are escaped and the output is re-validated.
4. **Scaffold** — `render` specialized to placeholder/default values: a starter
   document.
5. **Query** — *(dropped)* originally a jq-style selector over the extracted
   object. Removed to keep the dependency tree lean; document-level navigation,
   if reintroduced, would go through capture aliases/paths (see Edit) rather than
   a bundled jq engine.
6. **Edit in-place** — `render` specialized to *one* node: resolve a capture
   alias or path → its source span → splice the new value, byte-accurately,
   leaving everything else untouched. (The "LLM updates a list item with one
   command, 100% accuracy" case — no hand-rolled `sed`, no full-file rewrite.)

**Extract and render are inverses.** For a schema whose captured string values
are plain text, `extract(render(data)) == data` and `render(extract(md))`
re-validates, with these documented caveats:

> - Captured text is **trimmed** — leading/trailing whitespace is not preserved,
>   so the fixpoint holds for already-trimmed values.
> - Inline markup a document already contains (emphasis, links, code spans) is
>   **flattened to text** on extract; `render` cannot reconstruct the original
>   markup.
> - An ordered list re-renders every item as `1.` (CommonMark renumbers on
>   display) — valid and round-tripping, not byte-identical to a `1. 2. 3.`
>   source.
> - A schema with two adjacent same-marker lists in one scope is rejected at
>   parse time (above), so it never reaches a round trip.

Frontmatter round-trips: `render` emits the supported YAML subset and `extract`
reads it back, so a required frontmatter key no longer breaks `render(extract(md))`.
Checklist `[x]`/`[ ]` state survives extract→render. Labeled lists do not double
their label (extract strips the label, render re-adds it once).

Scaffold and edit are render restricted to placeholders / a single node. (2)–(6)
rest on two foundations: scope-unique capture aliases (enforced at parse time)
and source spans on matched nodes (surfaced via `Problem.span` and used by
`edit`).

**Non-goals:** not a *general* markdown renderer (no markdown→HTML, no arbitrary
CommonMark transforms — it only renders *its own schema* from data); not a
programming language (only cardinality and regex labels); does not own a host
application's reserved-key / relation / ID logic.

---

## Read on

The rest of the reference (use the sidebar in the rendered book, or these links):

- [The DSL at a glance](./spec/at-a-glance.md) — the shape of a schema in one
  screen
- [Grammar](./spec/grammar.md) — overall shape, frontmatter, body structure,
  markdown coverage, escaping
- [Resolved defaults & descriptions](./spec/defaults.md) —
  ordering/strictness/frontmatter, and `<? … ?>`
- [The data model](./spec/data-model.md) — what `extract` produces, and the
  public types
- [Operations](./spec/operations.md) — edit in-place, scaffolding, markdown
  parsing
- [Public API](./spec/api.md) — the API surface, embedding, open decisions,
  phasing

For one schema exercised end to end through all five operations, see the
[worked example](./marklens-reference.md).
