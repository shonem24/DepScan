use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::entity::EntityKind;
use crate::language::Language;
use crate::possible_dependencies::InferenceSpec;

pub struct Python;

static INFERENCE: InferenceSpec = InferenceSpec {
    member_access: "attribute",
    member_object_field: "object",
    member_name_field: "attribute",
    identifier: "identifier",
    call: "call",
    call_function_field: "function",
    call_arguments_field: "arguments",
    uncountable_arguments: &["keyword_argument", "list_splat", "dictionary_splat"],
    scopes: &[
        "function_definition",
        "lambda",
        "class_definition",
        "module",
        "list_comprehension",
        "dictionary_comprehension",
        "set_comprehension",
        "generator_expression",
    ],
    rebindings: &[
        "assignment",
        "augmented_assignment",
        "named_expression",
        "for_statement",
        "with_item",
        "except_clause",
        "if_statement",
        "match_statement",
    ],
    rebinding_target_fields: &["left", "name"],
    boundaries: &["function_definition", "class_definition", "lambda"],
    block: "block",
    function: "function_definition",
    class: "class_definition",
    assignment: "assignment",
    assignment_left_field: "left",
    assignment_right_field: "right",
    return_statement: "return_statement",
    decorated_wrappers: &["decorated_definition"],
    function_name_field: "name",
    parameters_field: "parameters",
    plain_parameters: &[
        "identifier",
        "typed_parameter",
        "default_parameter",
        "typed_default_parameter",
    ],
    optional_parameters: &["default_parameter", "typed_default_parameter"],
    implicit_parameters: 1,
    is_private: |name| name.starts_with('_'),
};

impl Language for Python {
    fn inference_spec(&self) -> Option<&'static InferenceSpec> {
        Some(&INFERENCE)
    }

    fn id(&self) -> &str {
        "python"
    }

    fn extensions(&self) -> &[&str] {
        &[".py"]
    }

    fn lsp_command(&self) -> Vec<String> {
        vec!["pyright-langserver".into(), "--stdio".into()]
    }

    fn init_options(&self, project_root: &Path) -> Value {
        let python = std::env::var("PYTHON_PATH").unwrap_or_else(|_| "python3".into());
        json!({
            "pythonPath": python,
            "extraPaths": [project_root.to_string_lossy().as_ref()],
        })
    }

    fn symbol_kind_map(&self) -> HashMap<u32, EntityKind> {
        HashMap::from([
            (5, EntityKind::Class),
            (6, EntityKind::Method),
            (7, EntityKind::Field),
            (8, EntityKind::Field),
            (9, EntityKind::Constructor),
            (10, EntityKind::Enum),
            (11, EntityKind::Interface),
            (12, EntityKind::Function),
            (13, EntityKind::Field),
            (23, EntityKind::Enum),
        ])
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn ref_queries(&self) -> &str {
        r"
(import_from_statement (dotted_name (identifier) @Import.ref .))
(import_from_statement (aliased_import (dotted_name (identifier) @Import.ref .)))
(import_statement (dotted_name (identifier) @Import.ref .))
(import_statement (aliased_import (dotted_name (identifier) @Import.ref .)))
(class_definition superclasses: (argument_list (identifier) @Extend.ref))
(class_definition superclasses: (argument_list (attribute attribute: (identifier) @Extend.ref)))
(call function: (identifier) @Call.ref)
(call function: (attribute attribute: (identifier) @Call.ref))
(typed_parameter type: (type (identifier) @Parameter.ref))
(typed_default_parameter type: (type (identifier) @Parameter.ref))
(typed_parameter type: (type (attribute attribute: (identifier) @Parameter.ref)))
(typed_default_parameter type: (type (attribute attribute: (identifier) @Parameter.ref)))
(typed_parameter type: (type (generic_type (identifier) @Parameter.ref)))
(typed_default_parameter type: (type (generic_type (identifier) @Parameter.ref)))
(function_definition return_type: (type (identifier) @Return.ref))
(function_definition return_type: (type (attribute attribute: (identifier) @Return.ref)))
(function_definition return_type: (type (generic_type (identifier) @Return.ref)))
(return_statement (call function: (identifier) @Return.ref))
(return_statement (call function: (attribute attribute: (identifier) @Return.ref)))
(return_statement (identifier) @Return.ref)
(return_statement (attribute object: (identifier) @Return.ref))
(return_statement (attribute object: (identifier) @Use.ref))
(decorator (identifier) @Annotation.ref)
(decorator (attribute attribute: (identifier) @Annotation.ref))
(decorator (call function: (identifier) @Annotation.ref))
(decorator (call function: (attribute attribute: (identifier) @Annotation.ref)))
(raise_statement (call function: (identifier) @Throw.ref))
(attribute attribute: (identifier) @Use.ref)
(for_statement right: (identifier) @Use.ref)
(for_statement right: (attribute attribute: (identifier) @Use.ref))
(argument_list (identifier) @Use.ref)
(call function: (attribute object: (identifier) @Use.ref))
"
    }
}
