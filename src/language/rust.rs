use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::entity::{Dep, DepKind, Entity, EntityKind};
use crate::language::Language;

pub struct Rust;

impl Language for Rust {
    fn id(&self) -> &str {
        "rust"
    }

    fn extensions(&self) -> &[&str] {
        &[".rs"]
    }

    fn lsp_command(&self) -> Vec<String> {
        vec!["rust-analyzer".into()]
    }

    fn init_options(&self, _project_root: &Path) -> Value {
        json!({}) // rust-analyzer auto-discovers from Cargo.toml
    }

    fn symbol_kind_map(&self) -> HashMap<u32, EntityKind> {
        HashMap::from([
            (5, EntityKind::Class),      // Class / type alias
            (6, EntityKind::Method),     // Method (in impl blocks)
            (8, EntityKind::Field),      // Field (struct fields)
            (10, EntityKind::Enum),      // Enum
            (11, EntityKind::Interface), // Interface (traits)
            (12, EntityKind::Function),  // Function (free / associated fns)
            (23, EntityKind::Class),     // Struct
        ])
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn ref_queries(&self) -> &str {
        RUST_QUERIES
    }

    fn extra_deps(&self, entities: &[Entity], _deps: &[Dep]) -> Vec<Dep> {
        structural_use_deps(entities)
    }
}

fn structural_use_deps(entities: &[Entity]) -> Vec<Dep> {
    entities
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if !matches!(e.kind, EntityKind::Method | EntityKind::Constructor) {
                return None;
            }
            let parent_idx = e.parent?;
            if entities[parent_idx].kind == EntityKind::Class {
                Some(Dep {
                    src: i,
                    tgt: parent_idx,
                    kind: DepKind::Use,
                })
            } else {
                None
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Grammar notes that affect query syntax
// ─────────────────────────────────────────────────────────────────────────────
//
// Inline rules (no named node in AST — concrete child appears directly):
//   _path, _type_identifier, _field_identifier, _reserved_identifier,
//   _declaration_statement, _expression_ending_with_block, _tokens,
//   _non_special_token
//
// Supertype rules (concrete child type appears, not the supertype wrapper):
//   _type, _expression, _pattern, _literal, _literal_pattern,
//   _declaration_statement
//
// Consequence for queries:
//   - _type_identifier → use `type_identifier` in patterns
//   - _field_identifier → use `field_identifier` in patterns
//   - `attribute` has NO field name for its path child (inline _path) →
//       use (attribute (identifier)) not (attribute path: (identifier))
//   - `scoped_identifier.name` is `choice(identifier, super)` — use `identifier`
//   - `scoped_type_identifier.name` is `_type_identifier` (inline) → use `type_identifier`
//
// Disambiguation (pipeline.rs in_call_function_position):
//   - For `obj.method()`: field_expression is in call function position.
//     Only the 'field' slot (method name) is filtered; 'value' (receiver) keeps Use dep.
//   - For `Foo::bar()`: scoped_identifier is in call function position.
//     Only the 'name' slot (function name) is filtered; 'path' (type) keeps Use dep.
//
// ─────────────────────────────────────────────────────────────────────────────

const RUST_QUERIES: &str = r#"
; ══════════════════════════════════════════════════════════════════════════════
; Import
; ══════════════════════════════════════════════════════════════════════════════

; use Foo;
(use_declaration argument: (identifier) @Import.ref)

; use foo::Bar; — only the leaf name
(use_declaration argument: (scoped_identifier name: (identifier) @Import.ref))

; use foo::Bar as Baz; — capture real name, not alias
(use_declaration argument: (use_as_clause path: (identifier) @Import.ref))
(use_declaration argument: (use_as_clause path: (scoped_identifier name: (identifier) @Import.ref)))

; use std::io::{Read, Write};
(use_declaration argument: (scoped_use_list list: (use_list (identifier) @Import.ref)))
(use_declaration argument: (scoped_use_list list: (use_list (scoped_identifier name: (identifier) @Import.ref))))

; use {Foo, Bar};
(use_declaration argument: (use_list (identifier) @Import.ref))
(use_declaration argument: (use_list (scoped_identifier name: (identifier) @Import.ref)))

; extern crate foo;
(extern_crate_declaration name: (identifier) @Import.ref)

; ══════════════════════════════════════════════════════════════════════════════
; Implement — trait implementations (Rust's equivalent of class-implements-interface)
; ══════════════════════════════════════════════════════════════════════════════

; impl Display for Foo — captures Display (the trait Foo implements)
(impl_item trait: (type_identifier) @Implement.ref)
(impl_item trait: (scoped_type_identifier name: (type_identifier) @Implement.ref))
(impl_item trait: (generic_type type: (type_identifier) @Implement.ref))
(impl_item trait: (generic_type type: (scoped_type_identifier name: (type_identifier) @Implement.ref)))

; ══════════════════════════════════════════════════════════════════════════════
; Extend — trait supertrait bounds (trait Derived: Base — analogous to interface extends interface)
; ══════════════════════════════════════════════════════════════════════════════

(trait_item bounds: (trait_bounds (type_identifier) @Extend.ref))
(trait_item bounds: (trait_bounds (generic_type type: (type_identifier) @Extend.ref)))

; ══════════════════════════════════════════════════════════════════════════════
; Call
; ══════════════════════════════════════════════════════════════════════════════

; Direct function call: foo()
(call_expression function: (identifier) @Call.ref)

; Method call: obj.method()
; The method name (field_identifier) is filtered by in_call_function_position.
; The receiver (value) produces a Use dep (see Use section below).
(call_expression function: (field_expression field: (field_identifier) @Call.ref))

; Associated function / static method: Foo::bar() or module::func()
; The function name is filtered; the path 'Foo' produces a Use dep (see below).
(call_expression function: (scoped_identifier name: (identifier) @Call.ref))

; Turbofish: foo::<T>(), Foo::bar::<T>(), obj.method::<T>()
(call_expression function: (generic_function function: (identifier) @Call.ref))
(call_expression function: (generic_function function: (field_expression field: (field_identifier) @Call.ref)))
(call_expression function: (generic_function function: (scoped_identifier name: (identifier) @Call.ref)))

; Macro invocations: vec![], println!(), custom_macro!()
; Custom project macros will resolve to project entities.
(macro_invocation macro: (identifier) @Call.ref)
(macro_invocation macro: (scoped_identifier name: (identifier) @Call.ref))

; Struct literal construction: Foo { x: 1 }
; reclassify_creates promotes Call→Create when target resolves to Class/Constructor.
(struct_expression name: (type_identifier) @Call.ref)
(struct_expression name: (scoped_type_identifier name: (type_identifier) @Call.ref))

; ══════════════════════════════════════════════════════════════════════════════
; Use — type positions
; ══════════════════════════════════════════════════════════════════════════════

; Struct field types: name: Type, name: &Type, name: Vec<Type>, name: *const Type
(field_declaration type: (type_identifier) @Use.ref)
(field_declaration type: (reference_type type: (type_identifier) @Use.ref))
(field_declaration type: (pointer_type type: (type_identifier) @Use.ref))
(field_declaration type: (generic_type type: (type_identifier) @Use.ref))
(field_declaration type: (generic_type type_arguments: (type_arguments (type_identifier) @Use.ref)))
(field_declaration type: (array_type element: (type_identifier) @Use.ref))

; Tuple-like enum variant fields: Variant(Foo, Bar)
(ordered_field_declaration_list type: (type_identifier) @Use.ref)
(ordered_field_declaration_list type: (reference_type type: (type_identifier) @Use.ref))
(ordered_field_declaration_list type: (generic_type type: (type_identifier) @Use.ref))
(ordered_field_declaration_list type: (generic_type type_arguments: (type_arguments (type_identifier) @Use.ref)))

; let binding type: let x: Foo = ...
(let_declaration type: (type_identifier) @Use.ref)
(let_declaration type: (reference_type type: (type_identifier) @Use.ref))
(let_declaration type: (pointer_type type: (type_identifier) @Use.ref))
(let_declaration type: (generic_type type: (type_identifier) @Use.ref))
(let_declaration type: (generic_type type_arguments: (type_arguments (type_identifier) @Use.ref)))

; type alias: type Alias = Foo
(type_item type: (type_identifier) @Use.ref)
(type_item type: (generic_type type: (type_identifier) @Use.ref))
(type_item type: (generic_type type_arguments: (type_arguments (type_identifier) @Use.ref)))

; const / static: const C: Foo = ...; static S: Bar = ...;
(const_item type: (type_identifier) @Use.ref)
(const_item type: (generic_type type: (type_identifier) @Use.ref))
(static_item type: (type_identifier) @Use.ref)
(static_item type: (generic_type type: (type_identifier) @Use.ref))

; Generic type bounds: <T: Bound>, <T: Bound1 + Bound2>
(type_parameter bounds: (trait_bounds (type_identifier) @Use.ref))
(type_parameter bounds: (trait_bounds (generic_type type: (type_identifier) @Use.ref)))

; where clause bounds: where T: Bound
(where_predicate bounds: (trait_bounds (type_identifier) @Use.ref))
(where_predicate bounds: (trait_bounds (generic_type type: (type_identifier) @Use.ref)))

; Associated type constraint: T: Iterator<Item = Foo> — captures Foo
(type_binding type: (type_identifier) @Use.ref)
(type_binding type: (generic_type type: (type_identifier) @Use.ref))

; Associated type declaration in trait body: type Output: Foo
(associated_type bounds: (trait_bounds (type_identifier) @Use.ref))

; Pattern matching — struct: match x { MyStruct { field } => ... }
(struct_pattern type: (type_identifier) @Use.ref)
(struct_pattern type: (scoped_type_identifier name: (type_identifier) @Use.ref))

; Pattern matching — tuple struct / enum variant: match x { MyError(msg) => ... }
; Note: tuple_struct_pattern.type uses identifier (expression position), not type_identifier
(tuple_struct_pattern type: (identifier) @Use.ref)
(tuple_struct_pattern type: (scoped_identifier name: (identifier) @Use.ref))

; ══════════════════════════════════════════════════════════════════════════════
; Use — expression / value positions
; ══════════════════════════════════════════════════════════════════════════════

; Qualified path prefix: MyEnum::Variant, MyStruct::CONST, Foo::new()
; in_call_function_position allows path slot through even when scoped_identifier
; is in call function position (only 'name' slot is filtered, not 'path').
(scoped_identifier path: (identifier) @Use.ref)

; Field access receiver: obj.field or self.field
; in_call_function_position allows 'value' slot through even in obj.method() calls.
(field_expression value: (identifier) @Use.ref)

; Field access name: obj.field_name → dep to struct field declaration
; in_call_function_position filters this when field_expression is the call function.
(field_expression field: (field_identifier) @Use.ref)

; ══════════════════════════════════════════════════════════════════════════════
; Assignment Uses — writes are generalized as Use in output
; ══════════════════════════════════════════════════════════════════════════════

; x = v
(assignment_expression left: (identifier) @Use.ref)

; self.x = v or obj.x = v
(assignment_expression left: (field_expression field: (field_identifier) @Use.ref))
(assignment_expression left: (field_expression value: (identifier) @Use.ref))

; x += v, x -= v, etc.
(compound_assignment_expr left: (identifier) @Use.ref)
(compound_assignment_expr left: (field_expression field: (field_identifier) @Use.ref))

; ══════════════════════════════════════════════════════════════════════════════
; Parameter — formal parameter types
; ══════════════════════════════════════════════════════════════════════════════

; Simple: fn foo(x: Bar)
(parameter type: (type_identifier) @Parameter.ref)

; Reference: fn foo(x: &Bar) / &mut Bar
(parameter type: (reference_type type: (type_identifier) @Parameter.ref))

; Raw pointer parameter: fn foo(x: *const Bar)
(parameter type: (pointer_type type: (type_identifier) @Parameter.ref))

; Generic base: fn foo(x: Vec<Bar>) → Vec
(parameter type: (generic_type type: (type_identifier) @Parameter.ref))

; Generic type arg: fn foo(x: Vec<Bar>) → Bar
(parameter type: (generic_type type_arguments: (type_arguments (type_identifier) @Parameter.ref)))

; impl Trait parameter: fn foo(x: impl Display) → Display
(parameter type: (abstract_type trait: (type_identifier) @Parameter.ref))

; ══════════════════════════════════════════════════════════════════════════════
; Return — function/closure return types (also emit Parameter for generic args)
; ══════════════════════════════════════════════════════════════════════════════

; fn foo() -> Bar  [covers both function_item and function_signature_item (trait methods)]
(function_item return_type: (type_identifier) @Return.ref)
(function_signature_item return_type: (type_identifier) @Return.ref)

; fn foo() -> &Bar
(function_item return_type: (reference_type type: (type_identifier) @Return.ref))
(function_signature_item return_type: (reference_type type: (type_identifier) @Return.ref))

; fn foo() -> Vec<Bar> → Vec
(function_item return_type: (generic_type type: (type_identifier) @Return.ref))
(function_signature_item return_type: (generic_type type: (type_identifier) @Return.ref))

; fn foo() -> Vec<Bar> → Bar
(function_item return_type: (generic_type type_arguments: (type_arguments (type_identifier) @Return.ref)))
(function_signature_item return_type: (generic_type type_arguments: (type_arguments (type_identifier) @Return.ref)))

; fn foo() -> impl Display  (impl Trait return)
(function_item return_type: (abstract_type trait: (type_identifier) @Return.ref))
(function_signature_item return_type: (abstract_type trait: (type_identifier) @Return.ref))

; fn foo() -> Box<dyn Error>  (dyn Trait return)
(function_item return_type: (dynamic_type trait: (type_identifier) @Return.ref))
(function_signature_item return_type: (dynamic_type trait: (type_identifier) @Return.ref))

; Closure return: |x| -> Bar { ... }
(closure_expression return_type: (type_identifier) @Return.ref)
(closure_expression return_type: (reference_type type: (type_identifier) @Return.ref))
(closure_expression return_type: (generic_type type: (type_identifier) @Return.ref))

; ══════════════════════════════════════════════════════════════════════════════
; Annotation — attribute macros and derives
; ══════════════════════════════════════════════════════════════════════════════
; Note: `attribute`'s path child is an inlined `_path` with NO field name.
; Query must use (attribute (identifier)) not (attribute path: (identifier)).

; #[derive(Debug)], #[serde(rename_all = "...")], #[my_proc_macro]
(attribute_item (attribute (identifier) @Annotation.ref))
(attribute_item (attribute (scoped_identifier name: (identifier) @Annotation.ref)))

; #![crate_type = "lib"]  (inner attributes)
(inner_attribute_item (attribute (identifier) @Annotation.ref))
(inner_attribute_item (attribute (scoped_identifier name: (identifier) @Annotation.ref)))

; ══════════════════════════════════════════════════════════════════════════════
; Cast — as expressions
; ══════════════════════════════════════════════════════════════════════════════

; x as Foo, x as &Foo, x as Vec<Foo>
(type_cast_expression type: (type_identifier) @Cast.ref)
(type_cast_expression type: (reference_type type: (type_identifier) @Cast.ref))
(type_cast_expression type: (pointer_type type: (type_identifier) @Cast.ref))
(type_cast_expression type: (generic_type type: (type_identifier) @Cast.ref))
"#;
