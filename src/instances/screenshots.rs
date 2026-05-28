use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub fn screenshot_dirs(instance_path: &Path) -> [PathBuf; 2] {
    [
        instance_path.join("screenshots"),
        instance_path.join(".minecraft").join("screenshots"),
    ]
}

pub fn latest_screenshot(instance_path: &Path) -> Option<PathBuf> {
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for dir in screenshot_dirs(instance_path) {
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("png") {
                continue;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|meta| meta.modified().ok())?;
            if newest.as_ref().is_none_or(|(time, _)| modified > *time) {
                newest = Some((modified, path));
            }
        }
    }
    newest.map(|(_, path)| path)
}

pub fn refresh_artwork(instance_path: &Path) -> Option<PathBuf> {
    latest_screenshot(instance_path)
}
