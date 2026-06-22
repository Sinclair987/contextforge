use std::path::{Component, Path, PathBuf};

pub(crate) fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    std::env::current_dir()
        .map(|current_dir| current_dir.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn relative_display(source: &Path, path: &Path) -> String {
    let source = absolute(source);
    let path = absolute(path);
    let relative = path.strip_prefix(&source).unwrap_or(&path);
    let parts = relative
        .components()
        .filter_map(|component| match component {
            Component::CurDir => None,
            Component::Normal(part) => Some(part.to_string_lossy()),
            _ => Some(component.as_os_str().to_string_lossy()),
        })
        .collect::<Vec<_>>();

    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}
