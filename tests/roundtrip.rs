//! Property tests for the inverse laws that make marklens a codec rather than a
//! one-way parser:
//!
//! * `extract(render(data)) == data` for data matching a schema's shape;
//! * `render(...)` always produces a document that re-validates;
//! * `edit` splices one node and leaves the document conforming.
//!
//! Strings are drawn from an adversarial alphabet (markdown metacharacters,
//! unicode, quotes) so the escaping in `render`/`edit` is exercised hard. Values
//! are trimmed and made non-empty, matching the documented contract (leading/
//! trailing whitespace is not preserved).

use marklens_core::parse_schema;
use proptest::prelude::*;
use serde_json::{json, Value};

/// An adversarial-but-legal captured string: internal metacharacters and
/// unicode, trimmed, never empty, no line breaks.
fn text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            proptest::char::range('a', 'z'),
            proptest::char::range('0', '9'),
            Just(' '),
            prop_oneof![
                Just('*'),
                Just('_'),
                Just('`'),
                Just('['),
                Just(']'),
                Just('<'),
                Just('>'),
                Just('&'),
                Just('|'),
                Just('~'),
                Just('#'),
                Just('-'),
                Just('+'),
                Just('='),
                Just('.'),
                Just('('),
                Just(')'),
                Just('!'),
                Just('\\'),
                Just('"'),
                Just(':'),
                Just('/'),
            ],
            prop_oneof![Just('é'), Just('☕'), Just('中'), Just('—')],
        ],
        1..12,
    )
    .prop_map(|chars| {
        let s: String = chars.into_iter().collect();
        let t = s.trim();
        if t.is_empty() {
            "x".to_string()
        } else {
            t.to_string()
        }
    })
}

fn texts() -> impl Strategy<Value = Vec<String>> {
    proptest::collection::vec(text(), 1..5)
}

proptest! {
    /// Bullet, ordered, prose, and nested-heading round trip.
    #[test]
    fn body_round_trips(
        overview in text(),
        bullets in texts(),
        steps in texts(),
    ) {
        let schema = parse_schema(concat!(
            "## @report Report\n",
            "  ### @intro Intro\n",
            "    > @overview\n",
            "  ### @items Items\n",
            "    - +@bullets\n",
            "  ### @steps Steps\n",
            "    1. +@ordered\n",
        )).unwrap();

        let data = json!({
            "report": {
                "intro": { "overview": overview },
                "items": { "bullets": bullets },
                "steps": { "ordered": steps },
            }
        });

        let md = schema.render(&data).expect("render");
        let problems = schema.validate(&md);
        prop_assert!(problems.is_empty(), "render output invalid:\n{md}\n{problems:?}");
        let back = schema.extract(&md).expect("extract");
        prop_assert_eq!(back, data, "\n--- rendered ---\n{}", md);
    }

    /// Checklists preserve checked state through the codec.
    #[test]
    fn checklist_round_trips(items in proptest::collection::vec((text(), any::<bool>()), 1..5)) {
        let schema = parse_schema("## @plan Plan\n  - [ ] +@cases\n").unwrap();
        let cases: Vec<Value> = items
            .iter()
            .map(|(t, c)| json!({ "text": t, "checked": c }))
            .collect();
        let data = json!({ "plan": { "cases": cases } });

        let md = schema.render(&data).expect("render");
        prop_assert!(schema.validate(&md).is_empty(), "invalid:\n{md}");
        prop_assert_eq!(schema.extract(&md).expect("extract"), data, "\n{}", md);
    }

    /// Labeled lists do not double their label on round trip.
    #[test]
    fn labeled_list_round_trips(items in texts()) {
        let schema = parse_schema("- +@docs Docs:\n").unwrap();
        // The label is a prefix; captured values are the text after it.
        let data = json!({ "docs": items });
        let md = schema.render(&data).expect("render");
        prop_assert!(schema.validate(&md).is_empty(), "invalid:\n{md}");
        prop_assert_eq!(schema.extract(&md).expect("extract"), data, "\n{}", md);
    }

    /// Typed frontmatter round trips (string/int/bool/enum/list).
    #[test]
    fn frontmatter_round_trips(
        title in text(),
        count in any::<i32>(),
        flag in any::<bool>(),
        tags in texts(),
    ) {
        let schema = parse_schema(concat!(
            "---\n",
            "title: string\n",
            "count: int\n",
            "flag: bool\n",
            "status: enum(planned, done)\n",
            "tags: [string]\n",
            "---\n",
            "> @body\n",
        )).unwrap();

        let data = json!({
            "title": title,
            "count": count,
            "flag": flag,
            "status": "done",
            "tags": tags,
            "body": "the body",
        });

        let md = schema.render(&data).expect("render");
        let problems = schema.validate(&md);
        prop_assert!(problems.is_empty(), "invalid:\n{md}\n{problems:?}");
        prop_assert_eq!(schema.extract(&md).expect("extract"), data, "\n{}", md);
    }

    /// Editing one item replaces exactly that item and keeps the doc conforming,
    /// regardless of multibyte content elsewhere.
    #[test]
    fn edit_is_surgical(bullets in proptest::collection::vec(text(), 2..6), replacement in text(), idx in 0usize..6) {
        let schema = parse_schema("- +@items\n").unwrap();
        let data = json!({ "items": bullets.clone() });
        let md = schema.render(&data).expect("render");
        let i = idx % bullets.len();
        let edited = schema.edit(&md, &format!("items.{i}"), &replacement).expect("edit");
        prop_assert!(schema.validate(&edited).is_empty(), "edited invalid:\n{edited}");
        let back = schema.extract(&edited).expect("extract");
        let mut expected = bullets.clone();
        expected[i] = replacement.clone();
        prop_assert_eq!(&back["items"], &json!(expected), "\n{}", edited);
    }
}
