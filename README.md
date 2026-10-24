# LSP Dependency Extraction Tool

Extracts structural dependencies from a codebase using LSP + tree-sitter. Supports **python**, **java**, **rust**, and **typescript**.

## 1. Run scans from the command line

All commands work the same on Windows, macOS and Linux. Run them from this folder.

### Install first

- Rust: [https://rustup.rs](https://rustup.rs)
- Node.js: [https://nodejs.org](https://nodejs.org) (needed for the Python and TypeScript language servers)

### Set up

```bash
npm run setup
```

This installs the language servers into this repo's `.tools/` folder, builds the tool, and then runs `npm run verify`. It installs `pyright` (Python) and `typescript-language-server` (TypeScript) with npm, and `rust-analyzer` (Rust) with rustup. The tool finds servers in `.tools/` on its own, so you don't need to change PATH.

The VS Code extension doesn't use `.tools/`. When you analyze a folder, it checks for a server and offers to install one if it can't find it.

Java needs one extra step; see [Scanning Java](#scanning-java) below.

To check everything is installed:

```bash
npm run verify
```

It lists each requirement as OK or MISSING, with the command that fixes it. Python and TypeScript servers are required; Rust and Java are optional.

### Scanning Java

Java uses the Eclipse language server, `jdtls`. It isn't installed by `npm run setup` because it needs Java and Python, and is a 50 MB download. Install it once:

**1. Check the requirements.** Both commands must work:

```bash
java -version      # must show version 21 or newer
python3 --version  # Windows: python --version
```

If either is missing, install [Java 21+](https://adoptium.net) or [Python 3](https://www.python.org/downloads/). On Windows, tick **"Add python.exe to PATH"** in the Python installer.

**2. Install jdtls.**

*macOS (Homebrew):*

```bash
brew install jdtls
```

*macOS / Linux (without Homebrew)*, from this folder:

```bash
mkdir -p .tools/jdtls
curl -L https://download.eclipse.org/jdtls/milestones/1.60.0/jdt-language-server-1.60.0-202606262232.tar.gz | tar -xz -C .tools/jdtls
```

*Windows (PowerShell)*, from this folder:

```powershell
mkdir .tools\jdtls
curl.exe -L -o jdtls.tar.gz https://download.eclipse.org/jdtls/milestones/1.60.0/jdt-language-server-1.60.0-202606262232.tar.gz
tar -xzf jdtls.tar.gz -C .tools\jdtls
del jdtls.tar.gz
# The bundled launcher ends with `pause`, which makes scans hang. Remove that line:
(Get-Content .tools\jdtls\bin\jdtls.bat) -notmatch '^pause' | Set-Content .tools\jdtls\bin\jdtls.bat
```

The tool finds `jdtls` in `.tools/jdtls` automatically, so you don't need to change PATH.

**3. Check it.** `npm run verify` should now show `OK jdtls (java)`.

Use version 1.60.0 as shown: newer jdtls builds need Java 25.

### Scan a project

```bash
cargo run --release -- /path/to/repo --language python --output deps.json
```

- The project folder is required.
- `--language`: `python` (default), `java`, `rust` or `typescript`.
- `--output`: result file (default `deps.json`).
- `--lsp-override "<command>"`: use a different language server, e.g. `--lsp-override pylsp`.

> [!WARNING]
> **Choose the right project root.** Point the tool at the folder your code's imports and build start from:
>
> | Language | Scan the folder that contains |
> |---|---|
> | Python | your top-level package (e.g. for `from maze.Room import Room`, the folder that *contains* `maze/`) |
> | Rust | `Cargo.toml` |
> | Java | `pom.xml` or `build.gradle` |
> | TypeScript | `package.json` or `tsconfig.json` |
>
> The language server resolves references relative to this root. With the wrong root, cross-file references such as imports, constructor calls and inheritance can't be resolved, so they won't appear in the results.
>
> **To check:** if expected `Import` edges are missing from `deps.json`, or (for Python) `deps.unmapped.json` lists many references, try scanning one folder up.



### Results

- `deps.json`: `variables` (entities) and `cells` (dependency edges, `src` -> `dest`).
- `deps.unmapped.json`: Python references that could not be matched to a project member, with a reason for each.

Python also infers dependencies for unresolved attribute accesses. Each such edge carries an `inference` list with `kind`, `resolution` (`explicit` or `possible`) and `candidate_count`. This is heuristic static analysis, so a single candidate is not proof of a runtime target.

### Other commands

| Command | What it does |
|---|---|
| `cargo clean` | Removes build output |
| Delete the `.tools/` folder | Removes the language servers installed by `npm run setup` (and jdtls) |

The setup commands are npm scripts in `package.json`.

## 2. Use the VS Code extension

The extension adds **Analyze Dependencies** to the right-click menu on a folder. It writes `dep-result.json` and `dep-result.unmapped.json` into the folder you analyze.

### Build and install

Do section 1 first (`npm run setup`), then, from the project folder, run the commands below. `npm run extension` builds the tool and copies it into the extension; `npx vsce package` creates `vscode-extension/lsp-dep-extractor-0.1.0.vsix`.

```bash
npm run extension
cd vscode-extension
npm install
npx vsce package
```

Then, in VS Code:

1. Press `Cmd+Shift+P` (`Ctrl+Shift+P` on Windows and Linux).
2. Run **Extensions: Install from VSIX...**
3. Select `lsp-dep-extractor-0.1.0.vsix` from the `vscode-extension` folder.
4. Run **Developer: Reload Window**.

Notes:

- `vsce` may ask about a missing repository or license. Answer `y`; the warnings are harmless.
- The bundled binary only works on the OS and CPU it was built on, so build the `.vsix` on the machine where it will be used.



### Use

Right-click a folder in the Explorer, choose **Analyze Dependencies**, and pick a language if asked. The extension detects or offers to install a language server.

### Settings

- `lspDepExtractor.preferredLsp`: preferred server per language, e.g. `{"python": "pylsp"}`.
- `lspDepExtractor.customLspCommands`: full launch command per language.

## 3. View the results

The repo includes a **DSM Viewer** VS Code extension that shows the output (`deps.json` or `dep-result.json`) as an interactive dependency matrix. See [dsm-viewer-extension/README.md](dsm-viewer-extension/README.md) to install and use it.
