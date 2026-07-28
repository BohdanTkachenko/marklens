# Operations

## Edit in-place

`edit` splices exactly the byte span that `validate`/`extract` matched, leaving
every other byte untouched. The `target` is a dotted path mirroring `extract`'s
shape — object keys are aliases, array elements are numeric indexes — and the
**first** segment may be a bare alias that is unique across the whole schema. So
`ordered` (the deploy-steps list, nested three levels deep) can be addressed
directly:

```rust
let new_doc = schema.edit(doc, "ordered.1", "Enable for all users")?;
```

Before / after — only the addressed item's text changes; the nested rollback
note, checkbox states, frontmatter, and every other byte are preserved, and the
result still validates (`schema.validate(&new_doc)` → `[]`):

```markdown
### Deploy steps
1. Ship the flag disabled
2. Enable for staff
   - flip the flag back off
```

```markdown
### Deploy steps
1. Ship the flag disabled
2. Enable for all users
   - flip the flag back off
```

Edit contract, as implemented:

- **Paths mirror `extract` output.** The full path here is
  `feature.rollout.steps.ordered.1`; because `ordered` is schema-wide unique,
  `ordered.1` resolves to it. For an item *with* nested children, `ordered.1`
  edits what `extract` exposes as `ordered[1].text` — `ordered.1.text` is **not**
  a valid path. The nested note is addressed by index even though `extract`
  exposes it as a plain string: `ordered.1.rollback.0` replaces
  `flip the flag back off`, while `ordered.1.rollback` (no index) returns
  `TargetNotFound`.
- **Only leaf list items and prose paragraphs are editable** — not headings, not
  frontmatter keys. Prose is addressed by its alias (`edit(doc, "blurb", …)`).
- **The marker and any checkbox prefix are preserved**, and `new_value` is
  **escaped** so it round-trips back through `extract` unchanged. The splice is
  the full item span, so an item containing inline code or markup is replaced
  correctly (no partial-run corruption). Editing a checklist item keeps its
  `[x]`/`[ ]`.
- **Failures are typed.** A path that names nothing editable →
  `EditError::TargetNotFound`; a numeric index past the end →
  `EditError::IndexOutOfRange { index, len }`; a `new_value` containing a line
  break → `EditError::InvalidValue`.
- **Bracket paths (`ordered[1]`) are not implemented** — use dotted `ordered.1`.
- Resolution is positional within a scope: the document should conform to the
  schema (optional nodes present in schema order) so an index lands on the
  intended item.

## Scaffold

`schema.scaffold()` for the [schema](../marklens-reference.md#the-schema) —
exact output (several list lines end in a trailing space after the marker;
prose scaffolds as `TODO`):

```markdown
---
title: ""
status: draft
sprint: 0
owners: []
---

# 

## Summary

TODO

## Rollout

### Feature flags

- 

### Deploy steps

1. 

## Verification

- [ ] 

## Decisions

### 

TODO

## Sign-off

- Approved by: 
```

Required frontmatter keys get typed placeholders (`""` for string, `0` for int,
first value for enum, `[]` for list); optional nodes (`tracking`, `?@refs`,
`?@rollback`) are omitted; regex titles scaffold as empty text.

**This scaffold does not validate against its own schema**, because the schema
has a regex-titled root: the empty `# ` fails `/.+/`, and the empty regex-titled
`### ` subsection and the placeholder `- [ ]`/prose lines fail their nodes.
`schema.validate(schema.scaffold())` returns, among others,
`feature: expected at least one section, found 0`. Treat this scaffold as a
human starting point.

A schema **without** a regex-titled root, empty-required prose, or a checklist
does scaffold to a conforming document. For example, a purely literal-heading
schema with a `- +@items` list scaffolds to markdown that `validate(scaffold())`
accepts with no problems.
