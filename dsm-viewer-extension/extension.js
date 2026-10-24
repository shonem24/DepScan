const vscode = require("vscode");
const path = require("path");

/**
 * @param {vscode.ExtensionContext} context
 */
function activate(context) {
  const provider = new DsmEditorProvider(context);

  context.subscriptions.push(
    vscode.window.registerCustomEditorProvider("dsmViewer.matrix", provider, {
      webviewOptions: { retainContextWhenHidden: true },
      supportsMultipleEditorsPerDocument: false,
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("dsmViewer.open", async (uri) => {
      const target = uri instanceof vscode.Uri ? uri : await pickJsonFile();
      if (!target) return;
      await vscode.commands.executeCommand("vscode.openWith", target, "dsmViewer.matrix");
    })
  );

  context.subscriptions.push(
    vscode.commands.registerCommand("dsmViewer.openActive", async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor) {
        vscode.window.showWarningMessage("Open a dependency JSON file first.");
        return;
      }
      await vscode.commands.executeCommand(
        "vscode.openWith",
        editor.document.uri,
        "dsmViewer.matrix"
      );
    })
  );
}

async function pickJsonFile() {
  const picked = await vscode.window.showOpenDialog({
    canSelectMany: false,
    filters: { JSON: ["json"] },
    openLabel: "Open as DSM",
  });
  return picked?.[0];
}

class DsmDocument {
  /**
   * @param {vscode.Uri} uri
   * @param {string} text
   */
  constructor(uri, text) {
    this.uri = uri;
    this.text = text;
  }

  dispose() {}
}

class DsmEditorProvider {
  /** @param {vscode.ExtensionContext} context */
  constructor(context) {
    this.context = context;
  }

  /**
   * @param {vscode.Uri} uri
   * @param {vscode.CancellationToken} _token
   */
  async openCustomDocument(uri, _token) {
    const bytes = await vscode.workspace.fs.readFile(uri);
    return new DsmDocument(uri, Buffer.from(bytes).toString("utf8"));
  }

  /**
   * @param {DsmDocument} document
   * @param {vscode.WebviewPanel} webviewPanel
   * @param {vscode.CancellationToken} _token
   */
  async resolveCustomEditor(document, webviewPanel, _token) {
    webviewPanel.webview.options = {
      enableScripts: true,
      localResourceRoots: [vscode.Uri.joinPath(this.context.extensionUri, "media")],
    };
    webviewPanel.webview.html = getWebviewHtml(
      webviewPanel.webview,
      this.context.extensionUri
    );

    const postLoad = () => {
      let data;
      try {
        data = JSON.parse(document.text);
      } catch (err) {
        webviewPanel.webview.postMessage({
          type: "error",
          message: `Invalid JSON: ${err.message}`,
        });
        return;
      }
      webviewPanel.webview.postMessage({
        type: "load",
        data,
        label: path.basename(document.uri.fsPath),
      });
    };

    const sub = webviewPanel.webview.onDidReceiveMessage((msg) => {
      if (msg?.type === "ready" || msg?.type === "reload") postLoad();
    });

    const watcher = vscode.workspace.createFileSystemWatcher(document.uri.fsPath);
    const reloadFromDisk = async () => {
      try {
        const bytes = await vscode.workspace.fs.readFile(document.uri);
        document.text = Buffer.from(bytes).toString("utf8");
        postLoad();
      } catch (_) {
        /* ignore */
      }
    };
    watcher.onDidChange(reloadFromDisk);

    webviewPanel.onDidDispose(() => {
      sub.dispose();
      watcher.dispose();
    });
  }
}

/**
 * @param {vscode.Webview} webview
 * @param {vscode.Uri} extensionUri
 */
function getWebviewHtml(webview, extensionUri) {
  const styleUri = webview.asWebviewUri(
    vscode.Uri.joinPath(extensionUri, "media", "styles.css")
  );
  const scriptUri = webview.asWebviewUri(
    vscode.Uri.joinPath(extensionUri, "media", "js", "main.js")
  );
  const csp = [
    "default-src 'none'",
    `style-src ${webview.cspSource}`,
    `script-src ${webview.cspSource}`,
  ].join("; ");

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta http-equiv="Content-Security-Policy" content="${csp}" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Dependency Structure Matrix</title>
  <link rel="stylesheet" href="${styleUri}" />
</head>
<body>
  <header class="topbar">
    <div class="brand">
      <span class="logo">DSM</span>
      <span id="status" class="status">Loading…</span>
    </div>
    <div class="controls">
      <button id="reload" class="btn" type="button" title="Reload from disk">Reload</button>
      <div class="seg" role="group" aria-label="Sort">
        <button id="sortGiven" type="button" class="active" title="Extraction / project order">As given</button>
        <button id="sortName" type="button" title="Alphabetical">By name</button>
        <button id="sortLowerTri" type="button" title="Reorder so dependencies sit in the lower triangle">Lower tri</button>
      </div>
      <div class="seg" role="group" aria-label="Level">
        <button id="levelFolders" type="button">Folders</button>
        <button id="levelFiles" type="button" class="active">Files</button>
        <button id="levelSymbols" type="button">Symbols</button>
      </div>
      <div class="seg zoom" role="group" aria-label="Zoom">
        <button id="zoomOut" type="button" title="Zoom out">−</button>
        <button id="zoomReset" type="button" title="Reset zoom" class="zoom-pct">100%</button>
        <button id="zoomIn" type="button" title="Zoom in">+</button>
      </div>
      <input id="search" type="search" placeholder="Filter…" />
    </div>
  </header>
  <section class="kinds-bar">
    <div class="seg" role="group" aria-label="Kind mode">
      <button id="kindModeToggle" type="button" class="active">Toggle</button>
      <button id="kindModeOnly" type="button">Only</button>
    </div>
    <button id="kindReset" class="btn" type="button">Reset</button>
    <div id="kindFilters" class="kind-list" aria-label="Dependency kinds"></div>
    <span id="stats" class="stats"></span>
  </section>
  <main class="workspace">
    <aside class="tree-panel">
      <div id="tree" class="tree"></div>
    </aside>
    <section class="matrix-panel">
      <div class="matrix-shell" id="matrixShell">
        <div class="corner"></div>
        <div id="colHeader" class="col-header"></div>
        <div id="rowHeader" class="row-header"></div>
        <div id="viewport" class="viewport">
          <canvas id="matrix"></canvas>
        </div>
        <div id="labelResize" class="label-resize" title="Drag to resize"></div>
      </div>
    </section>
  </main>
  <div id="hoverCard" class="hover-card" hidden></div>
  <script type="module" src="${scriptUri}"></script>
</body>
</html>`;
}

function deactivate() {}

module.exports = { activate, deactivate };
