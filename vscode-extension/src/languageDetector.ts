import * as fs from "fs";
import * as path from "path";
import { LSP_REGISTRY } from "./lspRegistry";

const SKIP_DIRS = new Set([
  "node_modules",
  ".git",
  "dist",
  "target",
  "build",
  "out",
  "__pycache__",
  ".venv",
  "venv",
  ".mypy_cache",
  ".ruff_cache",
]);

const EXT_TO_LANGUAGE: Record<string, string> = {
  ".py": "python",
  ".ts": "typescript",
  ".tsx": "typescript",
  ".java": "java",
  ".rs": "rust",
};

function countFiles(dir: string, ext: string): number {
  let count = 0;
  try {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      if (entry.isDirectory()) {
        if (!SKIP_DIRS.has(entry.name)) {
          count += countFiles(path.join(dir, entry.name), ext);
        }
      } else if (entry.name.endsWith(ext)) {
        count++;
      }
    }
  } catch {
    // ignore permission errors
  }
  return count;
}

export interface DetectedLanguage {
  language: string;
  count: number;
}

/** Returns all languages with at least one source file, sorted by file count descending. */
export function detectLanguages(repoPath: string): DetectedLanguage[] {
  const counts: Record<string, number> = {};

  for (const [ext, lang] of Object.entries(EXT_TO_LANGUAGE)) {
    if (!(lang in counts)) counts[lang] = 0;
    counts[lang] += countFiles(repoPath, ext);
  }

  return Object.entries(counts)
    .filter(([lang, count]) => lang in LSP_REGISTRY && count > 0)
    .sort(([, a], [, b]) => b - a)
    .map(([language, count]) => ({ language, count }));
}
