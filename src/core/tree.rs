//! Directory structure rendering.

use std::collections::HashMap;

use crate::util::path::{locale_cmp, split_segments};

/// A node in the rendered directory tree.
#[derive(Debug, Default)]
struct TreeNode {
    name: String,
    is_directory: bool,
    children: Vec<TreeNode>,
    child_index: HashMap<String, usize>,
}

impl TreeNode {
    fn new(name: &str, is_directory: bool) -> TreeNode {
        TreeNode {
            name: name.to_string(),
            is_directory,
            children: Vec::new(),
            child_index: HashMap::new(),
        }
    }

    fn child_mut(&mut self, name: &str) -> &mut TreeNode {
        if let Some(&idx) = self.child_index.get(name) {
            return &mut self.children[idx];
        }
        self.children.push(TreeNode::new(name, false));
        let idx = self.children.len() - 1;
        self.child_index.insert(name.to_string(), idx);
        &mut self.children[idx]
    }

    fn sort(&mut self) {
        self.children.sort_by(|a, b| {
            if a.is_directory == b.is_directory {
                locale_cmp(&a.name, &b.name)
            } else if a.is_directory {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });
        self.child_index.clear();
        for (i, child) in self.children.iter().enumerate() {
            self.child_index.insert(child.name.clone(), i);
        }
        for child in &mut self.children {
            child.sort();
        }
    }
}

/// Build a tree from a list of file paths and empty directory paths.
fn build_tree(files: &[String], empty_dirs: &[String]) -> TreeNode {
    let mut root = TreeNode::new("root", true);
    for file in files {
        add_path(&mut root, file, false);
    }
    for dir in empty_dirs {
        add_path(&mut root, dir, true);
    }
    root.sort();
    root
}

fn add_path(root: &mut TreeNode, path: &str, is_directory: bool) {
    let parts = split_segments(path);
    if parts.is_empty() {
        return;
    }
    let mut current = root;
    for (i, part) in parts.iter().enumerate() {
        let is_last = i + 1 == parts.len();
        let child = current.child_mut(part);
        if is_last {
            child.is_directory = is_directory;
        } else {
            child.is_directory = true;
        }
        current = child;
    }
}

fn render(node: &TreeNode, prefix: &str, out: &mut String) {
    for child in &node.children {
        out.push_str(prefix);
        out.push_str(&child.name);
        if child.is_directory {
            out.push('/');
        }
        out.push('\n');
        if child.is_directory {
            let nested = format!("{prefix}  ");
            render(child, &nested, out);
        }
    }
}

/// Render the directory tree for a single root.
pub fn generate_tree_string(files: &[String], empty_dirs: &[String]) -> String {
    let tree = build_tree(files, empty_dirs);
    let mut out = String::new();
    render(&tree, "", &mut out);
    out.trim().to_string()
}

/// Render the directory tree for multiple roots.
///
/// Each root gets its own `[label]/` section.
pub fn generate_tree_string_with_roots(
    files_by_root: &[(String, Vec<String>)],
    empty_dirs: &[String],
) -> String {
    if files_by_root.len() == 1 {
        return generate_tree_string(&files_by_root[0].1, empty_dirs);
    }
    let mut sections: Vec<String> = Vec::new();
    for (label, files) in files_by_root {
        if files.is_empty() {
            continue;
        }
        let tree = build_tree(files, &[]);
        let mut out = String::new();
        render(&tree, "  ", &mut out);
        sections.push(format!("[{label}]/\n{out}"));
    }
    sections.join("\n").trim().to_string()
}

/// Render the directory tree including per-file line counts.
pub fn generate_tree_string_with_line_counts(
    files: &[String],
    line_counts: &HashMap<String, usize>,
    empty_dirs: &[String],
) -> String {
    let tree = build_tree(files, empty_dirs);
    let mut out = String::new();
    render_with_counts(&tree, "", "", line_counts, &mut out);
    out.trim().to_string()
}

fn render_with_counts(
    node: &TreeNode,
    prefix: &str,
    current_path: &str,
    line_counts: &HashMap<String, usize>,
    out: &mut String,
) {
    for child in &node.children {
        let child_path = if current_path.is_empty() {
            child.name.clone()
        } else {
            format!("{current_path}/{}", child.name)
        };
        if child.is_directory {
            out.push_str(prefix);
            out.push_str(&child.name);
            out.push_str("/\n");
            let nested = format!("{prefix}  ");
            render_with_counts(child, &nested, &child_path, line_counts, out);
        } else {
            out.push_str(prefix);
            out.push_str(&child.name);
            if let Some(count) = line_counts.get(&child_path) {
                out.push_str(&format!(" ({count} lines)"));
            }
            out.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_tree_with_directories_first() {
        let files = vec!["src/main.rs".to_string(), "README.md".to_string()];
        let tree = generate_tree_string(&files, &[]);
        assert_eq!(tree, "README.md\nsrc/\n  main.rs");
    }

    #[test]
    fn renders_multiple_roots() {
        let roots = vec![
            ("src".to_string(), vec!["src/a.rs".to_string()]),
            ("tests".to_string(), vec!["tests/b.rs".to_string()]),
        ];
        let tree = generate_tree_string_with_roots(&roots, &[]);
        assert_eq!(tree, "[src]/\n  a.rs\n[tests]/\n  b.rs");
    }
}
