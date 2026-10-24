import * as cp from "child_process";
import * as vscode from "vscode";

export function runExtractor(
  binaryPath: string,
  repoPath: string,
  language: string,
  lspOverride: string,
  outputPath: string
): Promise<void> {
  const spawnWork = (): Promise<void> =>
    new Promise<void>((resolve, reject) => {
      const args = [
        repoPath,
        "--language",
        language,
        "--output",
        outputPath,
        "--lsp-override",
        lspOverride,
      ];

      const proc = cp.spawn(binaryPath, args, { shell: false });

      const stderrChunks: Buffer[] = [];
      proc.stderr.on("data", (chunk: Buffer) => stderrChunks.push(chunk));

      proc.on("close", (code) => {
        if (code === 0) {
          resolve();
        } else {
          const stderr = Buffer.concat(stderrChunks).toString().trim();
          reject(
            new Error(
              `lsp-dep-extractor exited with code ${code}` +
                (stderr ? `\n\n${stderr}` : "")
            )
          );
        }
      });

      proc.on("error", (err) => {
        reject(new Error(`Failed to launch binary: ${err.message}`));
      });
    });

  return new Promise<void>((resolve, reject) => {
    vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: `Analyzing ${language} dependencies…`,
        cancellable: false,
      },
      () => spawnWork().then(resolve, reject)
    );
  });
}
