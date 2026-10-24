use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::entity::EntityKind;
use crate::language::Language;

pub struct Java;

impl Language for Java {
    fn id(&self) -> &str {
        "java"
    }

    fn extensions(&self) -> &[&str] {
        &[".java"]
    }

    fn lsp_command(&self) -> Vec<String> {
        // jdtls keeps its workspace index here; temp_dir() works on every OS.
        let data = std::env::temp_dir().join("lsp-dep-java");
        vec![
            "jdtls".into(),
            "-data".into(),
            data.to_string_lossy().into_owned(),
        ]
    }

    fn init_options(&self, _project_root: &Path) -> Value {
        json!({}) // jdtls auto-discovers source roots from package declarations
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
            (13, EntityKind::Field),
        ])
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn ref_queries(&self) -> &str {
        JAVA_QUERIES
    }
}

const JAVA_QUERIES: &str = r#"
; Import — only the leaf name (e.g. ArrayList from java.util.ArrayList)
(import_declaration (scoped_identifier name: (identifier) @Import.ref))
(import_declaration (identifier) @Import.ref)

; Extend — class inherits from a superclass
(class_declaration superclass: (superclass (type_identifier) @Extend.ref))

; Implement — class implements interface(s) (distinct from Extend per spec)
(class_declaration interfaces: (super_interfaces (type_list (type_identifier) @Implement.ref)))

; Extend — interface extends interface(s) (true inheritance, not implementation)
(interface_declaration (extends_interfaces (type_list (type_identifier) @Extend.ref)))

; Call — method invocations; name field never overlaps with field_access, so no disambiguation needed
(method_invocation name: (identifier) @Call.ref)

; Call — object creation; reclassify_creates promotes Call→Create when target is Constructor/Class
(object_creation_expression type: (type_identifier) @Call.ref)

; Use — explicit field access: obj.field or this.field
(field_access field: (identifier) @Use.ref)

; Use — bare field as method-call receiver: bookedTickets.add(...)
; LSP resolves fields to their declaration; local variables resolve outside project root and are dropped
(method_invocation object: (identifier) @Use.ref)

; Use — bare field read in return statement: return name;
(return_statement (identifier) @Use.ref)

; Use — bare field in binary expression: "Station: " + name, total + fee
(binary_expression left:  (identifier) @Use.ref)
(binary_expression right: (identifier) @Use.ref)

; Use — bare field passed as method argument: train.releaseSeat(seatNumber)
(argument_list (identifier) @Use.ref)

; Use — bare field in increment/decrement: ticketCounter++
(update_expression (identifier) @Use.ref)

; Use — class reference in local variable declaration: Train t = new Train(...)
(local_variable_declaration type: (type_identifier) @Use.ref)
(local_variable_declaration type: (generic_type (type_identifier) @Use.ref))

; Use — class reference in enhanced-for loop: for (Train t : trains)
(enhanced_for_statement type: (type_identifier) @Use.ref)

; Use — iterable field in enhanced-for loop: for (Train t : trains) → trains field
(enhanced_for_statement value: (identifier) @Use.ref)

; Use — simple field type: private Route route; private Train train;
(field_declaration type: (type_identifier) @Use.ref)

; Use/Parameter — generic type argument in field declaration: ArrayList<Train> trains → Train
; Source resolves to the Field entity (when Field exclusion is lifted in find_references)
(field_declaration type: (generic_type (type_arguments (type_identifier) @Use.ref)))
(field_declaration type: (generic_type (type_arguments (type_identifier) @Parameter.ref)))

; Use — `this` may resolve to the enclosing class declaration.
; Ownership-only Constructor/Method → ownClass Use edges are filtered later.
(this) @Use.ref

; Use — constructor names may resolve to the class declaration; ownership-only
; Constructor → ownClass Use edges are filtered later.
(constructor_declaration name: (identifier) @Use.ref)

; Use — field write: field = value (bare name) or this.field = value.
; Writes are generalized as Use so the output does not split read/write edges.
(assignment_expression left: (identifier) @Use.ref)
(assignment_expression left: (field_access field: (identifier) @Use.ref))

; Cast — type cast expression: (SomeType) expr
(cast_expression type: (type_identifier) @Cast.ref)

; Parameter — simple formal parameter type: void method(Ticket t)
; Also Use: the reference emits both Parameter and Use for parameter types to project classes
(formal_parameter type: (type_identifier) @Parameter.ref)
(formal_parameter type: (type_identifier) @Use.ref)

; Parameter — generic formal parameter base type: void method(ArrayList<Ticket> t) → ArrayList
(formal_parameter type: (generic_type (type_identifier) @Parameter.ref))
(formal_parameter type: (generic_type (type_identifier) @Use.ref))

; Parameter — generic formal parameter type argument: void method(ArrayList<Ticket> t) → Ticket
(formal_parameter type: (generic_type (type_arguments (type_identifier) @Parameter.ref)))
(formal_parameter type: (generic_type (type_arguments (type_identifier) @Use.ref)))

; Return — simple return type
(method_declaration type: (type_identifier) @Return.ref)

; Return — generic return type: ArrayList<Route> getRoutes() → ArrayList
(method_declaration type: (generic_type (type_identifier) @Return.ref))

; Annotation — marker and normal
(marker_annotation name: (identifier) @Annotation.ref)
(annotation        name: (identifier) @Annotation.ref)

; Throw — runtime throw: throw new FooException()
(throw_statement (object_creation_expression type: (type_identifier) @Throw.ref))

; Throw — method signature declares throws: void method() throws IOException
(throws (type_identifier) @Throw.ref)

; Call — super(...) constructor invocation (calls parent class constructor)
(explicit_constructor_invocation constructor: (super) @Call.ref)

; Use — local variable initializer: Train t = someField;
(variable_declarator value: (identifier) @Use.ref)

; Use — local variable generic type arg: ArrayList<Train> list = ...
(local_variable_declaration type: (generic_type (type_arguments (type_identifier) @Use.ref)))

; Use — enhanced-for variable generic type arg: for (ArrayList<Train> group : groups)
(enhanced_for_statement type: (generic_type (type_arguments (type_identifier) @Use.ref)))

; Use — field/object as field access receiver: obj.field (obj is a field or class name)
; Complements existing field_access field: capture — covers the object side of a.b
(field_access object: (identifier) @Use.ref)

; Use — instanceof check: if (obj instanceof Train)
(instanceof_expression right: (type_identifier) @Use.ref)

; Use — array access base: arr[i]
(array_access array: (identifier) @Use.ref)

; Use — array creation type: new Train[n]
(array_creation_expression type: (type_identifier) @Use.ref)

; Use — generic object creation type arg: new ArrayList<Train>() → Train
(object_creation_expression type: (generic_type (type_arguments (type_identifier) @Use.ref)))

; Use — ternary expression operands: cond ? a : b
(ternary_expression condition:   (identifier) @Use.ref)
(ternary_expression consequence: (identifier) @Use.ref)
(ternary_expression alternative: (identifier) @Use.ref)

; Use — catch clause type: catch (IOException e)
(catch_formal_parameter (catch_type (type_identifier) @Use.ref))

; Use — parenthesized expression: (field) or (obj)
(parenthesized_expression (identifier) @Use.ref)

; Use — unary expression operand: !flag, -value, ~mask
(unary_expression operand: (identifier) @Use.ref)

; Use — switch condition: switch (field)
(switch_expression condition: (parenthesized_expression (identifier) @Use.ref))

; Use — method reference class: MyClass::method
(method_reference (type_identifier) @Use.ref)

; Parameter — array formal parameter: Train[] trains
(formal_parameter type: (array_type element: (type_identifier) @Parameter.ref))

; Return — array return type: Train[] getTrains()
(method_declaration type: (array_type element: (type_identifier) @Return.ref))

; Return + Parameter — generic return type argument: ArrayList<Route> findRoutes() → Route
; Reference counts these as Parameter; emit both for maximum coverage
(method_declaration type: (generic_type (type_arguments (type_identifier) @Return.ref)))
(method_declaration type: (generic_type (type_arguments (type_identifier) @Parameter.ref)))

; Throw — re-throw or throw local exception variable: throw e;
(throw_statement (identifier) @Throw.ref)
"#;
