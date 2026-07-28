# Render

`render` walks the schema and emits markdown from a JSON object shaped exactly
like `extract`'s output — the two are inverses. `render` escapes markdown
metacharacters and separates blocks, and always re-validates its own output.

On a compact body-only schema:

```rust
let schema = parse_schema(
    "## @summary Summary\n  > @blurb\n## @plan Test plan\n  - [ ] +@cases\n",
)?;
let data = json!({
    "summary": { "blurb": "Adds detached-head support." },
    "plan": { "cases": [
        { "text": "sync from a detached head", "checked": true },
        { "text": "refuse when dirty",         "checked": false },
    ] }
});
let md = schema.render(&data)?;
// "## Summary\n\nAdds detached-head support.\n\n## Test plan\n\n\
//  - [x] sync from a detached head\n- [ ] refuse when dirty\n"
assert!(schema.validate(&md).is_empty());
assert_eq!(schema.extract(&md)?, data);           // round-trip holds
```

The full [conforming document](./document.md) schema round-trips too:
`render(extract(doc))` re-validates and re-extracts to the same object. Checklist
state survives (`- [x]` stays checked), the `Approved by:` label is re-added
exactly once (no doubling), and metacharacters are escaped and recovered —
`render`ing `["*bold*", "a `code` b", "1. not a list", "# not a heading"]`
produces `- \*bold\*` / `- a \`code\` b` / `- 1\. not a list` /
`- \# not a heading`, which re-extracts to the original strings.

Caveats:

- **Whitespace is trimmed, not preserved.** A value like `"  padded  "` extracts
  as `"padded"`, so `extract(render(x)) == x` holds for plain-text values only
  after trimming — the documented contract is "no leading/trailing whitespace".
- **An ordered list re-emits every item as `1.`** (CommonMark renumbers on
  display). The output is valid and round-trips; it is just not byte-identical to
  a hand-authored `1. 2. 3.` source.
- A missing required field (body node or frontmatter key) is a
  `RenderError::MissingField`; a JSON value of the wrong type is a
  `RenderError::WrongType`; a frontmatter value violating its declared type
  (e.g. an `enum` value not in the set) is a `RenderError::InvalidFrontmatter`.
