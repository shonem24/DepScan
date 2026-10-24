# Adding a language

These steps add support for **any** language. Go is used as the worked example throughout. Replace every Go value with the values for your language.

All paths are relative to `LSP_Dependency_Extraction_Tool/`.

**Before you start**, make sure the existing tool works:

```bash
npm run setup     # installs the language servers into .tools/ and builds the tool
npm run verify    # everything should be OK
```

## Placeholders

Decide these first. Every step below uses them.

| Placeholder | Meaning | Go example |
|---|---|---|
| `<lang>` | Lowercase ID, used for `--language` and the file name | `go` |
| `<Lang>` | Type name for the Rust struct | `Go` |
| `<ext>` | File extensions to read | `.go` |
| `<server>` | Command that starts the language server over stdio | `gopls` |
| `<grammar-crate>` | tree-sitter grammar crate (must work with `tree-sitter = "0.25"`) | `tree-sitter-go` |
| `<grammar-const>` | The crate's language constant | `tree_sitter_go::LANGUAGE` |

---

## Step 1: Add the grammar

**File:** `Cargo.toml`, in `[dependencies]` next to the other `tree-sitter-*` crates.

```toml
tree-sitter-go = "0.23"          # <grammar-crate> = "<version>"
```

**Check:** `cargo build` succeeds.

## Step 2: Create the language file

Each supported language is one file in `src/language/` that implements the `Language` trait (defined in `src/language/mod.rs`). The rest of the tool never mentions a specific language. Instead, `pipeline.rs` calls these methods to get the language-specific settings it needs at each stage:

| Method | What it tells the tool | Used when |
|---|---|---|
| `id` | The language's name; must match `--language` and the Step 3 registry key | Telling the server each file's language |
| `extensions` | Which files belong to this language | Finding the project's source files |
| `lsp_command` | How to start the language server (program, then each argument) | Starting the server |
| `init_options` | Extra settings the server wants at startup (`{}` if none) | The LSP `initialize` request |
| `symbol_kind_map` | Which LSP symbols count as classes, methods, functions… (Step 4) | Building the list of entities |
| `ts_language` | The tree-sitter grammar from Step 1 | Parsing the code |
| `ref_queries` | Patterns that mark references and their dependency type (Step 5) | Finding the references |

Start with the template below. The first four methods are simple values you can fill in now. `symbol_kind_map` and `ref_queries` hold placeholders until Steps 4 and 5. The existing language files (`python.rs`, `java.rs`, `rust.rs`, `typescript.rs`) follow the same shape if you want more examples.

**File:** create `src/language/<lang>.rs` (e.g. `src/language/go.rs`).

```rust
use std::collections::HashMap;
use std::path::Path;
use serde_json::{json, Value};

use crate::entity::EntityKind;
use crate::language::Language;

pub struct Go;                                            // <Lang>

impl Language for Go {
    fn id(&self) -> &str { "go" }                         // <lang>
    fn extensions(&self) -> &[&str] { &[".go"] }          // <ext>
    fn lsp_command(&self) -> Vec<String> { vec!["gopls".into()] }   // <server>, one string per argument
    fn init_options(&self, _root: &Path) -> Value { json!({}) }    // server's initializationOptions

    fn symbol_kind_map(&self) -> HashMap<u32, EntityKind> {
        HashMap::from([                                   // see Step 4
            (5,  EntityKind::Class),
            (6,  EntityKind::Method),
            (8,  EntityKind::Field),
            (11, EntityKind::Interface),
            (12, EntityKind::Function),
            (23, EntityKind::Class),
        ])
    }

    fn ts_language(&self) -> tree_sitter::Language {
        tree_sitter_go::LANGUAGE.into()                   // <grammar-const>
    }

    fn ref_queries(&self) -> &str {
        r#"
        (call_expression function: (identifier) @Call.ref)
        (selector_expression field: (field_identifier) @Use.ref)
        "#                                                // see Step 5
    }
}
```

**Check:** nothing yet. It compiles after Step 3.

## Step 3: Register the language

**File:** `src/language/mod.rs`. Add the module next to `pub mod python;`, and the entry inside `registry()`.

```rust
pub mod go;                                   // pub mod <lang>;

// inside registry():
map.insert("go", Box::new(go::Go));           // map.insert("<lang>", Box::new(<lang>::<Lang>));
```

The registry key must equal `id()`.

**Check:** `cargo build` succeeds, and `cargo run -- --help` still works.

## Step 4: Map symbols to entities

**File:** `src/language/<lang>.rs`, `symbol_kind_map`.

The LSP labels each symbol with a number. Map the numbers your server sends for project entities:

| LSP number | Meaning | Entity kind |
|---|---|---|
| 5 | Class | `Class` |
| 6 | Method | `Method` |
| 7, 8 | Property, Field | `Field` |
| 9 | Constructor | `Constructor` |
| 10 | Enum | `Enum` |
| 11 | Interface | `Interface` |
| 12 | Function | `Function` |
| 23 | Struct | `Class` |

Allowed entity kinds: `Class`, `Interface`, `Enum`, `Constructor`, `Method`, `Function`, `Field`.

Numbers you leave out are skipped, but their children are still read. Fields only count inside a class.

**Example.** Go has no classes; `gopls` reports a `struct` as 23 and a struct field as 8. To show Go structs as classes with their fields, edit the `HashMap::from([...])` list in `symbol_kind_map`. Each line is `(LSP number, entity kind)`:

```rust
fn symbol_kind_map(&self) -> HashMap<u32, EntityKind> {
    HashMap::from([
        (6,  EntityKind::Method),
        (8,  EntityKind::Field),      // struct fields
        (11, EntityKind::Interface),
        (12, EntityKind::Function),
        (23, EntityKind::Class),      // ← add: Go structs become Class entities
    ])
}
```

To support another symbol type, add one line to this list. To ignore one (e.g. local variables, number 13), leave its number out.

**Check:** after Step 7, the output's `variables` list contains the classes and functions you expect.

## Step 5: Write reference patterns

**File:** `src/language/<lang>.rs`, `ref_queries`.

Each tree-sitter pattern marks a reference. The capture name is the dependency type:

```scheme
(call_expression function: (identifier) @Call.ref)
```

Allowed types: `Import`, `Extend`, `Implement`, `Call`, `Create`, `Override`, `Use`, `Parameter`, `Return`, `Annotation`, `Throw`, `Cast`.

Rules:
- **Never guess node names.** Get them from the grammar: run `tree-sitter parse sample<ext>`, or read the grammar's `node-types.json`.
- Capture the **name** the LSP can resolve (an identifier), not a whole expression.
- Capture names are case-sensitive (`@Call.ref`, not `@call.ref`). A wrong name is ignored without any error.
- Don't write patterns for `Create` or `Override`. The tool derives them.
- Start with `Import`, `Call` and `Extend`/`Implement`, then add `Use`, `Parameter` and `Return`.

**Check:** after Step 7, the output's `cells` contain the edges you expect.

## Step 6: Install the language server

Hook `<server>` into the setup scripts, so `npm run setup` installs it and `npm run verify` checks it.

| File | Change |
|---|---|
| `package.json` | If `<server>` installs with npm, add its package to the `npm install` list in the `setup` script. Otherwise, document the manual install in the README (see "Scanning Java" for an example). |
| `scripts/verify.js` | Add a line under "Language servers", e.g. `check("gopls (go)", find("gopls"), "see README", false);` |

Then run:

```bash
npm run setup     # installs <server> (if npm-based) and rebuilds the tool with your new language
npm run verify    # <server> should show as OK
```

**Check:** `npm run verify` shows `OK <server> (<lang>)`. Fix this before debugging anything else; the tool finds servers in the same places verify does.

## Step 7: Run and verify

Make a small sample project (2–3 files) with an import, a call across files, and a type that uses another type. Write down the edges you expect. Scan the sample's project root (the folder its imports and build start from); with the wrong root, cross-file references won't resolve. Then run:

```bash
cargo fmt --check
cargo test
cargo run --release -- /path/to/sample --language <lang> --output /tmp/deps-<lang>.json
```

**Check:**
- The command exits successfully.
- `variables` lists the expected entities.
- `cells` contain the expected edges (imports point to files; calls to classes become `Create`).

## Step 8: Wire up the extras

| File | Change |
|---|---|
| `README.md` | Install steps for `<server>`, a run command, and which folder counts as the project root |
| `vscode-extension/src/languageDetector.ts` | Add each `<ext>` → `<lang>` to `EXT_TO_LANGUAGE` |
| `vscode-extension/src/lspRegistry.ts` | Add a `<lang>` entry to `LSP_REGISTRY` |

The VS Code extension needs **both** entries. Run `npm run extension` afterwards to bundle the new binary.

---

## Done when

- [ ] `cargo fmt --check`, `cargo test` and `cargo build --release` pass
- [ ] The sample run produces the expected entities and edges
- [ ] `npm run verify` lists `<server>` as OK
- [ ] `package.json` (if needed), `scripts/verify.js`, README and both VS Code files are updated
- [ ] Unsupported syntax is listed in the README, not silently left out

---

## If something's wrong

| Symptom | Check |
|---|---|
| No files found | `extensions()`, project path, `.gitignore` |
| Server exits or hangs | `npm run verify`, `lsp_command()`, `init_options()` |
| Few cross-file edges, imports missing | The scanned folder isn't the project root; scan one folder up |
| Missing entities | `symbol_kind_map()` numbers vs. what the server sends |
| Error when queries run | Node names in `ref_queries()` |
| Entities but no edges | Capture names, and that the captured node is a name |
| Works in CLI, not VS Code | Both extension files, `npm run extension` |

---

## Advanced (optional)

### Inferred dependencies

For dynamic languages, the LSP often can't resolve `x.y` because it doesn't know the type of `x`. The tool can infer these. Inferred edges are marked `"possible"` in the output. Two rules are built in:

- **Receiver type inference:** `s.draw(); s.resize(2)` → the class that has both members.
- **Return type inference:** `c = Circle()` … `return c` → returns `Circle`.

**File:** `src/language/<lang>.rs`. Add an `InferenceSpec` and return it. Every `"..."` is a node or field name from your grammar:

```rust
use crate::possible_dependencies::InferenceSpec;

static INFERENCE: InferenceSpec = InferenceSpec {
    member_access: "...",            // node for x.y
    member_object_field: "...",      //   field holding x
    member_name_field: "...",        //   field holding y
    identifier: "...",               // a plain name
    call: "...", call_function_field: "...", call_arguments_field: "...",
    uncountable_arguments: &[],      // argument kinds that make counting unreliable (e.g. *args)
    scopes: &[],                     // nodes that open a new scope
    rebindings: &[], rebinding_target_fields: &[],   // nodes that reassign a variable
    boundaries: &[],                 // nested definitions to skip
    block: "...", function: "...", class: "...",
    assignment: "...", assignment_left_field: "...", assignment_right_field: "...",
    return_statement: "...",
    decorated_wrappers: &[],         // wrappers that can change a signature
    function_name_field: "...", parameters_field: "...",
    plain_parameters: &[], optional_parameters: &[],
    implicit_parameters: 0,          // e.g. 1 if methods take `self`
    is_private: |_name| false,       // which member names are private
};

// inside impl Language for <Lang>:
fn inference_spec(&self) -> Option<&'static InferenceSpec> { Some(&INFERENCE) }
```

**Check:** the output has cells with `"resolution": "possible"`, and `<output>.unmapped.json` is written next to it.

### Adding a new inference rule

A rule is a Rust function in `src/possible_dependencies.rs` that spots a code shape and emits possible edges. Rules use the language's `InferenceSpec` for node names, so one rule works for every language that has a spec.

The worked example adds **constructor default arguments**:

```python
def draw(shape=Circle()):   # → draw depends on Circle (Parameter)
```

#### R1. Define the rule

Write down three things before coding:

| Question | Example answer |
|---|---|
| What code shape triggers it? | A parameter whose default value is a call to a project class |
| What edge does it produce? | enclosing function → class, kind `Parameter` |
| Does it only cover references in U (unresolved)? | No: it scans every function, like the return type rule |

Rules that **only** cover unresolved references must check positions against U and record unmapped reasons. See R5.

#### R2. Add any new syntax names to `InferenceSpec`

**File:** `src/possible_dependencies.rs`, `pub struct InferenceSpec`. Only needed if the existing fields don't describe the shape. The example needs the field that holds a parameter's default value:

```rust
/// Field holding a parameter's default value.
pub default_value_field: &'static str,
```

Then fill it in for **every** language that has a spec, or the code won't compile.

**File:** `src/language/python.rs`, `static INFERENCE`:

```rust
default_value_field: "value",
```

#### R3. Write the rule function

**File:** `src/possible_dependencies.rs`. Model it on `infer_returned_constructor_deps`: parse each file, walk the tree, emit edges. Reuse the existing helpers:

| Helper | Gives you |
|---|---|
| `build_class_index(entities)` | class name → entity indices |
| `called_name(spec, node, text)` | the name being called in `X()` or `a.X()` |
| `entity_index.position_to_entity(uri, line, entities, &[])` | the entity that encloses a line |
| `node_text(node, text)` | a node's source text |
| `emitted` (passed in) | prevents duplicate edges |

```rust
fn infer_default_argument_deps(
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
        visit_default_arguments(
            spec, tree.root_node(), text, &file_uri,
            entities, entity_index, &class_index, deps, emitted,
        );
    }
    Ok(())
}

fn visit_default_arguments(
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
        let src = entity_index.position_to_entity(
            file_uri, node.start_position().row as u32, entities, &[],
        );
        if let (Some(src), Some(params)) = (src, node.child_by_field_name(spec.parameters_field)) {
            let mut cursor = params.walk();
            let params: Vec<_> = params.named_children(&mut cursor).collect();
            for param in params {
                if !spec.optional_parameters.contains(&param.kind()) {
                    continue;
                }
                let Some(value) = param.child_by_field_name(spec.default_value_field) else {
                    continue;
                };
                let Some(class_name) = called_name(spec, value, text) else {
                    continue;
                };
                let Some(targets) = class_index.get(&class_name) else {
                    continue;
                };
                let candidate_count = targets.len();
                for &tgt in targets {
                    if emitted.insert((src, tgt, DepKind::Parameter, candidate_count)) {
                        deps.push(PossibleDep { src, tgt, kind: DepKind::Parameter, candidate_count });
                    }
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit_default_arguments(
            spec, child, text, file_uri, entities, entity_index, class_index, deps, emitted,
        );
    }
}
```

Rules for every new rule:
- Set `candidate_count` to the number of targets that survive for that reference.
- Always go through `emitted` before pushing.
- Never emit an edge from an entity to itself.
- Only target project entities. The LSP has already shut down, so you can't ask it anything.

#### R4. Call the rule

**File:** `src/possible_dependencies.rs`, inside `infer_possible_deps`. Add the call straight after `infer_returned_constructor_deps(...)?;` and **before** the unmapped list is cleaned and written:

```rust
infer_default_argument_deps(
    spec, file_texts, ts_lang, entities, entity_index, &mut deps, &mut emitted,
)?;
```

#### R5. Rules that cover unresolved references only

If the rule handles references the LSP couldn't resolve:
- Only act on positions that are in `unresolved` (U).
- When the rule succeeds, add the position to `mapped_positions`, so it's removed from the unmapped report.
- When it fails, push an `UnmappedReference` with a new, descriptive `reason` string.

#### R6. Test it

Add a `#[cfg(test)]` module at the bottom of `src/possible_dependencies.rs`. Build `entities` by hand, give it a short source string, call `infer_possible_deps` directly (no language server needed), and assert on the returned edges. Then run the tool on a sample project:

**Check:**
- `cargo test` passes.
- A sample with `def draw(shape=Circle())` produces a cell from `draw` to `Circle` with `"kind": "Parameter"` and `"resolution": "possible"`.
- Output for code without the shape is unchanged.

### Other hooks

Optional methods on `Language`. The defaults do nothing.

| Method | Use it to |
|---|---|
| `extra_deps` | Add structural edges after extraction |
| `post_process_deps` | Filter or rewrite the final edges |
| `infer_possible_dependencies` | Replace the inference engine entirely |
