use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;
use tree_sitter::{Node, Parser};

use crate::entity::{Dep, DepKind, Entity, EntityKind};
use crate::entity_extractor::EntityIndex;
use crate::lsp::path_to_uri;

/// A reference the LSP could not resolve at all (it returned no definition).
/// References resolved to definitions outside the project are not recorded here, so
/// inference never guesses project targets for external libraries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UnresolvedReference {
    pub file_uri: String,
    pub line: u32,
    pub column: u32,
    pub kind: DepKind,
}

/// A dependency inferred by a language strategy rather than proven by the LSP.
/// It keeps the ordinary relationship kind and records how many targets matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PossibleDep {
    pub src: usize,
    pub tgt: usize,
    pub kind: DepKind,
    pub candidate_count: usize,
}

/// An unresolved reference that inference could not turn into any possible dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmappedReference {
    pub file_uri: String,
    pub line: u32,
    pub column: u32,
    pub src: Option<usize>,
    pub kind: DepKind,
    pub expression: String,
    pub reason: &'static str,
}

/// How a language's syntax tree expresses the constructs inference reads. Each language
/// provides one (like `ref_queries`); the engine itself contains no language-specific names.
pub struct InferenceSpec {
    /// Member access node, e.g. `obj.name`, with the fields holding its two parts.
    pub member_access: &'static str,
    pub member_object_field: &'static str,
    pub member_name_field: &'static str,
    /// Node kind of a plain name (also the kind a member name must have).
    pub identifier: &'static str,
    /// Call node and the fields holding the callee and the arguments.
    pub call: &'static str,
    pub call_function_field: &'static str,
    pub call_arguments_field: &'static str,
    /// Argument kinds that make a positional argument count unreliable.
    pub uncountable_arguments: &'static [&'static str],
    /// Nodes that open a new lexical scope.
    pub scopes: &'static [&'static str],
    /// Nodes that can rebind a variable, and the fields naming what they bind.
    pub rebindings: &'static [&'static str],
    pub rebinding_target_fields: &'static [&'static str],
    /// Nodes that rebinding scans must not descend into (nested definitions).
    pub boundaries: &'static [&'static str],
    pub block: &'static str,
    pub function: &'static str,
    pub class: &'static str,
    pub assignment: &'static str,
    pub assignment_left_field: &'static str,
    pub assignment_right_field: &'static str,
    pub return_statement: &'static str,
    /// Wrappers (e.g. decorators) whose functions are skipped for signature checks.
    pub decorated_wrappers: &'static [&'static str],
    pub function_name_field: &'static str,
    pub parameters_field: &'static str,
    /// Parameter kinds that are plain positional parameters, and those with defaults.
    pub plain_parameters: &'static [&'static str],
    pub optional_parameters: &'static [&'static str],
    /// Leading parameters (such as `self`) that callers do not pass.
    pub implicit_parameters: usize,
    /// Members that are not part of a class's public surface.
    pub is_private: fn(&str) -> bool,
}

struct Access {
    src: usize,
    receiver: String,
    member: String,
    is_call: bool,
    kind: DepKind,
    scope: usize,
    arguments: Option<usize>,
    file_uri: String,
    line: u32,
    column: u32,
    expression: String,
}

fn unmapped_access(access: &Access, reason: &'static str) -> UnmappedReference {
    UnmappedReference {
        file_uri: access.file_uri.clone(),
        line: access.line,
        column: access.column,
        src: Some(access.src),
        kind: access.kind,
        expression: access.expression.clone(),
        reason,
    }
}

/// Infers Python dependencies for unresolved direct attribute accesses. It is deliberately
/// separate from LSP resolution: resolved references remain explicit dependencies, while this
/// pass uses only public project members as candidate targets.
pub fn infer_possible_deps(
    spec: &InferenceSpec,
    file_texts: &[(PathBuf, String)],
    ts_lang: &tree_sitter::Language,
    entities: &[Entity],
    entity_index: &EntityIndex,
    unresolved: &[UnresolvedReference],
    explicit_deps: &[Dep],
    unmapped_path: &Path,
) -> Result<Vec<PossibleDep>> {
    let unresolved: HashSet<_> = unresolved.iter().cloned().collect();
    let mut unmapped: Vec<UnmappedReference> = Vec::new();
    let member_index = build_member_index(spec, entities);
    let inheritance = inheritance_map(explicit_deps);
    let signatures = signatures(spec, file_texts, ts_lang, entities)?;
    let mut accesses = Vec::new();

    for (path, text) in file_texts {
        let file_uri = path_to_uri(path);
        collect_accesses(
            spec,
            text,
            &file_uri,
            ts_lang,
            entities,
            entity_index,
            &unresolved,
            &mut accesses,
        )?;
    }

    // Unresolved references that were never `receiver.member` accesses cannot be inferred.
    let inspected: HashSet<(&str, u32, u32)> = accesses
        .iter()
        .map(|access| (access.file_uri.as_str(), access.line, access.column))
        .collect();
    let mut skipped_positions = HashSet::new();
    for reference in &unresolved {
        let position = (
            reference.file_uri.as_str(),
            reference.line,
            reference.column,
        );
        if inspected.contains(&position) || !skipped_positions.insert(position) {
            continue;
        }
        let expression = file_texts
            .iter()
            .find(|(path, _)| path_to_uri(path) == reference.file_uri)
            .and_then(|(_, text)| text.lines().nth(reference.line as usize))
            .map(|line| line.trim().to_string())
            .unwrap_or_default();
        unmapped.push(UnmappedReference {
            file_uri: reference.file_uri.clone(),
            line: reference.line,
            column: reference.column,
            src: entity_index.position_to_entity(
                &reference.file_uri,
                reference.line,
                entities,
                &[],
            ),
            kind: reference.kind,
            expression,
            reason: "not_an_attribute_access",
        });
    }

    let mut grouped: HashMap<(usize, usize, String), Vec<Access>> = HashMap::new();
    for access in accesses {
        grouped
            .entry((access.src, access.scope, access.receiver.clone()))
            .or_default()
            .push(access);
    }

    let mut deps = Vec::new();
    let mut emitted = HashSet::new();
    let mut mapped_positions: HashSet<(String, u32, u32)> = HashSet::new();
    for ((src, _, _), accesses) in grouped {
        let candidates: Vec<Vec<usize>> = accesses
            .iter()
            .map(|access| {
                candidate_members(access, &member_index, entities)
                    .into_iter()
                    .filter(|idx| match (access.arguments, signatures.get(idx)) {
                        (Some(count), Some((minimum, maximum))) => {
                            count >= *minimum && maximum.is_none_or(|max| count <= max)
                        }
                        _ => true,
                    })
                    .collect()
            })
            .collect();
        let mut shared_owners: Option<HashSet<usize>> = None;
        for targets in &candidates {
            let mut owners: HashSet<usize> = targets
                .iter()
                .filter_map(|idx| entities[*idx].parent)
                .collect();
            let declaring_owners = owners.clone();
            for (idx, entity) in entities.iter().enumerate() {
                if entity.kind == EntityKind::Class
                    && !ancestors(idx, &inheritance).is_disjoint(&declaring_owners)
                {
                    owners.insert(idx);
                }
            }
            shared_owners = Some(match shared_owners {
                Some(existing) => existing.intersection(&owners).copied().collect(),
                None => owners,
            });
        }
        // Keep subtypes too: a call on a base-class receiver may reach a subclass override.
        let owners = shared_owners.unwrap_or_default();
        if owners.is_empty() {
            for (access, targets) in accesses.iter().zip(&candidates) {
                let reason = if targets.is_empty() {
                    "no_matching_member"
                } else {
                    "no_shared_owner"
                };
                unmapped.push(unmapped_access(access, reason));
            }
            continue;
        }

        for (access, candidates) in accesses.into_iter().zip(candidates) {
            let targets: Vec<usize> = candidates
                .into_iter()
                .filter(|idx| {
                    entities[*idx].parent.is_some_and(|owner| {
                        owners.contains(&owner)
                            || owners.iter().any(|candidate| {
                                ancestors(*candidate, &inheritance).contains(&owner)
                            })
                    })
                })
                .collect();
            let candidate_count = targets.len();
            let kind = access.kind;
            if targets.is_empty() {
                unmapped.push(unmapped_access(&access, "excluded_by_receiver_group"));
            } else {
                mapped_positions.insert((access.file_uri.clone(), access.line, access.column));
            }
            for tgt in targets {
                if emitted.insert((src, tgt, kind, candidate_count)) {
                    deps.push(PossibleDep {
                        src,
                        tgt,
                        kind,
                        candidate_count,
                    });
                }
            }
        }
    }
    infer_returned_constructor_deps(
        spec,
        file_texts,
        ts_lang,
        entities,
        entity_index,
        &mut deps,
        &mut emitted,
    )?;

    // A position can be recorded under several kinds; keep it only if nothing mapped it.
    let mut seen_unmapped = HashSet::new();
    unmapped.retain(|reference| {
        let position = (reference.file_uri.clone(), reference.line, reference.column);
        !mapped_positions.contains(&position) && seen_unmapped.insert(position)
    });
    unmapped.sort_by(|a, b| (&a.file_uri, a.line, a.column).cmp(&(&b.file_uri, b.line, b.column)));

    write_unmapped(entities, &unmapped, unmapped_path)?;
    Ok(deps)
}

/// Writes the unmapped-reference report to `path`. Lines and columns are 1-based.
fn write_unmapped(entities: &[Entity], unmapped: &[UnmappedReference], path: &Path) -> Result<()> {
    let entries: Vec<_> = unmapped
        .iter()
        .map(|reference| {
            serde_json::json!({
                "file": crate::lsp::uri_to_path(&reference.file_uri).display().to_string(),
                "line": reference.line + 1,
                "column": reference.column + 1,
                "source": reference
                    .src
                    .and_then(|idx| entities.get(idx))
                    .map(|entity| entity.var_name.as_str()),
                "kind": reference.kind.as_str(),
                "expression": reference.expression,
                "reason": reference.reason,
            })
        })
        .collect();
    let json = serde_json::json!({ "schema": "1.0", "unmapped": entries });
    // A failed side-file write must not discard the analysis result.
    if let Err(error) = std::fs::write(path, serde_json::to_string_pretty(&json)?) {
        eprintln!("warning: could not write {}: {error}", path.display());
    }
    Ok(())
}

fn infer_returned_constructor_deps(
    spec: &InferenceSpec,
    file_texts: &[(PathBuf, String)],
    ts_lang: &tree_sitter::Language,
    entities: &[Entity],
    entity_index: &EntityIndex,
    deps: &mut Vec<PossibleDep>,
    emitted: &mut HashSet<(usize, usize, DepKind, usize)>,
) -> Result<()> {
    let class_index = build_class_index(entities);
    let mut parser = Parser::new();
    parser.set_language(ts_lang)?;

    for (path, text) in file_texts {
        let file_uri = path_to_uri(path);
        let Some(tree) = parser.parse(text, None) else {
            continue;
        };
        visit_functions_for_returned_constructors(
            spec,
            tree.root_node(),
            text,
            &file_uri,
            entities,
            entity_index,
            &class_index,
            deps,
            emitted,
        );
    }
    Ok(())
}

fn visit_functions_for_returned_constructors(
    spec: &InferenceSpec,
    node: Node,
    text: &str,
    file_uri: &str,
    entities: &[Entity],
    entity_index: &EntityIndex,
    class_index: &HashMap<String, Vec<usize>>,
    deps: &mut Vec<PossibleDep>,
    emitted: &mut HashSet<(usize, usize, DepKind, usize)>,
) {
    if node.kind() == spec.function {
        infer_returned_constructors_in_function(
            spec,
            node,
            text,
            file_uri,
            entities,
            entity_index,
            class_index,
            deps,
            emitted,
        );
        return;
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_functions_for_returned_constructors(
            spec,
            child,
            text,
            file_uri,
            entities,
            entity_index,
            class_index,
            deps,
            emitted,
        );
    }
}

fn infer_returned_constructors_in_function(
    spec: &InferenceSpec,
    function: Node,
    text: &str,
    file_uri: &str,
    entities: &[Entity],
    entity_index: &EntityIndex,
    class_index: &HashMap<String, Vec<usize>>,
    deps: &mut Vec<PossibleDep>,
    emitted: &mut HashSet<(usize, usize, DepKind, usize)>,
) {
    let Some(src) = entity_index.position_to_entity(
        file_uri,
        function.start_position().row as u32,
        entities,
        &[],
    ) else {
        return;
    };
    let mut bindings: HashMap<String, Vec<usize>> = HashMap::new();
    scan_return_flow(
        spec,
        function,
        text,
        src,
        &mut bindings,
        class_index,
        deps,
        emitted,
    );
}

fn scan_return_flow(
    spec: &InferenceSpec,
    node: Node,
    text: &str,
    src: usize,
    bindings: &mut HashMap<String, Vec<usize>>,
    class_index: &HashMap<String, Vec<usize>>,
    deps: &mut Vec<PossibleDep>,
    emitted: &mut HashSet<(usize, usize, DepKind, usize)>,
) {
    if [spec.function, spec.class].contains(&node.kind()) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == spec.block {
                scan_return_flow(spec, child, text, src, bindings, class_index, deps, emitted);
            }
        }
        return;
    }

    if node.kind() == spec.assignment {
        if let Some((name, targets)) = constructor_assignment(spec, node, text, class_index) {
            bindings.insert(name, targets);
        }
    } else if node.kind() == spec.return_statement {
        if let Some(returned) = returned_identifier(spec, node, text) {
            if let Some(targets) = bindings.get(&returned) {
                let candidate_count = targets.len();
                for &tgt in targets {
                    if emitted.insert((src, tgt, DepKind::Return, candidate_count)) {
                        deps.push(PossibleDep {
                            src,
                            tgt,
                            kind: DepKind::Return,
                            candidate_count,
                        });
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        scan_return_flow(spec, child, text, src, bindings, class_index, deps, emitted);
    }
}

fn constructor_assignment(
    spec: &InferenceSpec,
    node: Node,
    text: &str,
    class_index: &HashMap<String, Vec<usize>>,
) -> Option<(String, Vec<usize>)> {
    let left = node.child_by_field_name(spec.assignment_left_field)?;
    if left.kind() != spec.identifier {
        return None;
    }
    let right = node.child_by_field_name(spec.assignment_right_field)?;
    let class_name = called_name(spec, right, text)?;
    let targets = class_index.get(&class_name)?.clone();
    Some((node_text(left, text), targets))
}

fn returned_identifier(spec: &InferenceSpec, node: Node, text: &str) -> Option<String> {
    let value = node.named_child(0)?;
    (value.kind() == spec.identifier).then(|| node_text(value, text))
}

fn called_name(spec: &InferenceSpec, node: Node, text: &str) -> Option<String> {
    if node.kind() != spec.call {
        return None;
    }
    let function = node.child_by_field_name(spec.call_function_field)?;
    if function.kind() == spec.identifier {
        Some(node_text(function, text))
    } else if function.kind() == spec.member_access {
        function
            .child_by_field_name(spec.member_name_field)
            .map(|name| node_text(name, text))
    } else {
        None
    }
}

fn collect_accesses(
    spec: &InferenceSpec,
    text: &str,
    file_uri: &str,
    ts_lang: &tree_sitter::Language,
    entities: &[Entity],
    entity_index: &EntityIndex,
    unresolved: &HashSet<UnresolvedReference>,
    accesses: &mut Vec<Access>,
) -> Result<()> {
    let mut parser = Parser::new();
    parser.set_language(ts_lang)?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| anyhow::anyhow!("tree-sitter parse failed"))?;
    visit_attributes(
        spec,
        tree.root_node(),
        text,
        file_uri,
        entities,
        entity_index,
        unresolved,
        accesses,
    );
    Ok(())
}

fn visit_attributes(
    spec: &InferenceSpec,
    node: Node,
    text: &str,
    file_uri: &str,
    entities: &[Entity],
    entity_index: &EntityIndex,
    unresolved: &HashSet<UnresolvedReference>,
    accesses: &mut Vec<Access>,
) {
    if node.kind() == spec.member_access {
        let object = node.child_by_field_name(spec.member_object_field);
        let member = node.child_by_field_name(spec.member_name_field);
        if let (Some(object), Some(member)) = (object, member) {
            // Each segment of a chained expression is an independent inference unit. For
            // `order.customer.email`, this records both `order.customer` and
            // `order.customer.email`; future return-type propagation can further connect them.
            if member.kind() == spec.identifier {
                let pos = member.start_position();
                let unresolved_ref = UnresolvedReference {
                    file_uri: file_uri.to_string(),
                    line: pos.row as u32,
                    column: pos.column as u32,
                    kind: DepKind::Call,
                };
                for kind in [
                    DepKind::Call,
                    DepKind::Use,
                    DepKind::Return,
                    DepKind::Annotation,
                    DepKind::Throw,
                    DepKind::Create,
                ] {
                    if !unresolved.contains(&UnresolvedReference {
                        kind,
                        ..unresolved_ref.clone()
                    }) {
                        continue;
                    }
                    if let Some(src) =
                        entity_index.position_to_entity(file_uri, pos.row as u32, entities, &[])
                    {
                        accesses.push(Access {
                            src,
                            kind,
                            receiver: format!(
                                "{}:{}",
                                node_text(object, text),
                                binding_epoch(spec, node, object, text)
                            ),
                            scope: lexical_scope(spec, node).start_byte(),
                            arguments: call_arguments(spec, node),
                            file_uri: file_uri.to_string(),
                            line: pos.row as u32,
                            column: pos.column as u32,
                            expression: node_text(node, text),
                            member: node_text(member, text),
                            is_call: node
                                .parent()
                                .is_some_and(|parent| parent.kind() == spec.call),
                        });
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_attributes(
            spec,
            child,
            text,
            file_uri,
            entities,
            entity_index,
            unresolved,
            accesses,
        );
    }
}

fn node_text(node: Node, text: &str) -> String {
    text[node.byte_range()].to_string()
}

fn lexical_scope<'a>(spec: &InferenceSpec, mut node: Node<'a>) -> Node<'a> {
    while let Some(parent) = node.parent() {
        node = parent;
        if spec.scopes.contains(&node.kind()) {
            break;
        }
    }
    node
}

// Split refinement at assignments and control-flow boundaries. This deliberately avoids
// treating identical receiver text as proof of object identity across branches or rebinding.
fn binding_epoch(spec: &InferenceSpec, node: Node, object: Node, text: &str) -> usize {
    fn scan(
        spec: &InferenceSpec,
        node: Node,
        limit: usize,
        root: &str,
        text: &str,
        epoch: &mut usize,
    ) {
        if node.start_byte() >= limit {
            return;
        }
        if spec.rebindings.contains(&node.kind()) {
            let target = spec
                .rebinding_target_fields
                .iter()
                .find_map(|field| node.child_by_field_name(field));
            if target.is_none_or(|target| {
                node_text(target, text)
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|word| word == root)
            }) {
                *epoch = node.start_byte() + 1;
            }
        }
        if spec.boundaries.contains(&node.kind()) {
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            scan(spec, child, limit, root, text, epoch);
        }
    }
    let root = node_text(object, text);
    let root = root.split('.').next().unwrap_or(&root);
    let scope = lexical_scope(spec, node);
    let mut epoch = 0;
    let mut cursor = scope.walk();
    for child in scope.named_children(&mut cursor) {
        scan(spec, child, node.start_byte(), root, text, &mut epoch);
    }
    // Separate branch bodies as well as their following statements.
    let mut parent = node.parent();
    while let Some(p) = parent {
        if p.id() == scope.id() {
            break;
        }
        if p.kind() == spec.block {
            epoch = epoch.max(p.start_byte());
            break;
        }
        parent = p.parent();
    }
    epoch
}

fn call_arguments(spec: &InferenceSpec, node: Node) -> Option<usize> {
    let call = node.parent()?;
    if call.kind() != spec.call
        || call.child_by_field_name(spec.call_function_field)?.id() != node.id()
    {
        return None;
    }
    let args = call.child_by_field_name(spec.call_arguments_field)?;
    let mut cursor = args.walk();
    let args: Vec<_> = args.named_children(&mut cursor).collect();
    if args
        .iter()
        .any(|n| spec.uncountable_arguments.contains(&n.kind()))
    {
        return None;
    }
    Some(args.len())
}

// Extract positional arity once per source file; unknown/decorated signatures remain
// conservative rather than rejecting potentially valid calls.
fn signatures(
    spec: &InferenceSpec,
    files: &[(PathBuf, String)],
    language: &tree_sitter::Language,
    entities: &[Entity],
) -> Result<HashMap<usize, (usize, Option<usize>)>> {
    fn visit(
        spec: &InferenceSpec,
        node: Node,
        uri: &str,
        text: &str,
        entities: &[Entity],
        out: &mut HashMap<usize, (usize, Option<usize>)>,
    ) {
        if node.kind() == spec.function
            && !node
                .parent()
                .is_some_and(|p| spec.decorated_wrappers.contains(&p.kind()))
        {
            if let (Some(name), Some(parameters)) = (
                node.child_by_field_name(spec.function_name_field),
                node.child_by_field_name(spec.parameters_field),
            ) {
                if let Some((idx, _)) = entities.iter().enumerate().find(|(_, e)| {
                    e.file_uri == uri
                        && e.line_start == node.start_position().row as u32
                        && e.name == node_text(name, text)
                        && e.kind == EntityKind::Method
                }) {
                    let mut cursor = parameters.walk();
                    let params: Vec<_> = parameters.named_children(&mut cursor).collect();
                    if params
                        .iter()
                        .all(|p| spec.plain_parameters.contains(&p.kind()))
                    {
                        let minimum = params
                            .iter()
                            .skip(spec.implicit_parameters)
                            .filter(|p| !spec.optional_parameters.contains(&p.kind()))
                            .count();
                        out.insert(
                            idx,
                            (
                                minimum,
                                Some(params.len().saturating_sub(spec.implicit_parameters)),
                            ),
                        );
                    }
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            visit(spec, child, uri, text, entities, out);
        }
    }
    let mut parser = Parser::new();
    parser.set_language(language)?;
    let mut out = HashMap::new();
    for (path, text) in files {
        if let Some(tree) = parser.parse(text, None) {
            visit(
                spec,
                tree.root_node(),
                &path_to_uri(path),
                text,
                entities,
                &mut out,
            );
        }
    }
    Ok(out)
}

fn build_member_index(spec: &InferenceSpec, entities: &[Entity]) -> HashMap<String, Vec<usize>> {
    entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| {
            !(spec.is_private)(&entity.name)
                && matches!(entity.kind, EntityKind::Method | EntityKind::Field)
                && entity
                    .parent
                    .is_some_and(|parent| entities[parent].kind == EntityKind::Class)
        })
        .fold(HashMap::new(), |mut index, (idx, entity)| {
            index.entry(entity.name.clone()).or_default().push(idx);
            index
        })
}

fn build_class_index(entities: &[Entity]) -> HashMap<String, Vec<usize>> {
    entities
        .iter()
        .enumerate()
        .filter(|(_, entity)| entity.kind == EntityKind::Class)
        .fold(HashMap::new(), |mut index, (idx, entity)| {
            index.entry(entity.name.clone()).or_default().push(idx);
            index
        })
}

fn candidate_members(
    access: &Access,
    member_index: &HashMap<String, Vec<usize>>,
    entities: &[Entity],
) -> Vec<usize> {
    member_index
        .get(&access.member)
        .into_iter()
        .flatten()
        .copied()
        .filter(|idx| {
            matches!(entities[*idx].kind, EntityKind::Method)
                || (!access.is_call && entities[*idx].kind == EntityKind::Field)
        })
        .collect()
}

fn inheritance_map(deps: &[Dep]) -> HashMap<usize, Vec<usize>> {
    let mut result: HashMap<usize, Vec<usize>> = HashMap::new();
    for dep in deps.iter().filter(|d| d.kind == DepKind::Extend) {
        result.entry(dep.src).or_default().push(dep.tgt);
    }
    result
}

fn ancestors(owner: usize, inheritance: &HashMap<usize, Vec<usize>>) -> HashSet<usize> {
    let mut seen = HashSet::new();
    let mut pending = inheritance.get(&owner).cloned().unwrap_or_default();
    while let Some(parent) = pending.pop() {
        if parent != owner && seen.insert(parent) {
            pending.extend(inheritance.get(&parent).into_iter().flatten().copied());
        }
    }
    seen
}
