use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::entity::{Dep, Entity, EntityKind};
use crate::entity_extractor::EntityIndex;
use crate::possible_dependencies::{
    infer_possible_deps, InferenceSpec, PossibleDep, UnresolvedReference,
};

/// Borrowed inputs shared by language-specific inference strategies.
pub struct InferenceContext<'a> {
    pub file_texts: &'a [(PathBuf, String)],
    pub ts_language: &'a tree_sitter::Language,
    pub entities: &'a [Entity],
    pub entity_index: &'a EntityIndex,
    pub unresolved: &'a [UnresolvedReference],
    pub explicit_deps: &'a [Dep],
    /// File that receives the unmapped-reference report.
    pub unmapped_path: &'a Path,
}

pub mod java;
pub mod python;
pub mod rust;
pub mod typescript;

pub trait Language: Send + Sync {
    fn id(&self) -> &str;
    fn extensions(&self) -> &[&str];
    fn lsp_command(&self) -> Vec<String>;
    fn init_options(&self, project_root: &Path) -> Value;
    fn symbol_kind_map(&self) -> HashMap<u32, EntityKind>;
    fn ts_language(&self) -> tree_sitter::Language;
    fn ref_queries(&self) -> &str;

    /// Describes this language's syntax for possible-dependency inference. Languages that
    /// return `None` contribute no possible edges.
    fn inference_spec(&self) -> Option<&'static InferenceSpec> {
        None
    }

    /// Runs the shared inference engine with this language's spec. Override only to replace
    /// the engine entirely.
    fn infer_possible_dependencies(
        &self,
        context: &InferenceContext<'_>,
    ) -> anyhow::Result<Vec<PossibleDep>> {
        match self.inference_spec() {
            Some(spec) => infer_possible_deps(
                spec,
                context.file_texts,
                context.ts_language,
                context.entities,
                context.entity_index,
                context.unresolved,
                context.explicit_deps,
                context.unmapped_path,
            ),
            None => Ok(Vec::new()),
        }
    }

    fn extra_deps(&self, _entities: &[Entity], _deps: &[Dep]) -> Vec<Dep> {
        Vec::new()
    }

    fn post_process_deps(&self, deps: Vec<Dep>, _entities: &[Entity]) -> Vec<Dep> {
        deps
    }
}

pub fn registry() -> HashMap<&'static str, Box<dyn Language>> {
    let mut map: HashMap<&'static str, Box<dyn Language>> = HashMap::new();
    map.insert("python", Box::new(python::Python));
    map.insert("java", Box::new(java::Java));
    map.insert("rust", Box::new(rust::Rust));
    map.insert("typescript", Box::new(typescript::TypeScript));
    map
}
