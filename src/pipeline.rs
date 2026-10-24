use std::path::{Path, PathBuf};

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use tree_sitter::Query;

use crate::definition_resolver::resolve;
use crate::dependency_postprocessing::post_process_deps;
use crate::entity::{Dep, DepKind, Entity};
use crate::entity_extractor::{extract_entities, EntityIndex};
use crate::language::{InferenceContext, Language};
use crate::lsp::{path_to_uri, LspClient};
use crate::possible_dependencies::{PossibleDep, UnresolvedReference};
use crate::profile::Profiler;
use crate::reference_finder::find_references;
use crate::source_files::load_source_files;

pub fn run_pipeline(
    project_dir: &Path,
    lang: &dyn Language,
    lsp_override: Option<Vec<String>>,
    index_only: bool,
    unmapped_path: &Path,
) -> Result<(Vec<Entity>, Vec<Dep>, Vec<PossibleDep>)> {
    let mut profiler = Profiler::start();

    let (project_root, file_texts) = profiler.measure("file discovery", || {
        let project_root = project_dir.canonicalize()?;
        let file_texts = load_source_files(&project_root, lang.extensions())?;
        Ok((project_root, file_texts))
    })?;

    let root_uri = path_to_uri(&project_root);
    let cmd = lsp_override.unwrap_or_else(|| lang.lsp_command());
    let mut client = profiler.measure("LSP startup + init", || {
        let mut client = LspClient::start(cmd)?;
        client.initialize(&root_uri, lang.init_options(&project_root))?;
        eprintln!("Starting LSP, opening {} files…", file_texts.len());
        for (fp, text) in &file_texts {
            client.did_open(&path_to_uri(fp), lang.id(), text)?;
        }
        Ok(client)
    })?;

    eprintln!("Waiting for LSP to index…");
    let idx_spinner = ProgressBar::new_spinner();
    idx_spinner.set_style(ProgressStyle::with_template("{spinner:.cyan} Indexing  {msg}").unwrap());
    idx_spinner.set_message("waiting for LSP…");
    idx_spinner.enable_steady_tick(std::time::Duration::from_millis(80));
    profiler.measure("drain / indexing", || {
        client.drain_until_idle();
        Ok(())
    })?;
    idx_spinner.finish_and_clear();

    if index_only {
        client.print_profile();
        client.shutdown()?;
        profiler.record_zero("entity extraction");
        profiler.record_zero("tree-sitter refs");
        profiler.record_zero("LSP resolution");
        profiler.record_zero("post-processing");
        profiler.print("Pipeline phases");
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    }

    eprintln!("LSP ready. Extracting entities…");

    let mut entities: Vec<Entity> = Vec::new();
    let mut entity_index = EntityIndex::new();
    let kind_map = lang.symbol_kind_map();

    profiler.measure("entity extraction", || {
        let pb = bar(file_texts.len() as u64, "Entities  ");
        for (fp, text) in &file_texts {
            pb.set_message(file_name(fp));
            let base = entities.len();
            extract_entities(
                fp,
                text,
                &project_root,
                &mut client,
                &kind_map,
                &mut entities,
            )?;
            let mut indices: Vec<usize> = (base..entities.len()).collect();
            indices.sort_by_key(|&i| entities[i].line_end.saturating_sub(entities[i].line_start));
            entity_index.insert_file_entities(path_to_uri(fp), indices);
            pb.inc(1);
        }
        pb.finish_with_message("done");
        Ok(())
    })?;

    let ts_lang = lang.ts_language();
    let query = Query::new(&ts_lang, lang.ref_queries())?;

    let all_refs = profiler.measure("tree-sitter refs", || {
        let mut all_refs: Vec<(PathBuf, u32, u32, DepKind, usize)> = Vec::new();
        for (fp, text) in &file_texts {
            let fp_uri = path_to_uri(fp);
            for (line, col, dep_kind, src_idx) in
                find_references(text, &fp_uri, &ts_lang, &query, &entities, &entity_index)?
            {
                all_refs.push((fp.clone(), line, col, dep_kind, src_idx));
            }
        }
        Ok(all_refs)
    })?;
    eprintln!("Resolving {} references via LSP…", all_refs.len());

    let (raw_deps, unresolved_refs) = profiler.measure("LSP resolution", || {
        let pb = bar(all_refs.len() as u64, "Resolving ");
        let mut raw_deps: Vec<Dep> = Vec::new();
        let mut unresolved_refs = Vec::new();
        for (fp, line, col, dep_kind, src_idx) in all_refs {
            pb.set_message(file_name(&fp));
            let fp_uri = path_to_uri(&fp);
            let (target, unresolved) = resolve(
                &mut client,
                &fp_uri,
                line,
                col,
                &project_root,
                &entities,
                &entity_index,
            )?;
            if let Some(tgt_idx) = target {
                if dep_kind == DepKind::Import {
                    let file_tgt = entity_index.file_entity(&entities[tgt_idx].file_uri);
                    let file_src = entity_index.file_entity(&entities[src_idx].file_uri);
                    if src_idx != file_tgt {
                        raw_deps.push(Dep {
                            src: src_idx,
                            tgt: file_tgt,
                            kind: DepKind::Import,
                        });
                    }
                    if file_src != file_tgt {
                        raw_deps.push(Dep {
                            src: file_src,
                            tgt: file_tgt,
                            kind: DepKind::Import,
                        });
                    }
                } else if src_idx != tgt_idx {
                    raw_deps.push(Dep {
                        src: src_idx,
                        tgt: tgt_idx,
                        kind: dep_kind,
                    });
                }
            } else if unresolved {
                unresolved_refs.push(UnresolvedReference {
                    file_uri: fp_uri,
                    line,
                    column: col,
                    kind: dep_kind,
                });
            }
            pb.inc(1);
        }
        pb.finish_with_message("done");
        Ok((raw_deps, unresolved_refs))
    })?;

    client.print_profile();
    client.shutdown()?;

    let possible_deps = lang.infer_possible_dependencies(&InferenceContext {
        file_texts: &file_texts,
        ts_language: &ts_lang,
        entities: &entities,
        entity_index: &entity_index,
        unresolved: &unresolved_refs,
        explicit_deps: &raw_deps,
        unmapped_path,
    })?;

    let final_deps = profiler.measure("post-processing", || {
        Ok(post_process_deps(raw_deps, &entities, lang))
    })?;
    profiler.print("Pipeline phases");

    Ok((entities, final_deps, possible_deps))
}

fn bar(total: u64, prefix: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(&format!(
            "{{spinner:.cyan}} {prefix} [{{bar:40.cyan/blue}}] {{pos}}/{{len}} {{msg}}"
        ))
        .unwrap()
        .progress_chars("=>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}
