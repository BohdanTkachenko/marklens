//! The `marklens` command-line interface — one schema, five verbs.
//!
//! Every subcommand takes the schema as its first argument; documents and data
//! default to stdin (`-`). Validation and extraction exit non-zero when the
//! document does not conform, so the CLI composes in scripts and CI.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use marklens_core::{parse_schema, Problem, Schema};

mod formats;
use formats::Format;

#[derive(Parser)]
#[command(name = "marklens", version, about = "markdown ⇄ data, via a template")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a document against a schema (exit 1 if it does not conform).
    Validate {
        /// Schema file (`-` for stdin).
        schema: PathBuf,
        /// Document file (`-` for stdin, the default).
        #[arg(default_value = "-")]
        document: PathBuf,
    },
    /// Extract a conforming document into structured data on stdout.
    Extract {
        /// Schema file (`-` for stdin).
        schema: PathBuf,
        /// Document file (`-` for stdin, the default).
        #[arg(default_value = "-")]
        document: PathBuf,
        /// Output format.
        #[arg(short, long, value_enum, default_value = "json")]
        format: Format,
        /// Emit compact JSON instead of pretty-printed (JSON only).
        #[arg(long)]
        compact: bool,
    },
    /// Render a data object into a document on stdout.
    Render {
        /// Schema file (`-` for stdin).
        schema: PathBuf,
        /// Data file (`-` for stdin, the default).
        #[arg(default_value = "-")]
        data: PathBuf,
        /// Input format (default: inferred from the file extension, else JSON).
        #[arg(short, long, value_enum)]
        format: Option<Format>,
    },
    /// Print a starter document for a schema.
    Scaffold {
        /// Schema file (`-` for stdin).
        schema: PathBuf,
    },
    /// Replace the node at PATH with VALUE, preserving the rest byte-for-byte.
    Edit {
        /// Schema file (`-` for stdin).
        schema: PathBuf,
        /// Document file (`-` for stdin).
        document: PathBuf,
        /// Dotted capture path, e.g. `plan.cases.0`.
        path: String,
        /// Replacement text.
        value: String,
        /// Write the result back to the document file instead of stdout.
        #[arg(short, long)]
        in_place: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, Box<dyn std::error::Error>> {
    match cli.command {
        Command::Validate { schema, document } => {
            deny_double_stdin(&schema, &document)?;
            let schema = load_schema(&schema)?;
            let doc = read(&document)?;
            let problems = schema.validate(&doc);
            if problems.is_empty() {
                Ok(ExitCode::SUCCESS)
            } else {
                print_problems(&doc, &problems);
                Ok(ExitCode::FAILURE)
            }
        }
        Command::Extract {
            schema,
            document,
            format,
            compact,
        } => {
            deny_double_stdin(&schema, &document)?;
            let schema = load_schema(&schema)?;
            let doc = read(&document)?;
            match schema.extract(&doc) {
                Ok(value) => {
                    print!("{}", format.dump(&value, compact)?);
                    Ok(ExitCode::SUCCESS)
                }
                Err(problems) => {
                    print_problems(&doc, &problems);
                    Ok(ExitCode::FAILURE)
                }
            }
        }
        Command::Render {
            schema,
            data,
            format,
        } => {
            deny_double_stdin(&schema, &data)?;
            let schema = load_schema(&schema)?;
            let fmt = format
                .or_else(|| Format::from_ext(&data))
                .unwrap_or(Format::Json);
            let raw = read(&data)?;
            let value = fmt.parse(&raw).map_err(|e| -> Box<dyn std::error::Error> {
                // stdin can't be sniffed by extension, so it defaults to JSON;
                // point the user at -f when the guess is likely wrong.
                if format.is_none() {
                    format!("{e} (set the input format with -f json|yaml|toml|xml)").into()
                } else {
                    e
                }
            })?;
            print!("{}", schema.render(&value)?);
            Ok(ExitCode::SUCCESS)
        }
        Command::Scaffold { schema } => {
            let schema = load_schema(&schema)?;
            print!("{}", schema.scaffold());
            Ok(ExitCode::SUCCESS)
        }
        Command::Edit {
            schema,
            document,
            path,
            value,
            in_place,
        } => {
            deny_double_stdin(&schema, &document)?;
            let schema = load_schema(&schema)?;
            let doc = read(&document)?;
            let edited = schema.edit(&doc, &path, &value)?;
            if in_place {
                if document.as_os_str() == "-" {
                    return Err("--in-place cannot be used with stdin".into());
                }
                std::fs::write(&document, edited)?;
            } else {
                print!("{edited}");
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn load_schema(path: &Path) -> Result<Schema, Box<dyn std::error::Error>> {
    Ok(parse_schema(&read(path)?)?)
}

fn read(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    if path.as_os_str() == "-" {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else {
        // Include the path in the error (the bare io::Error does not).
        std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()).into())
    }
}

/// Two file arguments cannot both be stdin — the second read would see nothing.
fn deny_double_stdin(a: &Path, b: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if a.as_os_str() == "-" && b.as_os_str() == "-" {
        return Err("cannot read two inputs from stdin (`-`); pass one as a file".into());
    }
    Ok(())
}

/// Print each problem to stderr, prefixed with `line:col` when the problem
/// carries a source span.
fn print_problems(doc: &str, problems: &[Problem]) {
    for p in problems {
        match p.span {
            Some(span) => {
                let (line, col) = line_col(doc, span.start);
                eprintln!("{line}:{col}: {p}");
            }
            None => eprintln!("{p}"),
        }
    }
}

fn line_col(s: &str, byte: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in s.char_indices() {
        if i >= byte {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
