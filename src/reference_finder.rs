use anyhow::Result;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use crate::entity::{DepKind, Entity};
use crate::entity_extractor::EntityIndex;

pub fn find_references(
    text: &str,
    file_uri: &str,
    ts_lang: &tree_sitter::Language,
    query: &Query,
    entities: &[Entity],
    entity_index: &EntityIndex,
) -> Result<Vec<(u32, u32, DepKind, usize)>> {
    let mut parser = Parser::new();
    parser.set_language(ts_lang)?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("tree-sitter parse failed"))?;

    let source = text.as_bytes();
    let mut cursor = QueryCursor::new();
    let mut captures = cursor.captures(query, tree.root_node(), source);
    let mut raw: Vec<(u32, u32, String)> = Vec::new();
    while let Some((mat, cap_idx)) = captures.next() {
        let cap = &mat.captures[*cap_idx];
        let name = &query.capture_names()[cap.index as usize];
        if name.starts_with("Use") && in_call_function_position(cap.node) {
            continue;
        }
        let pos = cap.node.start_position();
        raw.push((pos.row as u32, pos.column as u32, name.to_string()));
    }

    let mut refs = Vec::new();
    for (row, col, capture_name) in raw {
        let dep_kind_str: &str = capture_name.split('.').next().unwrap_or("");
        let Some(dep_kind) = DepKind::from_capture(dep_kind_str) else {
            continue;
        };
        let Some(src_idx) = entity_index.position_to_entity(file_uri, row, entities, &[]) else {
            continue;
        };
        refs.push((row, col, dep_kind, src_idx));
    }
    Ok(refs)
}

fn in_call_function_position(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    // TypeScript/JS: new Foo() — constructor should not also appear as a Use dep
    if parent.kind() == "new_expression" {
        return parent
            .child_by_field_name("constructor")
            .map(|f| f.id() == node.id())
            .unwrap_or(false);
    }

    let Some(grandparent) = parent.parent() else {
        return false;
    };
    let in_fn_slot = (grandparent.kind() == "call" || grandparent.kind() == "call_expression")
        && grandparent
            .child_by_field_name("function")
            .map(|fn_node| fn_node.id() == parent.id())
            .unwrap_or(false);
    if !in_fn_slot {
        return false;
    }
    // For compound function expressions only filter the *function name* slot,
    // not receiver/path slots which should remain as Use deps.
    if parent.kind() == "field_expression" {
        // Rust method call: obj.method() — filter 'field' (method name), keep 'value' (receiver)
        return parent
            .child_by_field_name("field")
            .map(|f| f.id() == node.id())
            .unwrap_or(false);
    }
    if parent.kind() == "scoped_identifier" {
        // Rust associated call: Foo::bar() — filter 'name' (function), keep 'path' (type/module)
        return parent
            .child_by_field_name("name")
            .map(|f| f.id() == node.id())
            .unwrap_or(false);
    }
    if parent.kind() == "member_expression" {
        // TypeScript/JS method call: obj.method() — filter 'property' (method name), keep 'object' (receiver)
        // Note: property is property_identifier (not identifier), so identifier nodes here are
        // always the object slot and this returns false for them.
        return parent
            .child_by_field_name("property")
            .map(|f| f.id() == node.id())
            .unwrap_or(false);
    }
    true
}
