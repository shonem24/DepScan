pub mod client;
pub use client::LspClient;

use std::path::{Path, PathBuf};

pub fn path_to_uri(path: &Path) -> String {
    let raw = path.display().to_string();
    let encoded = raw
        .chars()
        .flat_map(|c| match c {
            ' ' => vec!['%', '2', '0'],
            '#' => vec!['%', '2', '3'],
            '?' => vec!['%', '3', 'F'],
            '[' => vec!['%', '5', 'B'],
            ']' => vec!['%', '5', 'D'],
            _ => vec![c],
        })
        .collect::<String>();
    format!("file://{}", encoded)
}

pub fn uri_to_path(uri: &str) -> PathBuf {
    let raw = uri.strip_prefix("file://").unwrap_or(uri);
    let decoded = raw
        .replace("%20", " ")
        .replace("%23", "#")
        .replace("%3F", "?")
        .replace("%5B", "[")
        .replace("%5D", "]");
    PathBuf::from(decoded)
}
