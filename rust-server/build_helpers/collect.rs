use std::fs;

pub(super) fn collect_source_files() -> Vec<String> {
    let public_dir = "../public";
    let mut files: Vec<String> = Vec::new();
    for entry in fs::read_dir(public_dir).expect("failed to read public/") {
        let entry = entry.expect("failed to read entry");
        if entry.file_type().unwrap().is_file() {
            files.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    files.sort();
    files
}
