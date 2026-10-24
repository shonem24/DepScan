use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::entity::{Entity, EntityKind};
use crate::lsp::{path_to_uri, LspClient};

pub struct EntityIndex {
    by_file_uri: HashMap<String, Vec<usize>>,
}

impl EntityIndex {
    pub fn new() -> Self {
        Self {
            by_file_uri: HashMap::new(),
        }
    }

    pub fn insert_file_entities(&mut self, file_uri: String, indices: Vec<usize>) {
        self.by_file_uri.insert(file_uri, indices);
    }

    pub fn position_to_entity(
        &self,
        uri: &str,
        line: u32,
        entities: &[Entity],
        exclude: &[EntityKind],
    ) -> Option<usize> {
        self.by_file_uri.get(uri)?.iter().copied().find(|&i| {
            let e = &entities[i];
            !exclude.contains(&e.kind) && e.line_start <= line && line <= e.line_end
        })
    }

    pub fn file_entity(&self, file_uri: &str) -> usize {
        *self.by_file_uri[file_uri].last().unwrap()
    }
}

pub fn extract_entities(
    file_path: &Path,
    file_text: &str,
    project_root: &Path,
    client: &mut LspClient,
    kind_map: &HashMap<u32, EntityKind>,
    entities: &mut Vec<Entity>,
) -> Result<()> {
    let file_uri = path_to_uri(file_path);
    let rel_path = file_path
        .strip_prefix(project_root)?
        .to_string_lossy()
        .to_string();
    let line_count = file_text.lines().count() as u32;

    let file_entity_idx = entities.len();
    entities.push(Entity {
        var_name: format!("{}/self (File)", rel_path),
        name: file_path.file_name().unwrap().to_string_lossy().to_string(),
        kind: EntityKind::File,
        line_start: 0,
        line_end: line_count,
        file_uri: file_uri.clone(),
        parent: None,
        children: Vec::new(),
    });

    let symbols = client.document_symbols(&file_uri)?;
    walk_symbols(
        &symbols,
        file_entity_idx,
        &rel_path,
        &file_uri,
        kind_map,
        entities,
    );
    Ok(())
}

fn walk_symbols(
    symbols: &[Value],
    parent_idx: usize,
    rel_path: &str,
    file_uri: &str,
    kind_map: &HashMap<u32, EntityKind>,
    entities: &mut Vec<Entity>,
) {
    for sym in symbols {
        let kind_int = sym.get("kind").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let children_val = sym.get("children").and_then(|v| v.as_array());

        let Some(&kind) = kind_map.get(&kind_int) else {
            if let Some(children) = children_val {
                walk_symbols(children, parent_idx, rel_path, file_uri, kind_map, entities);
            }
            continue;
        };

        if kind == EntityKind::Field && entities[parent_idx].kind != EntityKind::Class {
            if let Some(children) = children_val {
                walk_symbols(children, parent_idx, rel_path, file_uri, kind_map, entities);
            }
            continue;
        }

        let raw_name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let name = raw_name.split('(').next().unwrap_or(raw_name).to_string();
        let kind = if (name == "__init__" || name == "new")
            && matches!(kind, EntityKind::Method | EntityKind::Function)
        {
            EntityKind::Constructor
        } else {
            kind
        };

        let rng = sym
            .get("range")
            .or_else(|| sym.get("location").and_then(|l| l.get("range")));
        let line_start = rng.and_then(|r| r["start"]["line"].as_u64()).unwrap_or(0) as u32;
        let line_end = rng
            .and_then(|r| r["end"]["line"].as_u64())
            .unwrap_or(line_start as u64) as u32;

        let var_name = build_var_name(rel_path, &name, kind, &entities[parent_idx]);
        let entity_idx = entities.len();
        entities.push(Entity {
            var_name,
            name,
            kind,
            line_start,
            line_end,
            file_uri: file_uri.to_string(),
            parent: Some(parent_idx),
            children: Vec::new(),
        });
        entities[parent_idx].children.push(entity_idx);

        if let Some(children) = children_val {
            walk_symbols(children, entity_idx, rel_path, file_uri, kind_map, entities);
        }
    }
}

fn build_var_name(rel_path: &str, name: &str, kind: EntityKind, parent: &Entity) -> String {
    let plural = |k: EntityKind| match k {
        EntityKind::Method => Some("methods"),
        EntityKind::Constructor => Some("constructors"),
        EntityKind::Field => Some("fields"),
        _ => None,
    };

    if parent.kind == EntityKind::File {
        if kind == EntityKind::Function {
            return format!("{}/functions/{} (Function)", rel_path, name);
        }
        return match plural(kind) {
            Some(p) => format!("{}/{}/{} ({})", rel_path, p, name, kind),
            None => format!("{}/{}/self ({})", rel_path, name, kind),
        };
    }

    let suffix = format!("/self ({})", parent.kind);
    let parent_prefix = parent
        .var_name
        .strip_suffix(&suffix)
        .unwrap_or(&parent.var_name);
    match plural(kind) {
        Some(p) => format!("{}/{}/{} ({})", parent_prefix, p, name, kind),
        None => format!("{}/{}/self ({})", parent_prefix, name, kind),
    }
}
