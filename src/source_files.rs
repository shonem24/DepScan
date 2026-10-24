use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::{DirEntry, WalkBuilder};

pub fn load_source_files(
    project_root: &Path,
    extensions: &[&str],
) -> Result<Vec<(PathBuf, String)>> {
    let paths = discover_files(project_root, extensions)?;
    read_files(&paths)
}

fn discover_files(project_root: &Path, extensions: &[&str]) -> Result<Vec<PathBuf>> {
    let project_root = project_root.canonicalize()?;
    let mut paths: Vec<PathBuf> = Vec::new();
    let walker = WalkBuilder::new(&project_root)
        .hidden(false)
        .require_git(false)
        .filter_entry(should_descend)
        .build();

    for entry in walker {
        let entry = entry?;
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            continue;
        }

        let path = entry.path();
        let name = entry.file_name().to_string_lossy();
        if extensions.iter().any(|ext| name.ends_with(ext)) {
            if let Ok(canonical) = path.canonicalize() {
                paths.push(canonical);
            }
        }
    }

    paths.sort_by(|a, b| {
        let ra = a.strip_prefix(&project_root).unwrap_or(a);
        let rb = b.strip_prefix(&project_root).unwrap_or(b);
        ra.cmp(rb)
    });
    Ok(paths)
}

fn should_descend(entry: &DirEntry) -> bool {
    entry.file_name() != ".git"
}

fn read_files(paths: &[PathBuf]) -> Result<Vec<(PathBuf, String)>> {
    paths
        .iter()
        .map(|p| Ok((p.clone(), std::fs::read_to_string(p)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestProject {
        root: PathBuf,
    }

    impl TestProject {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("lsp-dep-{name}-{unique}"));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn write(&self, rel_path: &str, contents: &str) {
            let path = self.root.join(rel_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn discovery_honors_gitignore_rules() {
        let project = TestProject::new("gitignore");
        project.write(".gitignore", "target/\n*.gen.py\n");
        project.write("src/main.py", "");
        project.write("target/generated.py", "");
        project.write("src/schema.gen.py", "");

        let files = discover_files(&project.root, &[".py"]).unwrap();
        let root = project.root.canonicalize().unwrap();
        let rel_paths: Vec<_> = files
            .iter()
            .map(|path| path.strip_prefix(&root).unwrap().to_path_buf())
            .collect();

        assert_eq!(rel_paths, vec![PathBuf::from("src/main.py")]);
    }

    #[test]
    fn discovery_honors_gitignore_negations() {
        let project = TestProject::new("gitignore-negation");
        project.write(".gitignore", "generated/*\n!generated/keep.py\n");
        project.write("generated/drop.py", "");
        project.write("generated/keep.py", "");

        let files = discover_files(&project.root, &[".py"]).unwrap();
        let root = project.root.canonicalize().unwrap();
        let rel_paths: Vec<_> = files
            .iter()
            .map(|path| path.strip_prefix(&root).unwrap().to_path_buf())
            .collect();

        assert_eq!(rel_paths, vec![PathBuf::from("generated/keep.py")]);
    }
}
