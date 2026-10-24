import * as path from "path";
import * as vscode from "vscode";
import { detectLanguages } from "./languageDetector";
import { resolveServerCommand } from "./lspDetector";
import { LSP_REGISTRY } from "./lspRegistry";
import { runExtractor } from "./runner";

function getBinaryPath(context: vscode.ExtensionContext): string {
  const platform =
    process.platform === "win32"
      ? "win.exe"
      : process.platform === "darwin"
      ? "darwin"
      : "linux";
  return context.asAbsolutePath(
    path.join("bin", `lsp-dep-extractor-${platform}`)
  );
}

export function activate(context: vscode.ExtensionContext): void {
  const cmd = vscode.commands.registerCommand(
    "lspDepExtractor.analyze",
    async (uri?: vscode.Uri) => {
      // Resolve repo path from right-click URI or first workspace folder
      const repoPath =
        uri?.fsPath ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;

      if (!repoPath) {
        vscode.window.showErrorMessage(
          "No folder selected. Right-click a folder in the Explorer or open a workspace."
        );
        return;
      }

      // Detect language(s) from file counts
      const detected = detectLanguages(repoPath);
      let language: string | undefined;

      if (detected.length === 0) {
        // No known source files — let the user pick from all supported languages
        language = await vscode.window.showQuickPick(Object.keys(LSP_REGISTRY), {
          title: "Select language to analyze",
          placeHolder: "Could not auto-detect language from file extensions",
        });
      } else if (detected.length === 1) {
        language = detected[0].language;
      } else {
        // Multiple languages found — ask which one to analyze
        const items = detected.map(({ language: lang, count }) => ({
          label: lang,
          description: `${count} source file${count === 1 ? "" : "s"}`,
        }));
        const pick = await vscode.window.showQuickPick(items, {
          title: "Multiple languages detected — which would you like to analyze?",
          placeHolder: "Select the primary language for dependency analysis",
        });
        language = pick?.label;
      }

      if (!language) return;

      // Resolve which LSP server to use (auto-detect, prompt, or install)
      const serverCommand = await resolveServerCommand(language, context);
      if (!serverCommand) return;

      const outputPath = path.join(repoPath, "dep-result.json");

      try {
        await runExtractor(
          getBinaryPath(context),
          repoPath,
          language,
          serverCommand,
          outputPath
        );

        const doc = await vscode.workspace.openTextDocument(outputPath);
        await vscode.window.showTextDocument(doc);

        vscode.window.showInformationMessage(
          `Dependency analysis complete → dep-result.json`
        );
      } catch (err) {
        vscode.window.showErrorMessage(
          `Analysis failed: ${err instanceof Error ? err.message : String(err)}`
        );
      }
    }
  );

  context.subscriptions.push(cmd);
}

export function deactivate(): void {}
