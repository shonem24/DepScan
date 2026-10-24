use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::entity::EntityKind;
use crate::language::Language;

pub struct TypeScript;

impl Language for TypeScript {
    fn id(&self) -> &str {
        "typescript"
    }

    fn extensions(&self) -> &[&str] {
        &[".ts"]
    }

    fn lsp_command(&self) -> Vec<String> {
        vec!["typescript-language-server".into(), "--stdio".into()]
    }

    fn init_options(&self, _project_root: &Path) -> Value {
        json!({})
    }

    fn symbol_kind_map(&self) -> HashMap<u32, EntityKind> {
        HashMap::from([
            (5, EntityKind::Class),
            (6, EntityKind::Method),
            (7, EntityKind::Field), // Property
            (8, EntityKind::Field),
            (9, EntityKind::Constructor),
            (10, EntityKind::Enum),
            (11, EntityKind::Interface),
            (12, EntityKind::Function),
            (13, EntityKind::Field), // Variable
            (14, EntityKind::Field), // Constant
            (22, EntityKind::Enum),  // EnumMember
        ])
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn ref_queries(&self) -> &str {
        TYPESCRIPT_QUERIES
    }
}

const TYPESCRIPT_QUERIES: &str = r#"
; ── Import ──────────────────────────────────────────────────────────────────

; Named imports: import { Animal, Dog as D } from "./mod"
(import_statement
  (import_clause
    (named_imports
      (import_specifier
        name: (identifier) @Import.ref))))

; Default import: import Animal from "./mod"
(import_statement
  (import_clause
    (identifier) @Import.ref))

; Star / bare import: import * as Ns from "./mod" or import "./mod"
; Source string resolves to the module file via LSP
(import_statement
  source: (string) @Import.ref)

; Re-export with source: export { Foo } from "./mod"
(export_statement
  source: (string) @Import.ref)

; ── Extend ───────────────────────────────────────────────────────────────────

; class Dog extends Animal
(extends_clause
  (identifier) @Extend.ref)

; class Dog extends foo.Animal
(extends_clause
  (member_expression) @Extend.ref)

; interface I extends A
(extends_type_clause
  type: (type_identifier) @Extend.ref)

; interface I extends NS.A
(extends_type_clause
  type: (nested_type_identifier
    name: (type_identifier) @Extend.ref))

; interface I extends Array<T> — capture the generic base
(extends_type_clause
  type: (generic_type
    name: (type_identifier) @Extend.ref))

; ── Implement ────────────────────────────────────────────────────────────────

; class Dog implements IFeedable
(implements_clause
  (type_identifier) @Implement.ref)

; class Dog implements NS.IFeedable
(implements_clause
  (nested_type_identifier
    name: (type_identifier) @Implement.ref))

; class Dog implements Repository<Animal>
(implements_clause
  (generic_type
    name: (type_identifier) @Implement.ref))

; ── Call ─────────────────────────────────────────────────────────────────────

; Direct call: foo(args)
(call_expression
  function: (identifier) @Call.ref)

; Method call: obj.method(args) — capture the method name
(call_expression
  function: (member_expression
    property: (property_identifier) @Call.ref))

; Constructor call: new Foo(args) — reclassify_creates promotes Call→Create
(new_expression
  constructor: (identifier) @Call.ref)

; new foo.Bar(args)
(new_expression
  constructor: (member_expression
    property: (property_identifier) @Call.ref))

; super(args) constructor invocation
(call_expression
  function: (super) @Call.ref)

; super.method(args) handled by the member_expression query above

; ── Annotation (decorators) ──────────────────────────────────────────────────

; @Foo
(decorator
  (identifier) @Annotation.ref)

; @Foo.Bar
(decorator
  (member_expression
    property: (property_identifier) @Annotation.ref))

; @Foo(args)
(decorator
  (call_expression
    function: (identifier) @Annotation.ref))

; @Foo.Bar(args)
(decorator
  (call_expression
    function: (member_expression
      property: (property_identifier) @Annotation.ref)))

; ── Use ──────────────────────────────────────────────────────────────────────

; Field/property read: foo.bar (not in call position — filtered by in_call_function_position)
(member_expression
  property: (property_identifier) @Use.ref)

; Receiver: foo in foo.bar (kept by in_call_function_position after member_expression fix)
(member_expression
  object: (identifier) @Use.ref)

; typeof Foo in type position
(type_query
  (identifier) @Use.ref)

; this keyword may resolve to the enclosing class; ownership-only member →
; ownClass Use edges are filtered later.
(this) @Use.ref

; Identifier reads in expression contexts
(return_statement (identifier) @Use.ref)
(arguments (identifier) @Use.ref)
(binary_expression left:  (identifier) @Use.ref)
(binary_expression right: (identifier) @Use.ref)
(unary_expression  (identifier) @Use.ref)
(await_expression  (identifier) @Use.ref)
(spread_element    (identifier) @Use.ref)
(array             (identifier) @Use.ref)
(ternary_expression condition:   (identifier) @Use.ref)
(ternary_expression consequence: (identifier) @Use.ref)
(ternary_expression alternative: (identifier) @Use.ref)
(variable_declarator value: (identifier) @Use.ref)
(pair value: (identifier) @Use.ref)

; instanceof check: foo instanceof Bar
(binary_expression
  right: (identifier) @Use.ref)

; Type references in annotations (also Use for coverage)
(required_parameter  type: (type_annotation (type_identifier) @Use.ref))
(optional_parameter  type: (type_annotation (type_identifier) @Use.ref))
(public_field_definition type: (type_annotation (type_identifier) @Use.ref))
(variable_declarator     type: (type_annotation (type_identifier) @Use.ref))

; ── Assignment Uses ──────────────────────────────────────────────────────────

; foo = value — writes are generalized as Use in output
(assignment_expression left: (identifier)           @Use.ref)

; this.foo = value
(assignment_expression left: (member_expression property: (property_identifier) @Use.ref))

; foo += value
(augmented_assignment_expression left: (identifier)           @Use.ref)

(augmented_assignment_expression left: (member_expression property: (property_identifier) @Use.ref))

; ── Parameter ────────────────────────────────────────────────────────────────

; Simple parameter type: (x: Foo) — also Use
(required_parameter type: (type_annotation (type_identifier) @Parameter.ref))
(optional_parameter type: (type_annotation (type_identifier) @Parameter.ref))

; Generic base: (x: Array<Foo>) → Array
(required_parameter type: (type_annotation (generic_type name: (type_identifier) @Parameter.ref)))
(optional_parameter type: (type_annotation (generic_type name: (type_identifier) @Parameter.ref)))

; Generic type argument: (x: Array<Foo>) → Foo
(required_parameter type: (type_annotation (generic_type (type_arguments (type_identifier) @Parameter.ref))))
(optional_parameter type: (type_annotation (generic_type (type_arguments (type_identifier) @Parameter.ref))))

; Class field type: field: Foo
(public_field_definition type: (type_annotation (type_identifier) @Parameter.ref))
(public_field_definition type: (type_annotation (generic_type name: (type_identifier) @Parameter.ref)))

; Variable declaration type: let x: Foo
(variable_declarator type: (type_annotation (type_identifier) @Parameter.ref))
(variable_declarator type: (type_annotation (generic_type name: (type_identifier) @Parameter.ref)))

; ── Return ───────────────────────────────────────────────────────────────────

; Function/method return type: function foo(): Foo
(function_declaration    return_type: (type_annotation (type_identifier) @Return.ref))
(method_definition       return_type: (type_annotation (type_identifier) @Return.ref))
(arrow_function          return_type: (type_annotation (type_identifier) @Return.ref))
(function_expression     return_type: (type_annotation (type_identifier) @Return.ref))

; Generic return: foo(): Array<Foo> → Array
(function_declaration    return_type: (type_annotation (generic_type name: (type_identifier) @Return.ref)))
(method_definition       return_type: (type_annotation (generic_type name: (type_identifier) @Return.ref)))
(arrow_function          return_type: (type_annotation (generic_type name: (type_identifier) @Return.ref)))

; Generic return type argument: → Return + Parameter per project conventions
(function_declaration    return_type: (type_annotation (generic_type (type_arguments (type_identifier) @Return.ref))))
(function_declaration    return_type: (type_annotation (generic_type (type_arguments (type_identifier) @Parameter.ref))))
(method_definition       return_type: (type_annotation (generic_type (type_arguments (type_identifier) @Return.ref))))
(method_definition       return_type: (type_annotation (generic_type (type_arguments (type_identifier) @Parameter.ref))))

; Interface method signature / call signature
(method_signature return_type: (type_annotation (type_identifier) @Return.ref))
(call_signature   return_type: (type_annotation (type_identifier) @Return.ref))
(method_signature return_type: (type_annotation (generic_type name: (type_identifier) @Return.ref)))
(call_signature   return_type: (type_annotation (generic_type name: (type_identifier) @Return.ref)))

; ── Cast ─────────────────────────────────────────────────────────────────────

; expr as T (both dialects)
(as_expression (type_identifier) @Cast.ref)
(as_expression (generic_type name: (type_identifier) @Cast.ref))

; expr satisfies T (TypeScript 4.9+)
(satisfies_expression (type_identifier) @Cast.ref)
(satisfies_expression (generic_type name: (type_identifier) @Cast.ref))

; <T>expr (typescript dialect, not tsx)
(type_assertion (type_arguments (type_identifier) @Cast.ref))

; ── Throw ────────────────────────────────────────────────────────────────────

; throw new FooError()
(throw_statement (new_expression constructor: (identifier) @Throw.ref))

; throw new foo.FooError()
(throw_statement (new_expression constructor: (member_expression property: (property_identifier) @Throw.ref)))

; throw errorVariable
(throw_statement (identifier) @Throw.ref)
"#;
