# marklens — reference (worked example)

One schema exercised end to end: validate, extract, render, edit, scaffold.
Companion to the [design spec](./structure-dsl-spec.md).

Every artifact in these pages — validation results, JSON, edit output, scaffold
— is the **actual output** of marklens run against the schema and document shown,
verified against the crate. Where a shape is surprising (checklist objects, key
order, the frontmatter/regex-title `title` collision) it is called out. Genuinely
unimplemented features (`query`, `*` wildcard, table/code-fence markers) are
listed in [Not implemented](./reference/coverage.md#not-implemented), not silently
demonstrated.

This page holds the schema everything else refers to; the individual operations
are on the pages listed under [The walk-through](#the-walk-through) (and in the
sidebar of the rendered book).

---

## The schema

A feature runbook: typed frontmatter, an H1 with nested H2/H3 sections, every
list kind, a checklist, prose, a regex-titled repeated subsection, and a labeled
bullet.

```markdown
%ordered = true

---
title:  string                   <? human-readable feature title ?>
status: enum(draft, in_review, shipped)
sprint: int
owners: [string]
tracking? @issue: /^[A-Z]+-[0-9]+$/
---

# @feature /.+/                  <? H1, any text; captured under "title" ?>
  ## @summary Summary
    > @blurb                     <? one-paragraph description ?>

  ## @rollout Rollout
    ### @flags Feature flags
      - +@keys                   <? bullet list, one or more ?>
    ### @steps Deploy steps
      1. +@ordered               <? ordered, reproducible steps ?>
        - ?@rollback             <? optional rollback note per step ?>

  ## @checks Verification
    - [ ] +@cases                <? checklist, one or more ?>

  ## @decisions Decisions
    ### +@entries /.+/           <? repeated subsection, any title ?>
      > @rationale /^(Accept|Reject)/

  ## @signoff Sign-off
    - @approver Approved by:     <? single labeled bullet; value = text after label ?>
```

Two authoring facts this schema relies on:

- **Headings nest by level; lists and prose nest by indentation.** The `##`
  lines are written indented under `# @feature`, which reads well, but the
  indentation is optional for headings — `## @summary Summary` at column 0 nests
  under the H1 just the same, because a heading is a child of the nearest
  *lower-level* heading. Lists and prose, by contrast, nest strictly by
  indentation (the `- ?@rollback` bullet is a child of the `1. +@ordered` step
  because it is indented under it).
- **Frontmatter is typed, validated, and captured.** `title`/`status`/`sprint`/
  `owners` are required; `tracking` is optional and renamed to the alias
  `issue`. The set is closed by default, so an undeclared frontmatter key is a
  validation problem (add `%frontmatter = open` to allow extras). `owners:
  [string]` is a list; a trailing cardinality such as `[string]+` is now a
  **parse error**, not a silently dropped `+`.

One caveat this schema avoids on purpose: two adjacent same-marker lists in one
scope. Consecutive `- ` bullets are a single CommonMark list, so a second
sibling `- ` node could never match — `parse_schema` rejects such a schema with
a `SchemaError`. Separate the lists with a heading (as `Feature flags` /
`Deploy steps` are) or use a different marker.

## Descriptions and escaping

All parse as annotated:

```markdown
## Trade-offs: speed -- vs -- safety   <? '--' is fine: the desc is fenced by <? ?> ?>
## C++ and user@host and a/b           <? +, @, / are literal mid-label ?>
- \+must-start-with-plus               <? escape a LEADING head char ?>
## \/etc path                          <? escape a leading '/' so it is not a regex ?>
## a \<? b                             <? \<? is a literal '<?' in the title ?>
## "+/- tolerance; quoted"             <? quotes escape a leading '/', '+', '@' ?>
```

Results (title / description):

- `## Trade-offs: speed -- vs -- safety <? … ?>` → title `Trade-offs: speed -- vs
  -- safety`, description `'--' is fine: the desc is fenced`. The `<? … ?>`
  fence is stripped first, so `--`/`—` in the label need no escaping.
- `## C++ and user@host and a/b` → title `C++ and user@host and a/b` (no
  description). `+`, `@`, `/` are only special in the head/leading position.
- `- \+must-start-with-plus` → a bullet with card `Required` and literal item
  `+must-start-with-plus` (the `\+` is an escaped literal, not a `+` card).
- `## \/etc path` → title `/etc path` (the leading `/` is escaped, so it is a
  literal, not a regex).
- `## a \<? b` → title `a <? b` (`\<?` is a literal `<?`; no description).
- `## "+/- tolerance; quoted"` → title `+/- tolerance; quoted` (quotes protect a
  leading `/`, `+`, or `@` and are stripped).

Two subtler cases:

- Quotes coexist with a description. `## "weird +/- title" <? a note ?>` parses
  as title `weird +/- title`, description `a note` — the `<? … ?>` scan runs
  first and pulls the description out cleanly, then the remaining `"…"` is
  unquoted. An *interior* `<? … ?>` is likewise extracted:
  `## "pre <?mid?> post"` → title `pre  post`, description `mid`.
- `\?>` inside a description **unescapes** to `?>`, and the description continues
  past it: `## t <? use \?> literally ?>` → description `use ?> literally`. `\\`
  in a description unescapes to a single `\`. (A description runs to the closing
  `?>`, or to end of line if unclosed.)

---

## The walk-through

The same schema, run through every operation (one page each — use the sidebar in
the rendered book, or these links):

- [A conforming document](./reference/document.md) — a document that validates
  clean
- [Extracted data](./reference/extract.md) — the exact JSON `extract` returns,
  with conventions
- [Render](./reference/render.md) — data → markdown, the inverse of extract
- [Operations](./reference/operations.md) — a byte-accurate `edit`, and
  `scaffold` output
- [Coverage & gaps](./reference/coverage.md) — a feature map and what is not
  implemented
