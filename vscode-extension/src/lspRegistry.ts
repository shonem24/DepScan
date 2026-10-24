export interface LspOption {
  id: string;
  binary: string;
  command: string;
  label: string;
  installCmd: string;
  installNote?: string;
  priority: number;
}

export const LSP_REGISTRY: Record<string, LspOption[]> = {
  python: [
    {
      id: "pyright",
      binary: "pyright-langserver",
      command: "pyright-langserver --stdio",
      label: "Pyright (Microsoft)",
      installCmd: "npm install -g pyright",
      priority: 1,
    },
    {
      id: "pylsp",
      binary: "pylsp",
      command: "pylsp",
      label: "Python LSP Server (pylsp)",
      installCmd: "pip install python-lsp-server",
      priority: 2,
    },
    {
      id: "jedi-language-server",
      binary: "jedi-language-server",
      command: "jedi-language-server",
      label: "Jedi Language Server",
      installCmd: "pip install jedi-language-server",
      priority: 3,
    },
  ],
  typescript: [
    {
      id: "typescript-language-server",
      binary: "typescript-language-server",
      command: "typescript-language-server --stdio",
      label: "typescript-language-server",
      installCmd: "npm install -g typescript-language-server typescript",
      priority: 1,
    },
    {
      id: "vtsls",
      binary: "vtsls",
      command: "vtsls --stdio",
      label: "vtsls (Volar-based)",
      installCmd: "npm install -g @vtsls/language-server",
      priority: 2,
    },
    {
      id: "deno",
      binary: "deno",
      command: "deno lsp",
      label: "Deno LSP",
      installCmd: "curl -fsSL https://deno.land/install.sh | sh",
      installNote: "Only suitable for Deno projects.",
      priority: 3,
    },
  ],
  java: [
    {
      id: "jdtls",
      binary: "jdtls",
      command: "jdtls -data /tmp/lsp-dep-java",
      label: "Eclipse JDT.LS (jdtls)",
      installCmd: "brew install jdtls",
      installNote:
        "Or download from github.com/eclipse-jdtls/eclipse.jdt.ls/releases",
      priority: 1,
    },
  ],
  rust: [
    {
      id: "rust-analyzer",
      binary: "rust-analyzer",
      command: "rust-analyzer",
      label: "rust-analyzer",
      installCmd: "rustup component add rust-analyzer",
      priority: 1,
    },
  ],
};
