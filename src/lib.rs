//! # marklens — markdown ⇄ data, via a template
//!
//! One compact schema (a textual DSL) defines a bidirectional mapping between a
//! markdown document and a data object. From it you can **validate**,
//! **extract** (markdown → data), **render** (data → markdown), **scaffold**
//! (schema → starter document), and **edit in-place**.
//!
//! ## The DSL at a glance
//!
//! ```markdown
//! %ordered = true
//! ---
//! title: string                   <? typed frontmatter, checked and captured ?>
//! status: enum(planned, done)
//! ---
//! > @summary                      <? a required paragraph, captured as "summary" ?>
//! ## @manual Manual verification  <? an H2 section; @name is the capture alias ?>
//!   ### @setup Setup              <? headings nest by level (indent is optional) ?>
//!     - +@items                   <? bullet list, one or more items ?>
//!   ### @procedure Procedure
//!     1. +@steps                  <? ordered list ?>
//!       - ?@note                  <? optional per-item child ?>
//! ## @plan Test plan
//!   - [ ] +@cases                 <? checklist ?>
//! ```
//!
//! Markers: `#`–`######` headings, `-` bullets, `1.` ordered, `- [ ]`
//! checklists, `>` prose. Cardinality: bare = required, `?` optional, `+` one
//! or more, `*` zero or more, `{m,n}` range. `@name` is the capture alias;
//! after it comes an optional label (a literal prefix or a `/regex/`).
//! **Headings nest by their level** — mirroring the documents they match — so a
//! sub-heading need not be indented (though indentation is allowed and reads
//! well). Lists and prose nest by indentation.
//!
//! ## Quickstart
//!
//! ```
//! use marklens_core::parse_schema;
//!
//! let schema = parse_schema(concat!(
//!     "## @plan Test plan\n",
//!     "  - [ ] +@cases\n",
//!     "## @notes Notes\n",
//!     "  > @summary\n",
//! ))?;
//!
//! let doc = concat!(
//!     "## Test plan\n",
//!     "- [x] smoke test\n",
//!     "- [ ] edge cases\n",
//!     "\n",
//!     "## Notes\n",
//!     "Everything green so far.\n",
//! );
//!
//! // A conforming document produces no problems.
//! assert!(schema.validate(doc).is_empty());
//!
//! // Extract the captures as JSON, keyed by alias. Checklist items carry their
//! // checked state.
//! let data = schema.extract(doc).expect("conforms");
//! assert_eq!(
//!     data["plan"]["cases"],
//!     serde_json::json!([
//!         { "text": "smoke test", "checked": true },
//!         { "text": "edge cases", "checked": false },
//!     ])
//! );
//! assert_eq!(data["notes"]["summary"], "Everything green so far.");
//! # Ok::<(), marklens_core::SchemaError>(())
//! ```
//!
//! ## Operations
//!
//! | Operation | Signature | What it does |
//! |---|---|---|
//! | [`Schema::validate`] | `(&str) -> Vec<Problem>` | check a document; empty = conforms |
//! | [`Schema::extract`] | `(&str) -> Result<Value, Vec<Problem>>` | markdown → JSON keyed by capture aliases |
//! | [`Schema::render`] | `(&Value) -> Result<String, RenderError>` | data → markdown |
//! | [`Schema::scaffold`] | `() -> String` | starter document with placeholders |
//! | [`Schema::edit`] | `(&str, &str, &str) -> Result<String, EditError>` | replace one node's text in place |
//!
//! ## Round-trip contract
//!
//! `extract` and `render` are designed as inverses: for a schema whose captured
//! string values are plain text (no leading/trailing whitespace),
//! `extract(render(data)) == data`, and `render(extract(md))` re-validates.
//! `edit` splices a single node's byte span, leaving every other byte
//! untouched, and escapes the replacement so the document still round-trips.
//!
//! Where this cannot hold, it is honest about it: values with leading/trailing
//! whitespace are trimmed on extract; a schema with two adjacent same-marker
//! lists in one scope is rejected at parse time (markdown would merge them); and
//! inline markup a document already contains (emphasis, links) is flattened to
//! its text on extract.
//!
//! ## Status (0.1.0)
//!
//! The API is `0.x` and may change. Block types outside the vocabulary (tables,
//! code fences, blockquotes) are opaque: under `%strict` (the default) they are
//! reported as unexpected blocks; under `%strict = false` they are ignored.
//!
//! ## Further reading
//!
//! In the repository: [`docs/structure-dsl-spec.md`] (the language reference)
//! and [`docs/marklens-reference.md`] (a worked example, every artifact verified
//! against the implementation).
//!
//! [`docs/structure-dsl-spec.md`]: https://github.com/BohdanTkachenko/marklens/blob/main/docs/structure-dsl-spec.md
//! [`docs/marklens-reference.md`]: https://github.com/BohdanTkachenko/marklens/blob/main/docs/marklens-reference.md

#![warn(missing_docs)]

mod doc;
mod edit;
mod error;
mod frontmatter;
mod matcher;
mod parse;
mod render;
mod schema;
mod validate;

pub use doc::Span;
pub use error::{EditError, Problem, RenderError, SchemaError};
pub use parse::parse_schema;
pub use schema::{
    Card, FieldSchema, FieldType, Head, ListStyle, Match, Node, Regexp, Schema, SchemaOpts,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(src: &str) -> Schema {
        parse_schema(src).expect("schema parses")
    }

    #[test]
    fn defaults_are_strict() {
        let s = parse("> @x");
        assert!(s.opts.ordered && s.opts.strict && !s.opts.frontmatter_open);
    }

    #[test]
    fn directives_override_defaults() {
        let s = parse("%ordered = false\n%strict = false\n%frontmatter = open\n> @x");
        assert!(!s.opts.ordered && !s.opts.strict && s.opts.frontmatter_open);
    }

    #[test]
    fn frontmatter_types_and_alias() {
        let s = parse(
            "---\n\
             title: string\n\
             status: enum(planned, done)\n\
             tags: [string]\n\
             owner?: string\n\
             spec_url? @spec: /^https/\n\
             ---\n",
        );
        let f = &s.frontmatter;
        assert_eq!(f.len(), 5);
        assert_eq!(f[0].key, "title");
        assert_eq!(f[0].ty, FieldType::Str);
        assert_eq!(
            f[1].ty,
            FieldType::Enum(vec!["planned".into(), "done".into()])
        );
        assert_eq!(f[2].ty, FieldType::List(Box::new(FieldType::Str)));
        assert!(f[3].optional && f[3].alias == "owner");
        // alias override + regex type
        assert_eq!(f[4].key, "spec_url");
        assert_eq!(f[4].alias, "spec");
        assert!(f[4].optional);
        assert!(matches!(f[4].ty, FieldType::Regex(_)));
    }

    #[test]
    fn trailing_cardinality_on_list_type_is_rejected() {
        // `[string]+` has no meaning; the parser rejects it rather than silently
        // dropping the `+`.
        assert!(parse_schema("---\ntags: [string]+\n---\n").is_err());
    }

    #[test]
    fn body_head_card_name_label() {
        let s = parse(
            "## @manual Manual verification\n\
            \x20 ### @setup Setup\n\
            \x20   - +@items\n\
            \x20 ### @procedure Procedure\n\
            \x20   1. +@steps\n\
            \x20     - ?@note\n",
        );
        // top level: one heading "manual" with two child headings
        assert_eq!(s.body.len(), 1);
        let Node::Heading {
            level,
            title,
            head,
            children,
        } = &s.body[0]
        else {
            panic!("expected heading");
        };
        assert_eq!(*level, 2);
        assert_eq!(head.name.as_deref(), Some("manual"));
        assert_eq!(*title, Match::Literal("Manual verification".into()));
        assert_eq!(children.len(), 2);

        // setup -> a "+@items" bullet list
        let Node::Heading {
            children: setup, ..
        } = &children[0]
        else {
            panic!();
        };
        assert_eq!(setup.len(), 1);
        let Node::List { style, head, .. } = &setup[0] else {
            panic!("expected list");
        };
        assert_eq!(*style, ListStyle::Bullet);
        assert_eq!(head.name.as_deref(), Some("items"));
        assert_eq!(head.card, Card::Plus);

        // procedure -> ordered list -> optional nested bullet
        let Node::Heading { children: proc, .. } = &children[1] else {
            panic!();
        };
        let Node::List {
            style,
            children: steps_kids,
            ..
        } = &proc[0]
        else {
            panic!();
        };
        assert_eq!(*style, ListStyle::Ordered);
        assert_eq!(steps_kids.len(), 1);
        let Node::List { head, .. } = &steps_kids[0] else {
            panic!();
        };
        assert_eq!(head.card, Card::Optional);
        assert_eq!(head.name.as_deref(), Some("note"));
    }

    #[test]
    fn flat_heading_schema_nests_by_level() {
        // Unindented sub-headings still nest, mirroring the document.
        let s = parse("# @root /.+/\n## @child Child\n  - +@items\n");
        let Node::Heading { children, .. } = &s.body[0] else {
            panic!();
        };
        assert_eq!(
            children.len(),
            1,
            "## nests under # even flat: {:?}",
            s.body
        );
        let doc = "# Anything\n## Child\n- a\n- b\n";
        assert!(s.validate(doc).is_empty(), "{:?}", s.validate(doc));
    }

    #[test]
    fn labels_regex_literal_and_bare_with_card() {
        // regex label
        let s = parse("# @title /.+/");
        let Node::Heading {
            title: Match::Regex(re),
            ..
        } = &s.body[0]
        else {
            panic!();
        };
        assert_eq!(re.as_str(), ".+");

        // bare literal label with a leading card, no name
        let s = parse("## + Docs:");
        let Node::Heading { title, head, .. } = &s.body[0] else {
            panic!();
        };
        assert_eq!(head.card, Card::Plus);
        assert_eq!(head.name, None);
        assert_eq!(*title, Match::Literal("Docs:".into()));

        // range card
        let s = parse("- {1,5}@points");
        let Node::List { head, .. } = &s.body[0] else {
            panic!();
        };
        assert_eq!(head.card, Card::Range(1, Some(5)));
        assert_eq!(head.name.as_deref(), Some("points"));
    }

    #[test]
    fn invalid_regex_label_is_rejected_at_parse() {
        assert!(parse_schema("# @t /(unclosed/").is_err());
    }

    #[test]
    fn duplicate_alias_in_scope_is_rejected() {
        assert!(parse_schema("### @a First\n  - +@x\n### @a Second\n  - +@x\n").is_err());
    }

    #[test]
    fn node_indented_under_prose_is_rejected() {
        assert!(parse_schema("> @intro\n  - +@points\n").is_err());
    }

    #[test]
    fn adjacent_same_marker_lists_are_rejected() {
        // Two bullet lists in one scope would merge in the document.
        assert!(parse_schema("- +@a\n- +@b\n").is_err());
        // Different markers are fine.
        assert!(parse_schema("- +@a\n1. +@b\n").is_ok());
        // Separated by prose is fine.
        assert!(parse_schema("- +@a\n> @sep\n- +@b\n").is_ok());
    }

    #[test]
    fn description_is_fenced_and_label_may_contain_dashes() {
        let s = parse("## @t Trade-offs: speed -- vs -- safety   <? the desc ?>");
        let Node::Heading { title, head, .. } = &s.body[0] else {
            panic!();
        };
        assert_eq!(
            *title,
            Match::Literal("Trade-offs: speed -- vs -- safety".into())
        );
        assert_eq!(head.desc.as_deref(), Some("the desc"));
    }

    #[test]
    fn escaping_leading_special() {
        let s = parse("- \\+literal-plus");
        let Node::List { item, head, .. } = &s.body[0] else {
            panic!();
        };
        assert_eq!(head.card, Card::Required); // the '+' was escaped, not a card
        assert_eq!(*item, Some(Match::Literal("+literal-plus".into())));
    }

    #[test]
    fn unterminated_frontmatter_errors() {
        let err = parse_schema("---\ntitle: string\n").unwrap_err();
        assert!(err.message.contains("unterminated frontmatter"));
    }

    const SCHEMA: &str = "## @manual Manual verification\n\
        \x20 ### @setup Setup\n\
        \x20   - +@items\n\
        \x20 ### @procedure Procedure\n\
        \x20   1. +@steps\n\
         ## @plan Test plan\n\
        \x20 - [ ] +@cases\n";

    #[test]
    fn validate_accepts_a_conforming_document() {
        let s = parse(SCHEMA);
        let doc = "## Manual verification\n\
            ### Setup\n\
            - external monitor\n\n\
            ### Procedure\n\
            1. open a tab\n\
            2. run it\n\n\
            ## Test plan\n\
            - [x] one\n\
            - [ ] two\n";
        let problems = s.validate(doc);
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn validate_flags_missing_section_and_empty_list() {
        let s = parse(SCHEMA);
        // No "Test plan" section; Setup present but with no items.
        let doc = "## Manual verification\n\
            ### Setup\n\n\
            ### Procedure\n\
            1. open\n";
        let problems = s.validate(doc);
        let joined: Vec<String> = problems.iter().map(|p| p.to_string()).collect();
        let all = joined.join("\n");
        assert!(all.contains("plan"), "missing Test plan: {all}");
        assert!(
            all.contains("setup") && all.contains("at least one item"),
            "empty Setup list: {all}"
        );
    }

    #[test]
    fn validate_respects_strict_ordering() {
        // Schema wants Setup then Procedure; doc has them reversed.
        let s = parse("## @m M\n  ### @setup Setup\n    > @x\n  ### @proc Procedure\n    > @y\n");
        let reversed = "## M\n### Procedure\nbody\n\n### Setup\nbody\n";
        let problems = s.validate(reversed);
        assert!(!problems.is_empty(), "strict order should flag reversal");
    }

    #[test]
    fn strict_flags_unexpected_block() {
        let s = parse("## @notes Notes\n  > @body\n");
        let doc = "## Notes\nthe body\n\n## Surprise\nnot in the schema\n";
        let problems = s.validate(doc);
        assert!(
            problems.iter().any(|p| p.message.contains("unexpected")),
            "strict should flag the extra section: {problems:?}"
        );
        // Turning strict off accepts it.
        let lax = parse("%strict = false\n## @notes Notes\n  > @body\n");
        assert!(lax.validate(doc).is_empty(), "{:?}", lax.validate(doc));
    }

    #[test]
    fn frontmatter_is_typed_and_extracted() {
        let s = parse(
            "---\n\
             title: string\n\
             count: int\n\
             status: enum(planned, done)\n\
             tags: [string]\n\
             ---\n\
             > @body\n",
        );
        let doc = "---\ntitle: Hello\ncount: 3\nstatus: done\ntags: [a, b]\n---\n\nsome body\n";
        assert!(s.validate(doc).is_empty(), "{:?}", s.validate(doc));
        let v = s.extract(doc).expect("conforms");
        assert_eq!(v["title"], "Hello");
        assert_eq!(v["count"], json!(3));
        assert_eq!(v["status"], "done");
        assert_eq!(v["tags"], json!(["a", "b"]));

        // A bad type and an unknown key are both flagged.
        let bad =
            "---\ntitle: Hi\ncount: not-a-number\nstatus: done\ntags: []\nextra: x\n---\n\nb\n";
        let problems = s.validate(bad);
        assert!(problems.len() >= 2, "{problems:?}");
    }

    #[test]
    fn body_only_schema_ignores_document_frontmatter() {
        let s = parse("## @notes Notes\n  > @body\n");
        let doc = "---\ntitle: whatever\n---\n\n## Notes\nthe body\n";
        assert!(s.validate(doc).is_empty(), "{:?}", s.validate(doc));
    }

    #[test]
    fn frontmatter_empty_list_element_round_trips() {
        // `[""]` must stay distinct from `[]`.
        let s = parse("---\ntags: [string]\n---\n> @b\n");
        for tags in [json!([""]), json!([]), json!(["", ""]), json!(["a", ""])] {
            let data = json!({ "tags": tags, "b": "body" });
            let md = s.render(&data).expect("render");
            assert_eq!(s.extract(&md).expect("extract"), data, "tags={tags}\n{md}");
        }
    }

    #[test]
    fn nested_list_frontmatter_type_is_rejected() {
        assert!(parse_schema("---\nk: [[string]]\n---\n> @b\n").is_err());
    }

    #[test]
    fn invalid_calendar_dates_are_rejected() {
        let s = parse("---\nd: date\n---\n> @b\n");
        let ok = "---\nd: 2024-02-29\n---\n\nbody\n"; // 2024 is a leap year
        assert!(s.validate(ok).is_empty(), "{:?}", s.validate(ok));
        for bad in ["2026-02-29", "2026-02-30", "2026-04-31", "2026-13-01"] {
            let doc = format!("---\nd: {bad}\n---\n\nbody\n");
            assert!(!s.validate(&doc).is_empty(), "should reject {bad}");
        }
    }

    #[test]
    fn extract_builds_the_capture_object() {
        let s = parse(SCHEMA);
        let doc = "## Manual verification\n\
            ### Setup\n\
            - external monitor\n\n\
            ### Procedure\n\
            1. open a tab\n\
            2. run it\n\n\
            ## Test plan\n\
            - [x] one\n\
            - [ ] two\n";
        let v = s.extract(doc).expect("conforms");
        assert_eq!(v["manual"]["setup"]["items"][0], "external monitor");
        assert_eq!(v["manual"]["procedure"]["steps"][1], "run it");
        // Checklist items carry their checked state.
        assert_eq!(
            v["plan"]["cases"],
            json!([
                { "text": "one", "checked": true },
                { "text": "two", "checked": false },
            ])
        );
    }

    #[test]
    fn extract_errors_on_nonconforming() {
        let s = parse(SCHEMA);
        let bad = "## Manual verification\n### Setup\n\n### Procedure\n1. x\n";
        assert!(s.extract(bad).is_err());
    }

    #[test]
    fn scaffold_emits_required_only_and_validates() {
        let s = parse(
            "---\n\
             title: string\n\
             status: enum(planned, done)\n\
             tags: [string]\n\
             owner?: string\n\
             ---\n\
             ## @manual Manual verification\n\
            \x20 ### @setup Setup\n\
            \x20   - +@items\n\
             ## ?@refs References\n\
            \x20 - *@links\n",
        );
        let out = s.scaffold();
        assert!(out.contains("status: planned\n"), "{out}"); // first enum value
        assert!(out.contains("tags: []\n"), "{out}");
        assert!(!out.contains("owner"), "optional key omitted: {out}");
        assert!(out.contains("## Manual verification\n"), "{out}");
        assert!(out.contains("### Setup\n"), "{out}");
        assert!(
            !out.contains("References"),
            "optional heading omitted: {out}"
        );
        // The scaffold is itself a conforming document.
        assert!(s.validate(&out).is_empty(), "scaffold validates: {out}");
    }

    #[test]
    fn render_produces_conforming_document() {
        let s = parse(SCHEMA);
        let data = json!({
            "manual": {
                "setup": { "items": ["external monitor", "test device"] },
                "procedure": { "steps": ["open a tab", "run it"] }
            },
            "plan": { "cases": [
                { "text": "smoke", "checked": true },
                { "text": "edge case", "checked": false },
            ] }
        });
        let md = s.render(&data).expect("renders");
        let problems = s.validate(&md);
        assert!(
            problems.is_empty(),
            "rendered doc fails validate: {problems:?}\n---\n{md}"
        );
        // Full round trip: checked state survives.
        assert_eq!(s.extract(&md).expect("re-extract"), data);
    }

    #[test]
    fn render_errors_on_missing_required_field() {
        let s = parse(SCHEMA);
        // Missing "plan" (required section)
        let data = json!({
            "manual": {
                "setup": { "items": ["a"] },
                "procedure": { "steps": ["b"] }
            }
        });
        let err = s.render(&data).unwrap_err();
        assert!(matches!(err, RenderError::MissingField(_)));
    }

    #[test]
    fn round_trip_preserves_markdown_significant_text() {
        // The codec must be a lossless fixpoint for text comrak would otherwise
        // mangle: angle-bracket text (raw inline HTML) and backslashes.
        let schema = parse("- +@items\n");
        let md = "- run `printf '\\033\\\\x'` then press <ctrl>c\n";
        let data1 = schema.extract(md).expect("extract");
        let text = data1["items"][0].as_str().unwrap();
        assert!(text.contains("<ctrl>c"), "angle-bracket text lost: {text}");
        assert!(text.contains("\\033\\\\x"), "backslashes lost: {text}");
        // render then re-extract is identical.
        let rendered = schema.render(&data1).expect("render");
        let data2 = schema.extract(&rendered).expect("re-extract");
        assert_eq!(data1, data2);
    }

    #[test]
    fn round_trip_survives_markdown_metacharacters() {
        let schema = parse("- +@items\n");
        let data = json!({ "items": ["*bold*", "a `code` b", "1. not a list", "# not a heading"] });
        let md = schema.render(&data).expect("render");
        assert!(schema.validate(&md).is_empty(), "{md}");
        assert_eq!(schema.extract(&md).expect("re-extract"), data);
    }

    #[test]
    fn edit_replaces_list_item_text() {
        let s = parse(SCHEMA);
        let doc = "## Manual verification\n\
            ### Setup\n\
            - external monitor\n\n\
            ### Procedure\n\
            1. open a tab\n\
            2. run it\n\n\
            ## Test plan\n\
            - [x] one\n\
            - [ ] two\n";
        let edited = s
            .edit(doc, "manual.setup.items.0", "projector")
            .expect("edit succeeds");
        assert!(edited.contains("- projector\n"), "item replaced: {edited}");
        assert!(
            !edited.contains("external monitor"),
            "old text gone: {edited}"
        );
        assert!(edited.contains("open a tab"), "rest preserved: {edited}");
        assert!(s.validate(&edited).is_empty());
    }

    #[test]
    fn edit_is_byte_accurate_after_multibyte_content() {
        // An emoji in an earlier item must not corrupt a later splice, and an
        // item containing inline code must be replaced whole, not partially.
        let s = parse("- +@items\n");
        let doc = "- café ☕ `code`\n- second item\n";
        let edited = s.edit(doc, "items.1", "replaced").expect("edit");
        assert_eq!(edited, "- café ☕ `code`\n- replaced\n", "{edited}");
        let edited2 = s.edit(doc, "items.0", "new").expect("edit");
        assert_eq!(edited2, "- new\n- second item\n", "{edited2}");
    }

    #[test]
    fn edit_replaces_checklist_item_preserving_checkbox() {
        let s = parse(SCHEMA);
        let doc = "## Manual verification\n\
            ### Setup\n\
            - a\n\n\
            ### Procedure\n\
            1. step\n\n\
            ## Test plan\n\
            - [x] old case\n\
            - [ ] second\n";
        let edited = s
            .edit(doc, "plan.cases.0", "new case")
            .expect("edit succeeds");
        assert!(
            edited.contains("- [x] new case\n"),
            "checkbox preserved: {edited}"
        );
        assert!(!edited.contains("old case"), "old text gone: {edited}");
    }

    #[test]
    fn edit_reports_index_out_of_range() {
        let s = parse("- +@items\n");
        let doc = "- a\n- b\n";
        let err = s.edit(doc, "items.9", "x").unwrap_err();
        assert!(matches!(
            err,
            EditError::IndexOutOfRange { index: 9, len: 2 }
        ));
    }

    #[test]
    fn edit_rejects_multiline_value() {
        let s = parse("- +@items\n");
        let err = s.edit("- a\n", "items.0", "two\nlines").unwrap_err();
        assert!(matches!(err, EditError::InvalidValue { .. }));
    }

    #[test]
    fn edit_accepts_bare_unique_alias() {
        let s = parse(SCHEMA);
        let doc = "## Manual verification\n\
            ### Setup\n\
            - a\n\n\
            ### Procedure\n\
            1. first step\n\
            2. second step\n\n\
            ## Test plan\n\
            - [ ] c\n";
        // `steps` is unique across the schema, so no need for the full path.
        let edited = s.edit(doc, "steps.1", "rewritten").expect("edit");
        assert!(edited.contains("2. rewritten\n"), "{edited}");
        assert!(edited.contains("1. first step\n"), "{edited}");
    }

    #[test]
    fn problem_message_includes_description() {
        let s = parse("## @plan Plan\n  - +@cases <? one per behaviour ?>\n");
        let problems = s.validate("## Plan\n");
        assert!(
            problems
                .iter()
                .any(|p| p.to_string().contains("one per behaviour")),
            "{problems:?}"
        );
    }

    #[test]
    fn edit_empty_checkbox_item_inserts_separator() {
        let s = parse("- [ ] +@cases\n");
        let doc = "- [x]\n- [ ] two\n";
        let edited = s.edit(doc, "cases.0", "done").expect("edit");
        assert_eq!(edited, "- [x] done\n- [ ] two\n", "{edited}");
        assert!(s.validate(&edited).is_empty());
        assert_eq!(
            s.extract(&edited).unwrap()["cases"][0],
            json!({ "text": "done", "checked": true })
        );
    }

    #[test]
    fn edit_empty_item_inserts_after_marker() {
        // An item with no text (empty `- `) must edit after its marker, not at
        // byte 0 of the document.
        for (schema, doc, want) in [
            ("- +@items\n", "- \n- b\n", "- filled\n- b\n"),
            ("1. +@items\n", "1. \n2. b\n", "1. filled\n2. b\n"),
        ] {
            let s = parse(schema);
            let edited = s.edit(doc, "items.0", "filled").expect("edit");
            assert_eq!(edited, want, "schema={schema}");
            assert!(s.validate(&edited).is_empty());
        }
    }

    #[test]
    fn edit_scalar_list_by_bare_alias() {
        let s = parse("- ?@item\n");
        let doc = "- the only value\n";
        assert_eq!(s.extract(doc).unwrap()["item"], "the only value");
        let edited = s.edit(doc, "item", "replaced").expect("edit");
        assert_eq!(edited, "- replaced\n", "{edited}");
    }

    #[test]
    fn edit_labeled_item_preserves_label() {
        let s = parse("- +@refs Ref:\n");
        let doc = "- Ref: alpha\n- Ref: beta\n";
        assert_eq!(s.extract(doc).unwrap()["refs"], json!(["alpha", "beta"]));
        let edited = s.edit(doc, "refs.0", "gamma").expect("edit");
        assert_eq!(edited, "- Ref: gamma\n- Ref: beta\n", "{edited}");
        assert!(s.validate(&edited).is_empty(), "{:?}", s.validate(&edited));
    }

    #[test]
    fn ordered_repeated_heading_does_not_over_claim() {
        // A repeated heading must not reach over an intervening sibling section.
        let s = parse("## +@a /^A/\n  > @x\n## @b Foo\n  > @y\n");
        let doc = "## A one\nx1\n\n## Foo\ny\n\n## A two\nx2\n";
        // Interleaved A / Foo / A does not conform under %ordered, but the errors
        // should be coherent (A2 unexpected), not "Foo missing AND unexpected".
        let problems = s.validate(doc);
        let all = problems
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all.contains("unexpected"), "{all}");
        assert!(
            !all.contains("b: expected"),
            "Foo should be found, not missing: {all}"
        );
    }

    #[test]
    fn regex_titled_heading_title_round_trips_with_metacharacters() {
        let s = parse("# @sec /.+/\n  - +@items\n");
        let data = json!({ "sec": { "title": "Plan *v2* [x]", "items": ["a"] } });
        let md = s.render(&data).expect("render");
        assert!(s.validate(&md).is_empty(), "{md}");
        assert_eq!(s.extract(&md).expect("extract"), data);
    }

    #[test]
    fn render_errors_on_non_string_item() {
        let s = parse("- +@items\n");
        let err = s.render(&json!({ "items": [42] })).unwrap_err();
        assert!(matches!(err, RenderError::WrongType { .. }));
    }

    #[test]
    fn scaffold_of_checklist_with_children_validates() {
        let s = parse("- [ ] +@a\n  - +@b\n");
        let out = s.scaffold();
        assert!(
            s.validate(&out).is_empty(),
            "scaffold:\n{out}\n{:?}",
            s.validate(&out)
        );
    }

    #[test]
    fn duplicate_single_section_is_not_silently_swallowed() {
        // Two identical `## Sec` sections against a single-valued heading: the
        // extra must not be silently absorbed (that would lose data).
        let s = parse("## @s Sec\n  > @x\n");
        let doc = "## Sec\naa\n\n## Sec\nbb\n";
        let problems = s.validate(doc);
        assert!(!problems.is_empty(), "duplicate section should be flagged");
        assert!(s.extract(doc).is_err());
    }

    #[test]
    fn edit_of_marked_up_label_does_not_corrupt() {
        let s = parse("- +@refs Ref:\n");
        // Plain label: the label is preserved.
        assert_eq!(
            s.edit("- Ref: alpha\n", "refs.0", "gamma").unwrap(),
            "- Ref: gamma\n"
        );
        // Label wrapped in markup: rather than splicing inside the markup and
        // corrupting the item, the whole item is replaced (a loud, not silent,
        // outcome).
        let edited = s.edit("- *Ref:* target\n", "refs.0", "gamma").unwrap();
        assert!(
            !edited.contains("Ref:gamma"),
            "must not splice inside markup: {edited}"
        );
    }

    #[test]
    fn deep_nesting_is_bounded_not_crashed() {
        // Far past the depth cap: parse rejects, and document parsing does not
        // panic (it truncates).
        let deep_schema: String = (0..400)
            .map(|i| format!("{}- +@a{i}\n", "  ".repeat(i)))
            .collect();
        assert!(parse_schema(&deep_schema).is_err());
        let s = parse("- +@items\n");
        let deep_doc: String = (0..400)
            .map(|i| format!("{}- x\n", "  ".repeat(i)))
            .collect();
        let _ = s.validate(&deep_doc); // must return, not overflow the stack
    }

    #[test]
    fn edit_child_of_empty_parent_item() {
        // An empty parent item that still nests a child list must be navigable.
        let s = parse("- +@i\n  - +@j\n");
        let doc = "- \n  - c\n";
        assert!(s.validate(doc).is_empty(), "{:?}", s.validate(doc));
        assert_eq!(s.edit(doc, "i.0.j.0", "z").unwrap(), "- \n  - z\n");
    }

    #[test]
    fn labeled_scalar_and_checklist_edits_preserve_label() {
        let scalar = parse("- ?@item Val:\n");
        assert_eq!(
            scalar.edit("- Val: x\n", "item", "y").unwrap(),
            "- Val: y\n"
        );
        let checklist = parse("- [ ] +@c Do:\n");
        assert_eq!(
            checklist.edit("- [x] Do: task\n", "c.0", "new").unwrap(),
            "- [x] Do: new\n"
        );
    }

    #[test]
    fn deep_scaffold_nesting_validates() {
        for schema in [
            "- +@a\n  - +@b\n    1. +@c\n",
            "- [ ] +@a\n  - +@b\n    - +@c\n",
        ] {
            let s = parse(schema);
            assert!(
                s.validate(&s.scaffold()).is_empty(),
                "{schema}\n{}",
                s.scaffold()
            );
        }
    }

    #[test]
    fn edit_returns_target_not_found_for_bad_path() {
        let s = parse(SCHEMA);
        let doc = "## Manual verification\n\
            ### Setup\n\
            - a\n\n\
            ### Procedure\n\
            1. b\n\n\
            ## Test plan\n\
            - [ ] c\n";
        let err = s.edit(doc, "nonexistent.path", "x").unwrap_err();
        assert!(matches!(err, EditError::TargetNotFound));
    }
}
