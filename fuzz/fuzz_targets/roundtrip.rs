#![no_main]
//! Against a fixed schema, arbitrary documents must never panic — and any
//! document that extracts must satisfy the inverse law: rendering the extracted
//! data and extracting again yields the same value.

use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

use marklens_core::Schema;

fn schema() -> &'static Schema {
    static S: OnceLock<Schema> = OnceLock::new();
    S.get_or_init(|| {
        marklens_core::parse_schema(concat!(
            "---\n",
            "title: string\n",
            "count: int\n",
            "---\n",
            "## @manual Manual\n",
            "  ### @setup Setup\n",
            "    - +@items\n",
            "  ### @steps Steps\n",
            "    1. +@ordered\n",
            "## @plan Plan\n",
            "  - [ ] +@cases\n",
            "## @notes Notes\n",
            "  > @body\n",
        ))
        .expect("fixed schema parses")
    })
}

fuzz_target!(|data: &[u8]| {
    let Ok(doc) = std::str::from_utf8(data) else {
        return;
    };
    let schema = schema();
    // Must not panic.
    let _ = schema.validate(doc);
    if let Ok(value) = schema.extract(doc) {
        // extract → render → extract is a fixpoint.
        let rendered = schema.render(&value).expect("extracted data renders");
        let again = schema.extract(&rendered).expect("rendered doc re-extracts");
        assert_eq!(value, again, "round-trip diverged:\n{rendered}");
    }
});
