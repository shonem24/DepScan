// Checks that the tool and its language servers are installed.
// Run through `npm run verify`. Looks in the same places the tool does:
// .tools/node_modules/.bin, .tools/jdtls/bin, PATH, then npm's global folder.
const { spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const root = path.join(__dirname, "..");
const isWindows = process.platform === "win32";
const extensions = isWindows
  ? (process.env.PATHEXT || ".EXE;.CMD;.BAT").split(";").filter(Boolean)
  : [""];
// `npm run` adds node_modules/.bin from this folder and every parent folder to
// PATH. The tool doesn't see those, so leave them out to avoid a false OK.
const isNpmAdded = (dir) =>
  dir.split(path.sep).slice(-2).join("/") === "node_modules/.bin" || dir.includes("@npmcli");
// npm's global folder, where `npm run setup` installs servers. It is often not
// on PATH when npm uses a custom prefix, so the tool checks it separately.
const npmPrefix = (spawnSync("npm", ["prefix", "-g"], { encoding: "utf8", shell: isWindows }).stdout || "").trim();
const npmGlobalBin = npmPrefix && (isWindows ? npmPrefix : path.join(npmPrefix, "bin"));
const dirs = [
  path.join(root, ".tools", "node_modules", ".bin"),
  path.join(root, ".tools", "jdtls", "bin"),
  ...(process.env.PATH || "").split(path.delimiter).filter((d) => d && !isNpmAdded(d)),
  ...(npmGlobalBin ? [npmGlobalBin] : []),
];

function find(name) {
  for (const dir of dirs) {
    for (const ext of extensions) {
      const candidate = path.join(dir, name + ext);
      if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) return candidate;
    }
  }
  return null;
}

let ok = true;
function check(label, found, fix, required = true) {
  if (found) console.log(`  OK       ${label}`);
  else if (required) {
    console.log(`  MISSING  ${label} → ${fix}`);
    ok = false;
  } else console.log(`  missing  ${label} → ${fix}`);
}

console.log("Checking prerequisites…");
check("cargo", find("cargo"), "install Rust from https://rustup.rs");
const binary = path.join(root, "target", "release", `lsp-dep-extractor${isWindows ? ".exe" : ""}`);
check("lsp-dep-extractor (built)", fs.existsSync(binary), "cargo build --release");

console.log("  Language servers:");
check("pyright-langserver (python)", find("pyright-langserver"), "npm run setup");
check("typescript-language-server (typescript)", find("typescript-language-server"), "npm run setup");
// rustup leaves a rust-analyzer proxy in ~/.cargo/bin even when the component
// isn't installed, so run it instead of only checking that the file exists.
const rustAnalyzer = find("rust-analyzer");
const rustAnalyzerWorks =
  rustAnalyzer &&
  (spawnSync(rustAnalyzer, ["--version"], { encoding: "utf8" }).stdout || "").startsWith("rust-analyzer");
check("rust-analyzer (rust)", rustAnalyzerWorks, "rustup component add rust-analyzer", false);
check("jdtls (java)", find("jdtls"), "see README: Scanning Java", false);

if (ok) {
  console.log("Ready: cargo run --release -- /path/to/repo");
} else {
  console.log("Fix the MISSING items above, then re-run npm run verify.");
  process.exit(1);
}
