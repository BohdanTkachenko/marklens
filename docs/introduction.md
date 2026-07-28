# marklens

*markdown ⇄ data, via a template.*

marklens maps a Markdown document to typed data and back, from one small schema
that *looks like the document itself*. So structured Markdown — runbooks, ADRs,
release notes, LLM output — becomes data you can **validate**, **extract**,
**render**, **scaffold**, and **edit**, without hand-writing a parser.

New here? **Start with the [Guide](./guide.md)** — it walks one example through
all five operations in a couple of minutes.

## Install

The library is `marklens-core`; the command-line tool is `marklens`:

```sh
cargo add marklens-core       # the library
cargo install marklens        # the `marklens` CLI
```

## The rest of these docs

- **[Guide](./guide.md)** — the fastest way in: one example, all five operations.
- **[Worked example](./marklens-reference.md)** — a real, larger schema run end
  to end, with the exact output of each operation.
- **[Language & API spec](./structure-dsl-spec.md)** — the precise rules and the
  Rust API, for when you need the fine print.

Source and issues:
[github.com/BohdanTkachenko/marklens](https://github.com/BohdanTkachenko/marklens).
