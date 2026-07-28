# marklens

Command-line interface for
[`marklens-core`](https://crates.io/crates/marklens-core) — one compact schema
(a textual DSL) that maps a markdown document to and from structured data.

```sh
cargo install marklens   # installs the `marklens` executable
```

One subcommand per operation; the schema is the first argument, and documents
and data default to stdin (`-`). `validate` and `extract` exit non-zero when a
document does not conform, so the tool composes in scripts and CI.

```sh
marklens validate schema.mdp doc.md            # exit 1 + located problems on stderr
marklens extract  schema.mdp doc.md            # structured data on stdout
marklens render   schema.mdp data.json         # markdown on stdout
marklens scaffold schema.mdp                   # starter document
marklens edit     schema.mdp doc.md plan.cases.0 "new text" --in-place
```

`extract` and `render` speak JSON, YAML, TOML, and XML via `-f/--format`
(`render` also infers it from the data file's extension); all four round-trip:

```sh
marklens extract -f yaml schema.mdp doc.md | marklens render -f yaml schema.mdp -
```

See the [`marklens-core`](https://crates.io/crates/marklens-core) crate for the
schema language and the library API.

## License

Apache-2.0
