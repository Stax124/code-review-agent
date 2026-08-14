use std::path::Path;

use crate::utils::conversion::bytes_to_human_readable;

#[derive(Debug, serde::Serialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub is_file: bool,
    pub size: Option<u64>,
}

pub fn list_directory(
    base_path: &Path,
    subdirectory: Option<&str>,
) -> color_eyre::Result<Vec<DirectoryEntry>> {
    // Verify that subdirectory is at or below the base_path to prevent directory traversal attacks
    if let Some(subdir) = subdirectory {
        let canonical_base = base_path.canonicalize()?;
        let subdir_path = base_path.join(subdir).canonicalize()?;
        if !subdir_path.starts_with(&canonical_base) {
            // Return empty if the subdirectory is outside the base path
            return Ok(vec![]);
        }
    }

    let path = if let Some(subdir) = subdirectory {
        base_path.join(subdir)
    } else {
        base_path.to_path_buf()
    };
    let mut files = Vec::<DirectoryEntry>::new();

    if let Ok(entries) = std::fs::read_dir(&path) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().into_string().unwrap_or_default();
            let is_file = entry.file_type().map(|ft| ft.is_file()).unwrap_or(false);
            let size = if is_file {
                entry.metadata().ok().map(|meta| meta.len())
            } else {
                None
            };

            files.push(DirectoryEntry {
                name: file_name,
                is_file,
                size,
            });
        }
    }

    // Sort files: directories first, then files, both alphabetically
    files.sort_by(|a, b| {
        if a.is_file == b.is_file {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else if a.is_file {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Less
        }
    });

    Ok(files)
}

pub fn prettify_directory_listing(entries: &[DirectoryEntry]) -> String {
    entries
        .iter()
        .map(|entry| {
            let maybe_slash = if entry.is_file { "" } else { "/" };
            let size_str = entry
                .size
                .map(|size| format!(" [{}]", bytes_to_human_readable(size)))
                .unwrap_or_default();
            format!("{}{}{}", entry.name, maybe_slash, size_str)
        })
        .collect::<Vec<String>>()
        .join("\n")
}
