# Extracted data object

`schema.extract(doc)` — exact output, pretty-printed. `serde_json` orders object
keys alphabetically:

```json
{
  "feature": {
    "checks": {
      "cases": [
        {
          "checked": true,
          "text": "Toggle switches the theme"
        },
        {
          "checked": false,
          "text": "Preference survives a reload"
        }
      ]
    },
    "decisions": {
      "entries": [
        {
          "rationale": "Accept. It follows the user across devices.",
          "title": "Store the preference server-side"
        }
      ]
    },
    "rollout": {
      "flags": {
        "keys": [
          "dashboard_dark_mode",
          "theme_persistence"
        ]
      },
      "steps": {
        "ordered": [
          {
            "rollback": null,
            "text": "Ship the flag disabled"
          },
          {
            "rollback": "flip the flag back off",
            "text": "Enable for staff"
          }
        ]
      }
    },
    "signoff": {
      "approver": "design review"
    },
    "summary": {
      "blurb": "Adds a dark theme toggle that persists across sessions."
    },
    "title": "Dark mode for the dashboard"
  },
  "issue": "WEB-1234",
  "owners": [
    "alice",
    "bob"
  ],
  "sprint": 42,
  "status": "in_review",
  "title": "Dark mode for the dashboard"
}
```

## Extraction conventions

- **Frontmatter keys sit at the top level**, keyed by alias, with their declared
  types: `sprint` is a JSON number (`42`, not `"42"`), `owners` is an array,
  `tracking` is captured under its alias `issue`. `title` appears **twice**: as
  the top-level frontmatter `title` string, and again inside `feature` — a
  regex-titled heading also captures its own title under `"title"`, and this H1
  matches the same text. Only regex-titled headings get a `"title"` key;
  literal-titled headings do not.
- A named **heading** → an object of its captured children. `feature` nests the
  whole body because every `##` is a level-2 child of the H1.
- A named **list with child nodes** → an array of objects, each with the item's
  lead text under `"text"` plus the child aliases; an absent optional child is
  `null`, not omitted (see `steps[0].rollback`).
- A named **childless list** with `+`/`*`/`{m,n}` → an array of strings
  (`flags.keys`). With **bare/`?`** cardinality it is a *single string* — the
  first item's text (`signoff.approver`: the `Approved by:` label is stripped,
  leaving `"design review"`).
- A **checklist** → an array of `{ "text", "checked" }` objects, whatever
  child nodes it has; the `[x]`/`[ ]` state is captured under `"checked"`.
- **Prose** (`>`) → the paragraph's text as a string.
- Repeated headings (`+`/`*`/`{m,n}`) → an array of objects; `decisions.entries`
  is a one-element array because there is one `###` subsection.
- Captured text is **trimmed** (leading/trailing whitespace is not preserved).
  Inline markup is **flattened**: the item
  ``- see [the spec](https://example.com) and **bold** and `code` `` extracts as
  `"see the spec and bold and code"` — link URLs, emphasis markers, and
  code-span backticks are gone. A `/regex/` label therefore matches this
  flattened text, so a pattern like `/^\[.+\]\(.+\)$/` can never match a
  markdown link.
