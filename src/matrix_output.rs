use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::entity::{Dep, Entity};
use crate::possible_dependencies::PossibleDep;

#[derive(Serialize)]
struct Output<'a> {
    schema: &'static str,
    variables: Vec<&'a str>,
    cells: Vec<Cell>,
}

#[derive(Serialize)]
struct Cell {
    src: usize,
    dest: usize,
    values: HashMap<String, f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    inference: Vec<Inference>,
}

#[derive(Serialize)]
struct Inference {
    kind: String,
    resolution: &'static str,
    candidate_count: usize,
}

pub fn write_output(
    entities: &[Entity],
    deps: &[Dep],
    possible_deps: &[PossibleDep],
    path: &Path,
) -> Result<()> {
    let variables: Vec<&str> = entities.iter().map(|e| e.var_name.as_str()).collect();

    let mut cells_map: BTreeMap<(usize, usize), Vec<Inference>> = BTreeMap::new();
    let mut seen_edges: HashSet<(usize, usize, String, &'static str, usize)> = HashSet::new();
    for dep in deps {
        if dep.src != dep.tgt {
            push_inference(
                &mut cells_map,
                &mut seen_edges,
                dep.src,
                dep.tgt,
                dep.kind.as_str().to_string(),
                "explicit",
                1,
            );
        }
    }
    for dep in possible_deps {
        if dep.src != dep.tgt {
            push_inference(
                &mut cells_map,
                &mut seen_edges,
                dep.src,
                dep.tgt,
                dep.kind.as_str().to_string(),
                "possible",
                dep.candidate_count,
            );
        }
    }

    let cells: Vec<Cell> = cells_map
        .into_iter()
        .map(|((src, dest), kinds)| Cell {
            src,
            dest,
            values: kinds
                .iter()
                .map(|edge| (edge.kind.clone(), 1.0))
                .collect::<HashMap<_, _>>(),
            inference: kinds,
        })
        .collect();

    let json = serde_json::to_string_pretty(&Output {
        schema: "2.0",
        variables,
        cells,
    })?;
    std::fs::write(path, json)?;
    Ok(())
}

fn push_inference(
    cells_map: &mut BTreeMap<(usize, usize), Vec<Inference>>,
    seen_edges: &mut HashSet<(usize, usize, String, &'static str, usize)>,
    src: usize,
    dest: usize,
    kind: String,
    resolution: &'static str,
    candidate_count: usize,
) {
    if seen_edges.insert((src, dest, kind.clone(), resolution, candidate_count)) {
        cells_map.entry((src, dest)).or_default().push(Inference {
            kind,
            resolution,
            candidate_count,
        });
    }
}
