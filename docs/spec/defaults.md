# Resolved defaults

| Behavior | Default | Override | Enforced? |
| --- | --- | --- | --- |
| **Ordering** | **strict** — body nodes must appear in declared order | `%ordered = false` (any order) | **yes** |
| **Strictness** | **error on unexpected blocks** | per-node `?`/`*`, or `%strict = false` | **yes** |
| **Frontmatter** | **closed** — unknown keys error | `%frontmatter = open` | **yes** |
| **Markdown parser** | **established library** (see [Markdown parsing](./operations.md#markdown-parsing)) | — | yes (comrak) |

Under `%ordered`, each schema node scans document blocks from a per-scope cursor,
so an out-of-order section is reported as *missing*; `%ordered = false` scans each
node from the start of its scope. `%`-directives at the top of a schema set these
per-schema; a host application can also set them programmatically
(`SchemaOpts`).

## Descriptions

A node's `<? description ?>`:

- **Enriches errors:** a failing node appends its description to the problem
  message, e.g. `plan › cases: expected at least one item, found 0 — "one per
  behaviour"`.
- **Documents the schema** itself — it reads as annotated structure.
- **Powers IDE integration** — hover text and completion docs (planned, not yet
  built).

Descriptions are optional and never affect matching.
