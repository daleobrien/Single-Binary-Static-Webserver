use std::fs;
use std::path::Path;

/// Recursively walk `public/` and return file paths relative to that directory
/// (e.g. `"images/nested-test.svg"`). Directories are descended, not returned.
pub(super) fn collect_source_files() -> Vec<String> {
    let public_dir = "../public";
    let mut files: Vec<String> = Vec::new();
    walk_dir(Path::new(public_dir), "", &mut files);
    files.sort();
    files
}

fn walk_dir(dir: &Path, prefix: &str, files: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("failed to read public/ directory") {
        let entry = entry.expect("failed to read entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        let rel_path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if entry.file_type().unwrap().is_dir() {
            walk_dir(&entry.path(), &rel_path, files);
        } else {
            files.push(rel_path);
        }
    }
}
