# Matrix Viewer (browser)

Browser UI for a dependency matrix (`dep-result.json` / `deps.json`).

## Run

From the **project root** (`LSP_Dependency_Extraction_Tool/`):

```bash
npx --yes serve -l 8765
```

Open: http://localhost:8765/matrix-viewer/

It loads `../dep-result.json` if present, or use **Open** to pick a JSON file.

## VS Code extension

For install and setup in VS Code, see **[../dsm-viewer-extension/README.md](../dsm-viewer-extension/README.md)**.
