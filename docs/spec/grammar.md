# Grammar

A schema has three parts: optional `%`-directives, an optional frontmatter
block, then the body of headings and lists. This page covers each; you can write
most schemas from the [Guide](../guide.md) alone and reach here for the corners.

## Overall shape

```ebnf
schema      = directive* frontmatter? body
directive   = "%" key "=" value NEWLINE         # schema-level options (see Resolved defaults)
frontmatter = "---" NEWLINE fm-field* "---" NEWLINE
body        = node*
```

## Frontmatter schema

```ebnf
fm-field = key "?"? ("@" name)? ":" SP fm-type (SP desc)? NEWLINE
fm-type  = "string" | "int" | "bool" | "date"
         | "enum(" ident ("," SP? ident)* ")"
         | "[" fm-type "]"          # list of T (recursive: [[T]] is allowed)
         | "/" regex "/"            # string matching regex
desc     = "<?" text "?>"           # delimited description / comment
```

- `key?` ⇒ optional key; otherwise required.
- The alias **defaults to the key**; an explicit `@name` (after the optional `?`)
  renames it (`spec_url? @spec: /^https/`).
- Frontmatter is **closed by default** (unknown keys are errors);
  `%frontmatter = open` allows extras.

`validate` and `extract` check each key against its type, enforce required keys,
and (unless `open`) reject undeclared keys. Each value is coerced by type: `int`
→ JSON number, `bool` → JSON bool, `date` → a `YYYY-MM-DD` string (month/day
ranges checked), `[T]` → array, and `enum`/`/regex/`/`string` → string. `render`
emits the same values back.

The frontmatter is a small YAML subset — only what `render` can reproduce, so
the block round-trips: `key: scalar` lines, double-quoted scalars (with
`\n \t \" \\`), and flow lists `[a, b, "c, d"]`. Nested maps and block sequences
(`- item` lines) are not supported. An empty scalar reads as `""` (or `[]` for a
list). `render` quotes a scalar only when leaving it bare would reparse
differently.

Nested list types like `[[T]]` work. Anything after the matching `]` is a parse
error — `[string]+` is rejected, not a silently dropped `+`.

## Body structure

Each node is one line — `marker card?@name? label? <? desc ?>?` — plus an
optionally indented child block. Right after the marker comes a **glued head**:
an optional `card`, then an optional `@name` (no space between them — `+@docs`).
A space then begins the optional `label` (heading title / item match), which runs
to the ` <?` description (or end of line).

```ebnf
node     = INDENT marker (card? ("@" name)?) (SP label)? (SP desc)? NEWLINE children?
children = (node, nested per the rule below)+

marker   = heading | bullet | ordered | checkbox | prose
heading  = "#"{1,6} SP                # level = count of '#'; title is the label
bullet   = "-"
ordered  = digits "."                # "1."
checkbox = "- [ ]"
prose    = ">"                        # a non-empty paragraph

card     = "+" | "*" | "?" | "{" int ("," int?)? "}"   # leads the head, after the marker

label    = "/" regex "/"              # regex if it starts with '/'
         | text                       # …else a bare literal (text must start with it)
         | '"' literal '"'            # quotes OPTIONAL — only to escape a leading '/'
                                      #   or '+'/'@', or preserve exact whitespace
```

So `## +@docs Docs:` = card `+`, name `docs`, literal label `Docs:`; `## + Docs:`
is the same without a name; `## @docs Docs:` without the card.

**Nesting.** Headings nest by level: a heading is a child of the nearest
preceding heading of a smaller level. So a flat `# … ## …` schema makes the `##`
a child of the `#`, and matches a document that nests its H2 under its H1 —
heading indentation is ignored. Lists and prose nest by indentation: a node is a
child of the most recent node with a smaller leading-column count (tabs expand to
the next 4-column stop). Indenting a line under a `>` prose node is a parse error
— prose has no children.

**Cardinality** (item count on lists; matching-section count on headings;
presence on prose):

| Suffix | Meaning | Extraction of a heading / childless list |
| --- | --- | --- |
| (none) | required (a list ⇒ ≥1 item) | single value (scalar for a childless list) |
| `?` | optional (≤1) | single value / `null` if absent |
| `+` | ≥1 | array |
| `*` | ≥0 | array |
| `{m}` `{m,}` `{m,n}` | explicit bounds | array (even `{1}`) |

`## +@entries /.+/` = one or more headings at that level (a repeated subsection).
A child block under a **list** constrains *each item*. A list *with* child nodes
always extracts as an array of objects regardless of cardinality; a checklist
always extracts as an array of `{text, checked}` objects.

**Annotations.** `@name` (right after the card) is a capture alias
(`[a-z][a-z0-9_]*`), unique within its scope, attachable to any block. The alias
— not the heading text — is what extraction and edits address, so renaming a
heading never breaks them.

A literal-titled heading with no `@name` gets an auto-alias: the slug of its
title (`Manual verification` → `manual_verification`). Regex-titled headings,
lists, and prose without a name are matched but not captured. The slug rule:
lowercase ASCII, every non-alphanumeric character becomes `_` (runs are not
collapsed, so `Trade-offs: speed` → `trade_offs__speed`), then trim leading and
trailing `_`. A non-ASCII title can slug to `""`.

`<? text ?>` is a **delimited** description: because it is fenced, the label may
contain any characters (including `--`, `—`), and a `<? … ?>` on its own line is
an ignored comment. A node's description is appended to any validation problem it
produces ([Descriptions](./defaults.md#descriptions)).

## Markdown coverage

Two layers, kept separate:

- **The document** may use full CommonMark; marklens never rejects content
  outright, but the parser ([Markdown parsing](./operations.md#markdown-parsing),
  comrak) runs with GFM extensions **off**, so
  GFM-only constructs read as plain CommonMark: a table becomes an ordinary
  paragraph and a footnote reference stays literal text.
- **The schema vocabulary** models *block structure*: headings, the three list
  kinds, and paragraphs (`>`). That is what you can *assert and capture*.
  **Inline** content is matched and extracted as *flattened text*: comrak's
  inline tree is collapsed to its text, so emphasis markers, code-span
  backticks, and link/image URLs are **removed** before label matching and
  extraction (raw inline HTML like `<uuid>` survives verbatim). A `/regex/`
  label therefore constrains flattened text — a pattern targeting markdown
  syntax such as `/^\[.+\]\(.+\)$/` can never match a link.

Block types the schema has no marker for — fenced code, blockquotes, thematic
breaks, HTML blocks, and (extensions being off) GFM tables/images degraded to
paragraphs — are **opaque**. Under `%strict` (the default) an opaque block no
node claims is reported as an unexpected block; under `%strict = false` it is
ignored. A degraded table/image can be *claimed* by a `>` prose node and so
shadow a later real paragraph.

**Planned vocabulary extensions** (none implemented), so the schema can assert
richer blocks:

| Marker (proposed) | Asserts |
| --- | --- |
| `\| col \| col \|` | a GFM table with these columns |
| ` ```lang ` | a fenced code block (optional language) |
| `>> ` | a blockquote (its own child block) |
| `*` / `*+` | a freeform wildcard — *any* block(s) here (an escape hatch for `%strict` schemas) |

Until then, model rich/freeform regions with `>` (paragraph); everything the
document contains still round-trips through edit because edit splices bytes
([Edit in-place](./operations.md#edit-in-place)).

## Escaping

Special tokens are special **only in the position that gives them meaning**;
elsewhere they are literal, so most labels need no escaping:

- `card` (`+ * ? { }`) and `@name` matter only in the **head** (right after the
  marker). `## C++ trade-offs`, `## ping user@host`, `## a {set}` are all
  literal — the `+`/`@`/`{` are mid-label.
- `/` starts a **regex** only as the **first non-space char of the label**;
  `## and/or` is literal.
- `<?` starts a **description** anywhere on the line; `?>` (or end of line)
  closes it.

To force a literal where a token *would* be special, use either:

- **Backslash** — `\x` is a literal `x`, anywhere in a label. For a label that
  must *start* with a head/regex char, or that contains `<?`: `## \+plus`,
  `## \@handle`, `## \/etc`, `## a \<? b`. `\\` is a literal backslash.
- **Quotes** — wrap the whole label in `"…"` to escape a leading `/`, `+`, or
  `@`, or to preserve exact whitespace: `## "+/- tolerance; quoted"`.

Two finer points:

- Quotes **coexist** with a description. The `<? … ?>` scan runs first and
  removes the description, then the remaining `"…"` label is unquoted, so
  `## "weird +/- title" <? note ?>` → title `weird +/- title`, description
  `note`, and an interior `## "pre <?mid?> post"` → title `pre  post`,
  description `mid`.
- Inside a description, `\?>` **unescapes** to `?>` (the description continues
  past it) and `\\` unescapes to `\`. A description runs to its closing `?>`, or
  to end of line if unclosed.

Inside a **regex** `/…/`, `\/` is a literal slash (regex-native); `/pat/flags`
folds `flags` into a leading `(?flags)` group.
