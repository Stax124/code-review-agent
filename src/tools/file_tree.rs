use std::path::{Path, PathBuf};

use async_trait::async_trait;
use ignore::WalkBuilder;
use indexmap::IndexMap;
use serde_json::{Value, json};

use crate::tools::AgentTool;
use crate::utils::conversion::bytes_to_human_readable;

/// Maximum depth the walker will descend. Prevents pathological deep trees
/// from blowing up the output.
const MAX_WALK_DEPTH: usize = 10;

/// Maximum number of entries (files + directories) included in the tree.
/// Beyond this we stop walking and report a truncation notice.
const MAX_ENTRIES: usize = 5_000;

/// Maximum size of the rendered output in bytes. Mirrors the 1 MB cap that
/// `read_file` enforces to keep responses within the model's context window.
const MAX_OUTPUT_BYTES: usize = 1_000_000;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FileTreeToolArgs {
    subdirectory: Option<String>,
}

/// A node in the rendered file tree.
#[derive(Debug)]
struct TreeNode {
    name: String,
    is_dir: bool,
    size: Option<u64>,
    children: IndexMap<String, TreeNode>,
}

impl TreeNode {
    fn new_dir(name: String) -> Self {
        Self {
            name,
            is_dir: true,
            size: None,
            children: IndexMap::new(),
        }
    }

    fn new_file(name: String, size: Option<u64>) -> Self {
        Self {
            name,
            is_dir: false,
            size,
            children: IndexMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct FileTreeTool {}

impl FileTreeTool {
    pub fn new() -> Self {
        Self {}
    }

    /// Build a file tree rooted at `base_path`, honoring `.gitignore`
    fn build_file_tree(base_path: &Path) -> color_eyre::Result<(TreeNode, bool)> {
        let mut root = TreeNode::new_dir(
            base_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| base_path.to_string_lossy().into_owned()),
        );

        let mut walker = WalkBuilder::new(base_path);
        // Don't follow symlinks to avoid loops and surprising traversal.
        walker.follow_links(false);
        // Bound the depth to avoid pathological trees.
        walker.max_depth(Some(MAX_WALK_DEPTH));

        let mut entry_count: usize = 0;
        let mut truncated = false;

        for entry in walker.build() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    tracing::debug!("file_tree: skipping entry: {}", err);
                    continue;
                }
            };

            let entry_path = entry.path();
            // Skip the root itself; we already created a node for it.
            if entry_path == base_path {
                continue;
            }

            let relative = match entry_path.strip_prefix(base_path) {
                Ok(rel) => rel,
                Err(_) => continue,
            };

            entry_count += 1;
            if entry_count > MAX_ENTRIES {
                truncated = true;
                break;
            }

            let file_name = entry_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();

            let file_type = entry.file_type();
            let is_dir = match file_type {
                Some(ft) => ft.is_dir(),
                None => entry_path.is_dir(),
            };
            let size = if !is_dir {
                match entry.metadata() {
                    Ok(md) => Some(md.len()),
                    Err(err) => {
                        tracing::warn!(
                            "file_tree: metadata failed for {}: {}",
                            entry_path.display(),
                            err
                        );
                        None
                    }
                }
            } else {
                None
            };

            insert_into_tree(&mut root, relative, file_name, is_dir, size);
        }

        sort_children(&mut root);
        Ok((root, truncated))
    }

    /// Render a tree node as a human-readable string.
    fn render_tree(node: &TreeNode, truncated: bool) -> String {
        let mut output = String::new();
        output.push_str(&node.name);
        output.push('\n');
        render_children(node, "", &mut output);

        if truncated {
            let notice = format!(
                "\n... truncated: exceeded {} entries or {} bytes.\n",
                MAX_ENTRIES, MAX_OUTPUT_BYTES
            );
            output.push_str(&notice);
        } else if output.len() > MAX_OUTPUT_BYTES {
            // Find the nearest char boundary at or before MAX_OUTPUT_BYTES
            let mut cut = MAX_OUTPUT_BYTES;
            while cut > 0 && !output.is_char_boundary(cut) {
                cut -= 1;
            }
            output.truncate(cut);
            output.push_str(&format!(
                "\n... truncated: output exceeded {} bytes.\n",
                MAX_OUTPUT_BYTES
            ));
        }

        output
    }
}

/// Insert a new entry into the tree, creating intermediate directory nodes as needed.
fn insert_into_tree(
    root: &mut TreeNode,
    relative: &Path,
    file_name: String,
    is_dir: bool,
    size: Option<u64>,
) {
    let mut current = root;
    // Walk every parent component (all but the final segment, which is the
    // entry itself).
    let parent_components: Vec<_> = relative
        .parent()
        .map(|p| p.components().collect())
        .unwrap_or_default();

    for component in parent_components {
        let name = component.as_os_str().to_string_lossy().into_owned();
        if !current.children.contains_key(&name) {
            current
                .children
                .insert(name.clone(), TreeNode::new_dir(name.clone()));
        }
        current = current
            .children
            .get_mut(&name)
            .expect("directory node just inserted");
    }

    if is_dir {
        current
            .children
            .entry(file_name.clone())
            .or_insert_with(|| TreeNode::new_dir(file_name.clone()));
    } else {
        let node = current
            .children
            .entry(file_name.clone())
            .or_insert_with(|| TreeNode::new_file(file_name.clone(), size));
        // If we already had a placeholder and now learn it's a file, set the size.
        if node.size.is_none() && size.is_some() {
            node.size = size;
        }
    }
}

/// Recursively sort children: directories first, then files, both
/// alphabetically (case-insensitive).
fn sort_children(node: &mut TreeNode) {
    for child in node.children.values_mut() {
        sort_children(child);
    }
    // `IndexMap` doesn't have a built-in sort; rebuild in sorted order.
    let entries: Vec<(String, TreeNode)> = node.children.drain(..).collect();
    let mut entries = entries;
    entries.sort_by(|a, b| {
        let a_is_dir = a.1.is_dir;
        let b_is_dir = b.1.is_dir;
        if a_is_dir == b_is_dir {
            a.0.to_lowercase().cmp(&b.0.to_lowercase())
        } else if a_is_dir {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        }
    });
    node.children.extend(entries);
}

fn render_children(node: &TreeNode, prefix: &str, output: &mut String) {
    if output.len() > MAX_OUTPUT_BYTES {
        return;
    }
    let children: Vec<&TreeNode> = node.children.values().collect();
    let last_index = children.len().saturating_sub(1);
    for (i, child) in children.iter().enumerate() {
        let is_last = i == last_index;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };

        output.push_str(prefix);
        output.push_str(connector);
        output.push_str(&child.name);
        if child.is_dir {
            output.push('/');
        } else if let Some(size) = child.size {
            output.push_str(&format!(" [{}]", bytes_to_human_readable(size)));
        }
        output.push('\n');

        if child.is_dir && !child.children.is_empty() {
            let new_prefix = format!("{}{}", prefix, child_prefix);
            render_children(child, &new_prefix, output);
        }
    }
}

#[async_trait]
impl AgentTool for FileTreeTool {
    fn name(&self) -> &'static str {
        "file_tree"
    }

    fn description(&self) -> &str {
        "Output a recursive file tree of the current directory. Files and directories ignored by .gitignore (and other ignore files) are excluded. Directories are shown with a trailing slash and files include their size. Output is truncated at 1 MB or 5,000 entries or 10 levels of depth, whichever comes first."
    }

    fn properties_schema(&self) -> Value {
        json!({
            "subdirectory": {
                "type": "string",
                "description": "Optional subdirectory to scope the tree to. If not provided, the current directory is used."
            }
        })
    }

    fn required_parameters(&self) -> Vec<&'static str> {
        vec![]
    }

    async fn execute(
        &self,
        args: Value,
    ) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
        let args = serde_json::from_value::<FileTreeToolArgs>(args)?;

        // Verify that the subdirectory is at or below the current directory to
        // prevent directory traversal attacks.
        let current_dir = std::env::current_dir()?.canonicalize()?;
        let full_path: PathBuf = match &args.subdirectory {
            Some(subdir) => {
                let candidate = current_dir.join(subdir).canonicalize()?;
                if !candidate.starts_with(&current_dir) {
                    return Err(format!(
                        "The subdirectory '{}' is outside of the current directory boundary.",
                        subdir
                    )
                    .into());
                }
                if !candidate.is_dir() {
                    return Err(format!("'{}' is not a directory", subdir).into());
                }
                candidate
            }
            None => current_dir.clone(),
        };

        let (tree, truncated) = Self::build_file_tree(&full_path)?;
        let output = Self::render_tree(&tree, truncated);

        let subdir_label = args.subdirectory.as_deref().unwrap_or(".");
        let summary = if truncated {
            format!(
                "file_tree: {} (truncated at {} entries / {} bytes)",
                subdir_label, MAX_ENTRIES, MAX_OUTPUT_BYTES
            )
        } else {
            format!("file_tree: {}", subdir_label)
        };

        Ok((output, summary))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a temporary directory with the given layout and return its path.
    /// Layout is a list of (relative_path, is_dir) pairs.
    fn make_tree(layout: &[(&str, bool)]) -> PathBuf {
        let td = tempfile::tempdir().expect("tempdir");
        let root = td.path().to_path_buf();
        for (rel, is_dir) in layout {
            let p = root.join(rel);
            if *is_dir {
                fs::create_dir_all(&p).expect("mkdir");
            } else {
                if let Some(parent) = p.parent() {
                    fs::create_dir_all(parent).expect("mkdir parent");
                }
                fs::write(&p, b"hello").expect("write file");
            }
        }
        // Disable cleanup so the directory lives for the duration of the test.
        td.keep()
    }

    #[test]
    fn empty_directory() {
        let root = make_tree(&[]);
        let (tree, truncated) = FileTreeTool::build_file_tree(&root).unwrap();
        assert!(!truncated);
        assert!(tree.children.is_empty());
    }

    #[test]
    fn single_file() {
        let root = make_tree(&[("foo.txt", false)]);
        let (tree, truncated) = FileTreeTool::build_file_tree(&root).unwrap();
        assert!(!truncated);
        assert_eq!(tree.children.len(), 1);
        let foo = tree.children.get("foo.txt").unwrap();
        assert!(!foo.is_dir);
        assert_eq!(foo.size, Some(5));
    }

    #[test]
    fn nested_directories() {
        let root = make_tree(&[("a/b/c/deep.txt", false), ("a/top.txt", false)]);
        let (tree, truncated) = FileTreeTool::build_file_tree(&root).unwrap();
        assert!(!truncated);

        let a = tree.children.get("a").expect("a/ exists");
        assert!(a.is_dir);
        let b = a.children.get("b").expect("a/b/ exists");
        assert!(b.is_dir);
        let c = b.children.get("c").expect("a/b/c/ exists");
        assert!(c.is_dir);
        assert!(c.children.contains_key("deep.txt"));

        let top = a.children.get("top.txt").expect("a/top.txt exists");
        assert!(!top.is_dir);
    }

    #[test]
    fn sort_order_directories_first() {
        let root = make_tree(&[
            ("zebra.txt", false),
            ("apple.txt", false),
            ("middle", true),
            ("first", true),
        ]);
        let (tree, _) = FileTreeTool::build_file_tree(&root).unwrap();
        let names: Vec<&str> = tree.children.keys().map(|s| s.as_str()).collect();
        // Directories first (first, middle), then files (apple.txt, zebra.txt).
        assert_eq!(names, vec!["first", "middle", "apple.txt", "zebra.txt"]);
    }

    #[test]
    fn render_includes_size_and_trailing_slash() {
        let root = make_tree(&[("dir", true), ("file.txt", false)]);
        let (tree, truncated) = FileTreeTool::build_file_tree(&root).unwrap();
        assert!(!truncated);
        let rendered = FileTreeTool::render_tree(&tree, truncated);
        assert!(
            rendered.contains("dir/"),
            "dir should have trailing slash: {rendered}"
        );
        assert!(
            rendered.contains("file.txt [5"),
            "file should show size: {rendered}"
        );
    }
}
