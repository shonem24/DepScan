mod definition_resolver;
mod dependency_postprocessing;
mod entity;
mod entity_extractor;
mod language;
mod lsp;
mod matrix_output;
mod pipeline;
mod possible_dependencies;
mod profile;
mod reference_finder;
mod source_files;

use std::path::Path;

use anyhow::{bail, Result};
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "lsp-dep-extractor",
    about = "Extract structural dependencies via LSP"
)]
struct Cli {
    project_dir: String,

    #[arg(short, long, default_value = "deps.json")]
    output: String,

    #[arg(long, default_value = "python")]
    language: String,

    /// Override the LSP server command, e.g. "pylsp" or "typescript-language-server --stdio".
    /// If omitted, the language's built-in default is used.
    #[arg(long)]
    lsp_override: Option<String>,

    /// Stop after LSP startup/open/indexing and print timing phases.
    #[arg(long)]
    index_only: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let registry = language::registry();
    let Some(lang) = registry.get(cli.language.as_str()) else {
        bail!(
            "unsupported language '{}'; available: {}",
            cli.language,
            registry.keys().cloned().collect::<Vec<_>>().join(", ")
        );
    };

    let lsp_override = cli
        .lsp_override
        .map(|s| s.split_whitespace().map(String::from).collect::<Vec<_>>());

    // `maze_deps.json` -> `maze_deps.unmapped.json`, so runs sharing a folder don't collide.
    let unmapped_path = Path::new(&cli.output).with_extension("unmapped.json");
    let (entities, deps, possible_deps) = pipeline::run_pipeline(
        Path::new(&cli.project_dir),
        lang.as_ref(),
        lsp_override,
        cli.index_only,
        &unmapped_path,
    )?;
    matrix_output::write_output(&entities, &deps, &possible_deps, Path::new(&cli.output))?;

    println!(
        "{} entities, {} deps → {}",
        entities.len(),
        deps.len(),
        cli.output
    );
    Ok(())
}
