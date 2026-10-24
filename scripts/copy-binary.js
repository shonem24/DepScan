// Copies the release binary into the VS Code extension under the name it
// expects for this OS. Run through `npm run extension`.
const fs = require("fs");
const path = require("path");

const root = path.join(__dirname, "..");
const exe = process.platform === "win32" ? ".exe" : "";
const suffix = { darwin: "darwin", win32: "win.exe" }[process.platform] || "linux";

const source = path.join(root, "target", "release", `lsp-dep-extractor${exe}`);
const binDir = path.join(root, "vscode-extension", "bin");
const destination = path.join(binDir, `lsp-dep-extractor-${suffix}`);

fs.mkdirSync(binDir, { recursive: true });
fs.copyFileSync(source, destination);
console.log(`Copied binary to ${destination}`);
