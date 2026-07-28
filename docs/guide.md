# Guide

marklens turns a Markdown document into typed data — and back — using a schema
that looks like the document itself.

If you've ever written code to pull fields out of Markdown (release notes,
runbooks, ADRs, an LLM's output), or hand-checked that some Markdown has the
right shape, this replaces that. You write one small schema, and you get five
operations from it.

## One example, start to finish

Here's a slice of a feature doc:

```markdown
## Summary
Adds a dark theme toggle.

## Steps
1. Ship the flag off
2. Enable for staff
```

You want the data out of it. Write a schema that mirrors the document, marking
the parts you care about with `@names`:

```markdown
## @summary Summary
  > @blurb
## @steps Steps
  1. +@items
```

Read that as: an H2 titled *Summary* with a paragraph under it (`>`), then an H2
titled *Steps* with a numbered list. Now `extract` gives you exactly this:

```json
{
  "steps": {
    "items": [
      "Ship the flag off",
      "Enable for staff"
    ]
  },
  "summary": {
    "blurb": "Adds a dark theme toggle."
  }
}
```

That's the whole idea. The schema is a skeleton of the document; the `@names` are
the keys you get back. You never wrote a parser.

## How to read a schema

Each line is one block, and it starts with a marker — the same markers you
already use in Markdown:

| You write | It matches |
| --- | --- |
| `#`, `##`, `###` … | a heading (by level) |
| `-` | a bullet |
| `1.` | a numbered item |
| `- [ ]` | a checkbox |
| `>` | a paragraph |

After the marker come two optional things:

- **`@name`** — the key this block is captured under. `@summary` → `"summary"`.
  No name means "match this, but don't capture it."
- **A count** — `+` is one-or-more, `?` is optional, plain is exactly one.
  So `+@items` is "one or more items, collected into a list."

The rest of the line is the **label**: the literal text to match (`Summary`), or
a `/regex/` for "any text — and capture it" (as in `# @title /.+/`). Indentation
nests one block under another, exactly as it reads.

That's most of the language. The [spec](./structure-dsl-spec.md) has the rest —
frontmatter typing, escaping, the full grammar — but you rarely need it to start.

## The five things you can do

All five come from that one schema:

**validate** — does a document match? You get back a list of problems, each with
a path to what's wrong and where in the file. Nothing matches unexpectedly and
nothing required is missing.

**extract** — pull the document into typed JSON (the example above). Frontmatter
values keep their types — numbers, booleans, lists — and body text comes out as
strings.

**render** — the reverse. Hand it JSON shaped like `extract`'s output and it
writes the Markdown back. `extract` and `render` are inverses: extract a doc,
render it, and you get the same doc.

**scaffold** — a blank starter document, straight from the schema:

```markdown
## Summary

TODO

## Steps

1. 
```

**edit** — change one value in place, surgically. To rewrite the second step:

```sh
marklens edit schema doc.md items.1 "Enable for everyone"
```

```markdown
## Steps
1. Ship the flag off
2. Enable for everyone
```

Only that line changes; every other byte of the file is left exactly as it was.
This is the "let an LLM update one list item without rewriting the whole file"
case — no `sed`, no full-file regeneration.

## Why the schema looks like the document

Most tools make you learn a query language and describe your document from the
outside. marklens flips it: you sketch the document you already have, and the
sketch *is* the schema. If you can write the Markdown, you can write the schema.

## Where to go next

- **[Worked example](./marklens-reference.md)** — one real, larger schema (typed
  frontmatter, nested sections, a checklist, a repeated section) run through all
  five operations, with the exact output of each.
- **[Language & API spec](./structure-dsl-spec.md)** — the precise rules: every
  marker and count, escaping, the frontmatter grammar, the data model, and the
  Rust API. Reach for it when you need the fine print.
