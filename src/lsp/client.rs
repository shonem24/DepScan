use std::cell::Cell;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use serde_json::{json, Value};

#[derive(Default)]
struct CallStats {
    count: u64,
    total_ms: f64,
    max_ms: f64,
}

impl CallStats {
    fn record(&mut self, ms: f64) {
        self.count += 1;
        self.total_ms += ms;
        if ms > self.max_ms {
            self.max_ms = ms;
        }
    }

    fn avg_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total_ms / self.count as f64
        }
    }
}

pub struct LspClient {
    stdin: BufWriter<ChildStdin>,
    rx: Receiver<Value>,
    next_id: u32,
    child: Child,
    ready: Cell<bool>,
    saw_status: Cell<bool>,
    stat_init: CallStats,
    stat_did_open: CallStats,
    stat_drain: CallStats,
    stat_symbol: CallStats,
    stat_def: CallStats,
}

impl LspClient {
    pub fn start(command: Vec<String>) -> Result<Self> {
        let mut child = Command::new(resolve_program(&command[0]))
            .args(&command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdout = child.stdout.take().unwrap();
        let stdin = child.stdin.take().unwrap();

        let (tx, rx) = channel();
        thread::Builder::new()
            .name("lsp-reader".into())
            .spawn(move || reader_loop(stdout, tx))?;

        Ok(LspClient {
            stdin: BufWriter::new(stdin),
            rx,
            next_id: 1,
            child,
            ready: Cell::new(false),
            saw_status: Cell::new(false),
            stat_init: CallStats::default(),
            stat_did_open: CallStats::default(),
            stat_drain: CallStats::default(),
            stat_symbol: CallStats::default(),
            stat_def: CallStats::default(),
        })
    }

    pub fn initialize(&mut self, root_uri: &str, init_options: Value) -> Result<()> {
        let t = Instant::now();
        let id = self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "workspaceFolders": [{ "uri": root_uri, "name": "project" }],
                "capabilities": {
                    "textDocument": {
                        "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
                        "definition": {},
                    },
                    "experimental": { "serverStatusNotification": true }
                },
                "initializationOptions": init_options,
            }),
        )?;
        self.await_response(id)?;
        self.stat_init.record(t.elapsed().as_secs_f64() * 1000.0);
        self.notify("initialized", json!({}))?;
        Ok(())
    }

    pub fn did_open(&mut self, uri: &str, language_id: &str, text: &str) -> Result<()> {
        let t = Instant::now();
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text,
                }
            }),
        )?;
        self.stat_did_open
            .record(t.elapsed().as_secs_f64() * 1000.0);
        Ok(())
    }

    pub fn drain_until_idle(&mut self) {
        let t = Instant::now();
        // ready = server said indexing is done (serverStatus quiescent).
        // heartbeat = process still alive.
        //   ready                         → move on
        //   heartbeat + !ready            → keep waiting
        //   !heartbeat + !ready           → crashed / gone
        // Servers that never send status (pyright) still exit on 2s silence.
        loop {
            if self.ready.get() {
                break;
            }
            match self.rx.recv_timeout(Duration::from_secs(2)) {
                Ok(msg) => self.observe(&msg),
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {
                    let alive = matches!(self.child.try_wait(), Ok(None));
                    if !alive || !self.saw_status.get() || t.elapsed() > Duration::from_secs(300) {
                        break;
                    }
                }
            }
        }
        self.stat_drain.record(t.elapsed().as_secs_f64() * 1000.0);
    }

    fn observe(&self, msg: &Value) {
        if msg.get("method").and_then(|m| m.as_str()) == Some("experimental/serverStatus") {
            self.saw_status.set(true);
            self.ready
                .set(msg["params"]["quiescent"].as_bool().unwrap_or(false));
        }
    }

    pub fn document_symbols(&mut self, uri: &str) -> Result<Vec<Value>> {
        let id = self.request(
            "textDocument/documentSymbol",
            json!({
                "textDocument": { "uri": uri }
            }),
        )?;
        let t = Instant::now();
        let result = self.await_response(id)?;
        self.stat_symbol.record(t.elapsed().as_secs_f64() * 1000.0);
        Ok(result.as_array().cloned().unwrap_or_default())
    }

    pub fn definition(&mut self, uri: &str, line: u32, character: u32) -> Result<Vec<Value>> {
        let id = self.request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character },
            }),
        )?;
        let t = Instant::now();
        let result = self.await_response_cancellable(id)?;
        self.stat_def.record(t.elapsed().as_secs_f64() * 1000.0);
        Ok(match result {
            None => vec![],
            Some(Value::Array(arr)) => arr,
            Some(obj @ Value::Object(_)) => vec![obj],
            _ => vec![],
        })
    }

    pub fn print_profile(&self) {
        eprintln!("\n── LSP call profile ────────────────────────────────────────────────");
        eprintln!(
            "{:<30} {:>8} {:>10} {:>10} {:>10}",
            "method", "calls", "total ms", "avg ms", "max ms"
        );
        eprintln!("{}", "─".repeat(72));
        for (name, s) in [
            ("initialize", &self.stat_init),
            ("textDocument/didOpen", &self.stat_did_open),
            ("drain_until_idle", &self.stat_drain),
            ("textDocument/docSymbol", &self.stat_symbol),
            ("textDocument/definition", &self.stat_def),
        ] {
            eprintln!(
                "{:<30} {:>8} {:>10.1} {:>10.2} {:>10.2}",
                name,
                s.count,
                s.total_ms,
                s.avg_ms(),
                s.max_ms
            );
        }
        eprintln!("{}", "─".repeat(72));
        let total = self.stat_init.total_ms
            + self.stat_did_open.total_ms
            + self.stat_drain.total_ms
            + self.stat_symbol.total_ms
            + self.stat_def.total_ms;
        eprintln!("{:<30} {:>8} {:>10.1}", "total LSP time", "", total);
    }

    pub fn shutdown(&mut self) -> Result<()> {
        let id = self.request("shutdown", json!(null))?;
        self.await_response(id)?;
        self.notify("exit", json!(null))?;
        let _ = self.child.wait();
        Ok(())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<u32> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        Ok(id)
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    fn send(&mut self, msg: Value) -> Result<()> {
        let body = serde_json::to_string(&msg)?;
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
        self.stdin.flush()?;
        Ok(())
    }

    fn await_response(&self, target_id: u32) -> Result<Value> {
        loop {
            let msg = self
                .rx
                .recv()
                .map_err(|_| anyhow::anyhow!("LSP server disconnected"))?;
            self.observe(&msg);
            let is_response = msg
                .get("id")
                .and_then(|id| id.as_u64())
                .map(|id| id == target_id as u64)
                .unwrap_or(false);
            if !is_response {
                continue;
            }
            if let Some(error) = msg.get("error") {
                bail!("LSP error: {}", error);
            }
            return Ok(msg["result"].clone());
        }
    }

    fn await_response_cancellable(&self, target_id: u32) -> Result<Option<Value>> {
        loop {
            let msg = self
                .rx
                .recv()
                .map_err(|_| anyhow::anyhow!("LSP server disconnected"))?;
            self.observe(&msg);
            let is_response = msg
                .get("id")
                .and_then(|id| id.as_u64())
                .map(|id| id == target_id as u64)
                .unwrap_or(false);
            if !is_response {
                continue;
            }
            if let Some(error) = msg.get("error") {
                let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
                if matches!(code, -32800 | -32801 | -32603) {
                    // Definition lookup is best-effort. Some servers, notably jdtls,
                    // can return per-position internal errors for unresolved or
                    // compiler-synthesized selections while remaining usable.
                    return Ok(None);
                }
                bail!("LSP error: {}", error);
            }
            return Ok(Some(msg["result"].clone()));
        }
    }
}

/// Finds the server program for `name`, checking in order:
/// 1. `.tools/node_modules/.bin` and `.tools/jdtls/bin`, where `npm run setup`
///    and the README's jdtls steps install servers. They are looked for next to the binary
///    (target/release/../../) and in the current directory.
/// 2. PATH. On Windows this uses PATHEXT, because `Command::new` only finds
///    `.exe` files and npm installs servers as `.cmd` launchers.
/// 3. npm's global folder (`npm prefix -g`), for servers installed with
///    `npm install -g`. It is often missing from PATH with a custom npm prefix.
///
/// Anything else, including names with a path or extension, is used as given.
fn resolve_program(name: &str) -> PathBuf {
    if Path::new(name).components().count() != 1 || Path::new(name).extension().is_some() {
        return PathBuf::from(name);
    }

    let mut dirs: Vec<PathBuf> = Vec::new();
    let roots = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.ancestors().nth(3).map(Path::to_path_buf))
        .into_iter()
        .chain(std::env::current_dir().ok());
    for root in roots {
        let tools = root.join(".tools");
        dirs.push(tools.join("node_modules").join(".bin"));
        dirs.push(tools.join("jdtls").join("bin"));
    }
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.extend(path_dirs.iter().cloned());

    if let Some(found) = find_program(name, &dirs) {
        return found;
    }
    npm_global_bin(&path_dirs)
        .and_then(|bin| find_program(name, &[bin]))
        .unwrap_or_else(|| PathBuf::from(name))
}

/// Looks for `name` in `dirs`, trying Windows executable extensions where needed.
fn find_program(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .filter(|ext| !ext.is_empty())
            .map(String::from)
            .collect()
    } else {
        vec![String::new()]
    };
    dirs.iter()
        .flat_map(|dir| {
            extensions
                .iter()
                .map(move |ext| dir.join(format!("{name}{ext}")))
        })
        .find(|candidate| candidate.is_file())
}

/// The folder npm installs global commands into: `<prefix>/bin`, or the prefix
/// itself on Windows. Returns None if npm isn't installed.
fn npm_global_bin(path_dirs: &[PathBuf]) -> Option<PathBuf> {
    let npm = find_program("npm", path_dirs)?;
    let output = Command::new(npm).args(["prefix", "-g"]).output().ok()?;
    let prefix = String::from_utf8(output.stdout).ok()?;
    let prefix = PathBuf::from(prefix.trim());
    Some(if cfg!(windows) {
        prefix
    } else {
        prefix.join("bin")
    })
}

fn reader_loop(stdout: ChildStdout, tx: Sender<Value>) {
    let mut reader = BufReader::new(stdout);
    while let Some(content_length) = read_content_length(&mut reader) {
        let mut body = vec![0u8; content_length];
        if reader.read_exact(&mut body).is_err() {
            break;
        }
        if let Ok(msg) = serde_json::from_slice::<Value>(&body) {
            if tx.send(msg).is_err() {
                break;
            }
        }
    }
}

fn read_content_length(reader: &mut impl BufRead) -> Option<usize> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return length;
        }
        if let Some(val) = trimmed.strip_prefix("Content-Length: ") {
            length = val.trim().parse().ok();
        }
    }
}
