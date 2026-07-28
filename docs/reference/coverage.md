# Feature coverage map

| Feature | Demonstrated by |
| --- | --- |
| `%ordered` directive (enforced) | top of [the schema](../marklens-reference.md#the-schema) |
| `%strict` (rejects unexpected blocks) | [A conforming document](./document.md) |
| Typed frontmatter `string`/`enum`/`int`/`[string]`/`/regex/`, optional `?`, alias `@issue` | [the schema](../marklens-reference.md#the-schema), extracted in [Extracted data](./extract.md) |
| Frontmatter validation + closed key set | [A conforming document](./document.md) |
| Heading levels 1/2/3, nesting **by level** | `# @feature` → `## Summary` → `### @flags` |
| Literal vs regex heading title | `## Summary` vs `# @feature /.+/` |
| Auto-derived alias | `## Summary` → `"summary"` |
| Regex-title capture (`"title"`) | `feature.title`, `decisions.entries[0].title` in [Extracted data](./extract.md) |
| Repeated subsection | `### +@entries /.+/` under Decisions |
| Bullet / ordered / checklist / prose | `- ` / `1. ` / `- [ ]` / `>` |
| Checklist `{text, checked}` capture | `checks.cases` in [Extracted data](./extract.md) |
| Labeled single bullet (scalar, label stripped) | `- @approver Approved by:` → `"design review"` |
| Regex label on prose | `> @rationale /^(Accept\|Reject)/` |
| Cardinality `+` / `?` / `{m,n}` | `+@keys` / `?@rollback` / (see spec) |
| Nesting: list-item → list | step → `rollback` |
| `<? … ?>` descriptions, escaping | [Descriptions and escaping](../marklens-reference.md#descriptions-and-escaping) |
| validate / extract / render / edit / scaffold | [document](./document.md)–[scaffold](./operations.md#scaffold) |

## Not implemented

Named in the design spec, still absent:

| Feature | Current behavior |
| --- | --- |
| `query` (jq selector) | dropped from the design; run your own queries over `extract`'s JSON |
| `*` / `*+` freeform wildcard marker | not implemented — a leading `*` is not a recognized marker |
| Table / code-fence / blockquote assertion markers | planned vocabulary, not implemented; model such regions with `>` prose |
| Bracket index paths (`ordered[1]`) | `TargetNotFound`; use dotted `ordered.1` |
| `[T]+` frontmatter list cardinality | parse error (not a silently dropped `+`) |
| `Captured` struct in the public API | not a public type; `Problem.span` and the public `Span` cover source locations instead |
| Descriptions surfaced in IDE hovers | not implemented (they *do* enrich validation problem messages — see the spec) |
