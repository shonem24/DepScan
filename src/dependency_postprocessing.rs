use std::collections::HashMap;

use crate::entity::{Dep, DepKind, Entity, EntityKind};
use crate::language::Language;

pub fn post_process_deps(raw_deps: Vec<Dep>, entities: &[Entity], lang: &dyn Language) -> Vec<Dep> {
    let extend_deps: Vec<Dep> = raw_deps
        .iter()
        .filter(|d| d.kind == DepKind::Extend)
        .cloned()
        .collect();
    let mut final_deps = reclassify_creates(raw_deps, entities);
    final_deps.extend(detect_overrides(&extend_deps, entities));
    final_deps.extend(lang.extra_deps(entities, &final_deps));
    final_deps = lang.post_process_deps(final_deps, entities);
    filter_ownership_use_deps(final_deps, entities)
}

fn reclassify_creates(deps: Vec<Dep>, entities: &[Entity]) -> Vec<Dep> {
    deps.into_iter()
        .map(|d| {
            let is_ctor_or_class = matches!(
                entities[d.tgt].kind,
                EntityKind::Constructor | EntityKind::Class
            );
            if d.kind == DepKind::Call && is_ctor_or_class {
                // When jdtls resolves `new X(...)` it lands on the constructor definition.
                // Normalise: remap Constructor target → its parent Class so that
                // Create(src, Constructor) becomes Create(src, Class), matching the
                // semantics "src creates an instance of class X".
                let tgt = if entities[d.tgt].kind == EntityKind::Constructor {
                    entities[d.tgt].parent.unwrap_or(d.tgt)
                } else {
                    d.tgt
                };
                Dep {
                    kind: DepKind::Create,
                    tgt,
                    ..d
                }
            } else {
                d
            }
        })
        .collect()
}

fn detect_overrides(extend_deps: &[Dep], entities: &[Entity]) -> Vec<Dep> {
    let parent_map: HashMap<usize, usize> = extend_deps.iter().map(|d| (d.src, d.tgt)).collect();

    extend_deps
        .iter()
        .flat_map(|ext| {
            let anc = ancestor_methods(ext.src, &parent_map, entities);
            entities[ext.src]
                .children
                .iter()
                .copied()
                .filter_map(move |child_idx| {
                    let child = &entities[child_idx];
                    if child.kind != EntityKind::Method {
                        return None;
                    }
                    anc.get(child.name.as_str()).map(|&anc_idx| Dep {
                        src: child_idx,
                        tgt: anc_idx,
                        kind: DepKind::Override,
                    })
                })
        })
        .collect()
}

fn filter_ownership_use_deps(deps: Vec<Dep>, entities: &[Entity]) -> Vec<Dep> {
    deps.into_iter()
        .filter(|d| !is_ownership_use_dep(d, entities))
        .collect()
}

fn is_ownership_use_dep(dep: &Dep, entities: &[Entity]) -> bool {
    if dep.kind != DepKind::Use {
        return false;
    }

    let src = &entities[dep.src];
    let tgt = &entities[dep.tgt];
    matches!(src.kind, EntityKind::Method | EntityKind::Constructor)
        && matches!(
            tgt.kind,
            EntityKind::Class | EntityKind::Interface | EntityKind::Enum
        )
        && src.parent == Some(dep.tgt)
}

fn ancestor_methods<'e>(
    cls_idx: usize,
    parent_map: &HashMap<usize, usize>,
    entities: &'e [Entity],
) -> HashMap<&'e str, usize> {
    let mut methods: HashMap<&'e str, usize> = HashMap::new();
    let mut cur = parent_map.get(&cls_idx).copied();
    while let Some(anc_idx) = cur {
        for &child_idx in &entities[anc_idx].children {
            let child = &entities[child_idx];
            if child.kind == EntityKind::Method {
                methods.entry(child.name.as_str()).or_insert(child_idx);
            }
        }
        cur = parent_map.get(&anc_idx).copied();
    }
    methods
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::registry;
    use serde_json::Value;
    use std::path::Path;

    fn entity(kind: EntityKind, parent: Option<usize>) -> Entity {
        Entity {
            var_name: String::new(),
            name: String::new(),
            kind,
            line_start: 0,
            line_end: 0,
            file_uri: String::new(),
            parent,
            children: Vec::new(),
        }
    }

    fn dep(src: usize, tgt: usize, kind: DepKind) -> Dep {
        Dep { src, tgt, kind }
    }

    fn strategy_entities() -> Vec<Entity> {
        vec![
            entity(EntityKind::File, None),
            entity(EntityKind::Class, Some(0)),
            entity(EntityKind::Method, Some(1)),
        ]
    }

    #[test]
    fn filters_only_member_to_own_type_use_deps() {
        let entities = vec![
            entity(EntityKind::File, None),
            entity(EntityKind::Class, Some(0)),
            entity(EntityKind::Method, Some(1)),
            entity(EntityKind::Constructor, Some(1)),
            entity(EntityKind::Field, Some(1)),
            entity(EntityKind::Method, Some(1)),
            entity(EntityKind::Class, Some(0)),
            entity(EntityKind::Field, Some(6)),
        ];

        let deps = vec![
            dep(2, 1, DepKind::Use),
            dep(3, 1, DepKind::Use),
            dep(2, 4, DepKind::Use),
            dep(2, 5, DepKind::Call),
            dep(4, 1, DepKind::Use),
            dep(2, 6, DepKind::Use),
            dep(7, 6, DepKind::Use),
        ];

        let filtered = filter_ownership_use_deps(deps, &entities);
        assert_eq!(filtered.len(), 5);
        assert!(filtered
            .iter()
            .any(|d| d.src == 2 && d.tgt == 4 && d.kind == DepKind::Use));
        assert!(filtered
            .iter()
            .any(|d| d.src == 2 && d.tgt == 5 && d.kind == DepKind::Call));
        assert!(filtered
            .iter()
            .any(|d| d.src == 4 && d.tgt == 1 && d.kind == DepKind::Use));
        assert!(filtered
            .iter()
            .any(|d| d.src == 2 && d.tgt == 6 && d.kind == DepKind::Use));
        assert!(filtered
            .iter()
            .any(|d| d.src == 7 && d.tgt == 6 && d.kind == DepKind::Use));
        assert!(!filtered
            .iter()
            .any(|d| d.src == 2 && d.tgt == 1 && d.kind == DepKind::Use));
        assert!(!filtered
            .iter()
            .any(|d| d.src == 3 && d.tgt == 1 && d.kind == DepKind::Use));
    }

    #[test]
    fn only_rust_strategy_adds_structural_extra_deps() {
        let entities = strategy_entities();
        let deps = Vec::new();

        // Driven by registry(): a newly registered language is covered
        // automatically, and fails here if it starts adding extra deps.
        let languages = registry();
        for (id, language) in &languages {
            if *id != "rust" {
                assert!(
                    language.extra_deps(&entities, &deps).is_empty(),
                    "{id} should not add structural extra deps"
                );
            }
        }

        let rust_extra = languages["rust"].extra_deps(&entities, &deps);
        assert_eq!(rust_extra.len(), 1);
        assert_eq!(rust_extra[0].src, 2);
        assert_eq!(rust_extra[0].tgt, 1);
        assert_eq!(rust_extra[0].kind, DepKind::Use);
    }

    struct TestLanguage;

    impl Language for TestLanguage {
        fn id(&self) -> &str {
            "test"
        }

        fn extensions(&self) -> &[&str] {
            &[]
        }

        fn lsp_command(&self) -> Vec<String> {
            Vec::new()
        }

        fn init_options(&self, _project_root: &Path) -> Value {
            Value::Null
        }

        fn symbol_kind_map(&self) -> HashMap<u32, EntityKind> {
            HashMap::new()
        }

        fn ts_language(&self) -> tree_sitter::Language {
            panic!("not used by post-processing tests")
        }

        fn ref_queries(&self) -> &str {
            ""
        }

        fn extra_deps(&self, _entities: &[Entity], deps: &[Dep]) -> Vec<Dep> {
            assert!(deps.iter().any(|d| d.kind == DepKind::Create));
            vec![dep(2, 1, DepKind::Use), dep(2, 4, DepKind::Use)]
        }

        fn post_process_deps(&self, mut deps: Vec<Dep>, _entities: &[Entity]) -> Vec<Dep> {
            assert!(deps
                .iter()
                .any(|d| d.src == 2 && d.tgt == 4 && d.kind == DepKind::Use));
            deps.push(dep(3, 1, DepKind::Use));
            deps.push(dep(2, 5, DepKind::Call));
            deps
        }
    }

    #[test]
    fn post_processing_runs_language_hooks_before_common_ownership_filter() {
        let entities = vec![
            entity(EntityKind::File, None),
            entity(EntityKind::Class, Some(0)),
            entity(EntityKind::Method, Some(1)),
            entity(EntityKind::Constructor, Some(1)),
            entity(EntityKind::Field, Some(1)),
            entity(EntityKind::Method, Some(1)),
        ];
        let raw_deps = vec![dep(2, 3, DepKind::Call)];

        let processed = post_process_deps(raw_deps, &entities, &TestLanguage);

        assert!(processed
            .iter()
            .any(|d| d.src == 2 && d.tgt == 1 && d.kind == DepKind::Create));
        assert!(processed
            .iter()
            .any(|d| d.src == 2 && d.tgt == 4 && d.kind == DepKind::Use));
        assert!(processed
            .iter()
            .any(|d| d.src == 2 && d.tgt == 5 && d.kind == DepKind::Call));
        assert!(!processed
            .iter()
            .any(|d| d.src == 2 && d.tgt == 1 && d.kind == DepKind::Use));
        assert!(!processed
            .iter()
            .any(|d| d.src == 3 && d.tgt == 1 && d.kind == DepKind::Use));
    }

    #[test]
    fn language_queries_do_not_emit_set_dependencies() {
        for language in registry().values() {
            assert!(
                !language.ref_queries().contains("@Set."),
                "{} should model assignment targets as Use, not Set",
                language.id()
            );
        }
    }
}
