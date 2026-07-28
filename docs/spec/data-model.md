# The data model

Parsing a **conforming** document against the schema yields a JSON value, keyed
by capture aliases, ready for `serde_json` consumers.

- **Frontmatter fields** appear at the top level under their aliases, typed
  ([Frontmatter schema](./grammar.md#frontmatter-schema)): int→number,
  bool→bool, `[T]`→array, the rest→string.
- A named **heading** → an object of its captured children. A **regex-titled**
  heading also captures its heading text under `"title"` (literal-titled
  headings do not). A **repeated** heading (`+`/`*`/`{m,n}`) → an array of such
  objects.
- A named **list with child nodes** → an array of objects, each with the item's
  lead text under `"text"` plus the child aliases; an absent optional child is
  `null`, not omitted.
- A named **childless list**: `+`/`*`/`{m,n}` → an array of strings; **bare/`?`**
  → a single string (the first item's text, with a literal label prefix stripped:
  `- @docs Docs:` on `- Docs: README updated` → `"README updated"`).
- A **checklist** → an array of `{ "text", "checked" }` objects (with any child
  aliases merged in), whatever its cardinality.
- **Prose** (`>`) → the paragraph's text as a string.
- Uncaptured nodes (regex-titled heading / list / prose without `@name`) are
  validated but produce no key; they appear in [`Problem`] paths as `"block"`.

Captured body text is trimmed and inline markup flattened
([Markdown coverage](./grammar.md#markdown-coverage)). A complete
worked example with exact output lives in the
[worked example](../marklens-reference.md).

Public types (see the crate for the full model; `name`/`card`/`desc` are grouped
in a `Head` struct, and regexes are the compiled [`Regexp`] type):

```rust
pub struct Schema { pub opts: SchemaOpts, pub frontmatter: Vec<FieldSchema>, pub body: Vec<Node> }

pub enum Node {
    Heading { level: u8, title: Match, head: Head, children: Vec<Node> },
    List    { style: ListStyle, item: Option<Match>, head: Head, children: Vec<Node> },
    Prose   { text: Option<Match>, head: Head },
}
pub struct Head { pub name: Option<String>, pub card: Card, pub desc: Option<String> }
pub enum ListStyle { Bullet, Ordered, Checklist }
pub enum Match { Literal(String), Regex(Regexp) }
pub enum Card  { Required, Optional, Star, Plus, Range(u32, Option<u32>) }

pub enum FieldType { Str, Int, Bool, Date, Enum(Vec<String>), List(Box<FieldType>), Regex(Regexp) }
```

There is no public `Captured { value, span }` type. Source locations come from
[`Problem::span`] and the public [`Span`], and `edit` uses the same matched spans
internally.
