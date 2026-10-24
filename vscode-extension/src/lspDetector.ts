import * as cp from "child_process";
import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import { LSP_REGISTRY, LspOption } from "./lspRegistry";

function onPath(binary: string): Promise<boolean> {
  return new Promise((resolve) => {
    const cmd =
      process.platform === "win32" ? `where ${binary}` : `which ${binary}`;
    cp.exec(cmd, (err) => resolve(!err));
  });
}

// npm's global folder, where `npm run setup` installs servers. It is often not
// on PATH when npm uses a custom prefix; the bundled tool checks it too.
function inNpmGlobal(binary: string): Promise<boolean> {
  return new Promise((resolve) => {
    cp.exec("npm prefix -g", (err, stdout) => {
      if (err) return resolve(false);
      const prefix = stdout.trim();
      const bin = process.platform === "win32" ? prefix : path.join(prefix, "bin");
      const names = process.platform === "win32" ? [`${binary}.cmd`, `${binary}.exe`] : [binary];
      resolve(names.some((name) => fs.existsSync(path.join(bin, name))));
    });
  });
}

async function isInstalled(binary: string): Promise<boolean> {
  return (await onPath(binary)) || (await inNpmGlobal(binary));
}

async function pollUntilInstalled(
  binary: string,
  timeoutMs = 60000
): Promise<boolean> {
  const intervalMs = 2000;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await isInstalled(binary)) return true;
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  return false;
}

async function promptInstall(
  language: string,
  options: LspOption[],
  context: vscode.ExtensionContext
): Promise<string | undefined> {
  type InstallItem = vscode.QuickPickItem & { option?: LspOption; custom?: true };

  const items: InstallItem[] = options.map((opt) => ({
    label: `Install ${opt.label}`,
    description: opt.installCmd,
    detail: opt.installNote,
    option: opt,
  }));
  items.push({ label: "Enter custom LSP command…", custom: true });

  const picked = await vscode.window.showQuickPick(items, {
    title: `No ${language} LSP server found — choose one to install`,
    placeHolder: "Select an LSP server to install or enter a custom command",
  });

  if (!picked) return undefined;

  if (picked.custom) {
    const cmd = await vscode.window.showInputBox({
      prompt: `Enter the full LSP launch command for ${language}`,
      placeHolder: "e.g. /path/to/my-lsp --stdio",
    });
    if (!cmd) return undefined;
    const cfg = vscode.workspace.getConfiguration("lspDepExtractor");
    const existing = cfg.get<Record<string, string>>("customLspCommands", {});
    await cfg.update(
      "customLspCommands",
      { ...existing, [language]: cmd },
      vscode.ConfigurationTarget.Global
    );
    return cmd;
  }

  const opt = picked.option!;
  const terminal = vscode.window.createTerminal("LSP Install");
  terminal.show();
  terminal.sendText(opt.installCmd);

  const installed = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: `Installing ${opt.label}…`,
      cancellable: false,
    },
    () => pollUntilInstalled(opt.binary)
  );

  if (!installed) {
    vscode.window.showErrorMessage(
      `Timed out waiting for "${opt.binary}" to appear in PATH. ` +
        `Run the install command in the terminal, then try again.`
    );
    return undefined;
  }

  // Remember the choice so we don't prompt again
  const cfg = vscode.workspace.getConfiguration("lspDepExtractor");
  const existing = cfg.get<Record<string, string>>("preferredLsp", {});
  await cfg.update(
    "preferredLsp",
    { ...existing, [language]: opt.id },
    vscode.ConfigurationTarget.Global
  );

  return opt.command;
}

export async function resolveServerCommand(
  language: string,
  context: vscode.ExtensionContext
): Promise<string | undefined> {
  const cfg = vscode.workspace.getConfiguration("lspDepExtractor");

  // 1. Fully custom command — use as-is, no binary check
  const customCmds = cfg.get<Record<string, string>>("customLspCommands", {});
  if (customCmds[language]) {
    return customCmds[language];
  }

  const options = LSP_REGISTRY[language];
  if (!options) {
    vscode.window.showErrorMessage(`No LSP registry entry for language "${language}".`);
    return undefined;
  }

  // 2. Pinned preferred LSP
  const preferred = cfg.get<Record<string, string>>("preferredLsp", {});
  if (preferred[language]) {
    const opt = options.find((o) => o.id === preferred[language]);
    if (opt) {
      if (await isInstalled(opt.binary)) {
        return opt.command;
      }
      vscode.window.showWarningMessage(
        `Preferred LSP "${opt.label}" not found in PATH. Falling back to auto-detect.`
      );
    }
  }

  // 3. Auto-detect all installed options in priority order
  const found: LspOption[] = [];
  for (const opt of [...options].sort((a, b) => a.priority - b.priority)) {
    if (await isInstalled(opt.binary)) {
      found.push(opt);
    }
  }

  if (found.length === 1) {
    const statusBar = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Left
    );
    statusBar.text = `$(check) Using ${found[0].label} for ${language}`;
    statusBar.show();
    setTimeout(() => statusBar.dispose(), 5000);
    return found[0].command;
  }

  if (found.length > 1) {
    type FoundItem = vscode.QuickPickItem & { option: LspOption };
    const items: FoundItem[] = found.map((opt) => ({
      label: opt.label,
      description: opt.command,
      option: opt,
    }));
    const picked = await vscode.window.showQuickPick(items, {
      title: `Multiple ${language} LSP servers found — pick one`,
      placeHolder: "Select which LSP server to use",
    });
    if (!picked) return undefined;

    // Remember choice in workspace state
    context.workspaceState.update(`lspChoice.${language}`, picked.option.id);
    return picked.option.command;
  }

  // 4. Nothing found — offer install
  return promptInstall(language, options, context);
}
