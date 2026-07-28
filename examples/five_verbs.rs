//! One schema, five operations. Run with `cargo run --example five_verbs`.
//!
//! Nothing is hardcoded here — the three files in `examples/` are the whole
//! story, and each is exactly what one operation consumes or produces:
//!
//! - `five_verbs.schema.md` — the schema (a schema is itself markdown)
//! - `five_verbs.json`      — the data object
//! - `five_verbs.md`        — the rendered document, byte for byte
//!
//! The document is checked against `render`'s output rather than merely shipped
//! alongside it, so it cannot drift: if the renderer changes, this example
//! fails.

use std::error::Error;
use std::fs;
use std::path::Path;

use marklens_core::parse_schema;

fn main() -> Result<(), Box<dyn Error>> {
    // Resolve against the crate root so the example runs from any directory.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");

    // A schema reads like a skeleton of the document it describes.
    let schema_src = fs::read_to_string(dir.join("five_verbs.schema.md"))?;
    let schema = parse_schema(&schema_src)?;
    let data: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("five_verbs.json"))?)?;
    let doc = fs::read_to_string(dir.join("five_verbs.md"))?;

    // 1. scaffold — a starter document, from the schema alone.
    println!("== scaffold ==\n{}", schema.scaffold());

    // 2. render — data → markdown, reproducing the shipped document exactly.
    let rendered = schema.render(&data)?;
    assert_eq!(rendered, doc, "five_verbs.md is stale; re-render it");
    println!("== render ==\n{rendered}");

    // 3. validate — an empty result means the document conforms.
    assert!(schema.validate(&doc).is_empty());
    println!("== validate == conforms\n");

    // 4. extract — markdown → data, the inverse of render.
    let extracted = schema.extract(&doc).expect("conforms");
    assert_eq!(extracted, data);
    println!(
        "== extract ==\n{}\n",
        serde_json::to_string_pretty(&extracted)?
    );

    // 5. edit — replace one node's text, leaving every other byte alone.
    let edited = schema.edit(&doc, "steps.1", "run it twice")?;
    println!("== edit (steps.1) ==\n{edited}");

    Ok(())
}
