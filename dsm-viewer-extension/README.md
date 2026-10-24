# DSM Viewer

VS Code extension that shows a dependency matrix for `dep-result.json` / `deps.json`.

It only **views** a JSON file. Create that file first with the analyzer (`../vscode-extension`) or `cargo run --release -- …` in the project root.

You need: [VS Code](https://code.visualstudio.com/) and [Node.js](https://nodejs.org/) (to build the install package).

---

## 1. Build the install package

A `.vsix` is the installable extension file. It is **not** checked in for you to rely on — on a first setup you create it:

```bash
cd LSP_Dependency_Extraction_Tool/dsm-viewer-extension
npx --yes @vscode/vsce package --no-dependencies
```

That creates `dsm-viewer-0.1.0.vsix` in this folder.

## 2. Install it into VS Code

1. `Ctrl+Shift+P` (Mac: `Cmd+Shift+P`)
2. **Extensions: Install from VSIX...**
3. Select `dsm-viewer-0.1.0.vsix` in this folder
4. Command Palette → **Developer: Reload Window**

## 3. Get a dependency JSON

Pick one:

**A — Terminal** (from `LSP_Dependency_Extraction_Tool/`)

```bash
npm run setup
cargo run --release -- /path/to/your/code --language python
```

Creates `deps.json`.

**B — Analyzer extension** (`../vscode-extension`)

Right-click a folder → **Analyze Dependencies**. Creates `dep-result.json`.

## 4. Open the matrix

1. Open that JSON file in VS Code  
2. Command Palette → **DSM: Open Matrix Viewer**

---

**Controls:** Folders / Files / Symbols (depth) · As given / By name / Lower tri (sort) · kind chips (filter) · Zoom / Filter / Reload

**Dev without installing:** `code --extensionDevelopmentPath=.` (loads this folder; no `.vsix` needed)
