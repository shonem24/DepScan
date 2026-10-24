use std::path::Path;

use anyhow::Result;

use crate::entity::Entity;
use crate::entity_extractor::EntityIndex;
use crate::lsp::{path_to_uri, uri_to_path, LspClient};

pub fn resolve(
    client: &mut LspClient,
    file_uri: &str,
    line: u32,
    character: u32,
    project_root: &Path,
    entities: &[Entity],
    entity_index: &EntityIndex,
) -> Result<(Option<usize>, bool)> {
    let locations = client.definition(file_uri, line, character)?;
    let unresolved = locations.is_empty();
    for loc in locations {
        let Some(tgt_uri_str) = loc.get("uri").and_then(|v| v.as_str()) else {
            continue;
        };
        let tgt_path = uri_to_path(tgt_uri_str);
        let Ok(tgt_canonical) = tgt_path.canonicalize() else {
            continue;
        };
        if !tgt_canonical.starts_with(project_root) {
            continue;
        }
        let tgt_uri = path_to_uri(&tgt_canonical);
        let tgt_line = loc["range"]["start"]["line"].as_u64().unwrap_or(0) as u32;
        if let Some(idx) = entity_index.position_to_entity(&tgt_uri, tgt_line, entities, &[]) {
            return Ok((Some(idx), false));
        }
    }
    Ok((None, unresolved))
}
