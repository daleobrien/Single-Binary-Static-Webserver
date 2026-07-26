use std::hash::{Hash, Hasher};

pub(super) fn compute_version_hash() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before Unix epoch");
    let mut build_hasher = std::collections::hash_map::DefaultHasher::new();
    now.hash(&mut build_hasher);
    format!("{:016x}", build_hasher.finish())
}
