use crate::module_tree::{ModuleInfo, ModulePath, ModuleTreeNode};

const MODULE_PATH_SEPARATOR: &str = "\\";

pub struct ModuleTreeView {
    /// Direct descendants of this (sub)tree
    pub children: Vec<ModuleTreeViewNode>,
}

impl ModuleTreeView {
    pub fn new() -> Self {
        ModuleTreeView {
            children: Default::default(),
        }
    }

    /// Create a new `ModuleTreeView` from a `ModuleTreeNode` by merging all
    /// nodes which only have 1 child together, recursively.
    ///
    /// This allows reducing the depth of the tree without losing information.
    /// The idea is to reduce the "size" of the tree to ease browsing.
    pub fn from_tree_node(root_node: ModuleTreeNode) -> Self {
        let mut root_node_children: Vec<ModuleTreeViewNode> = root_node
            .children
            .into_iter()
            .map(|(name, node)| ModuleTreeViewNode {
                tree_node: node,
                name,
                children: Default::default(),
            })
            .collect();

        for view_node in root_node_children.iter_mut() {
            populate_tree_view(view_node);
        }
        // Sort children
        root_node_children.sort_by(sort_tree_view_leaves);

        ModuleTreeView {
            children: root_node_children,
        }
    }
}

pub struct ModuleTreeViewNode {
    /// Backing node
    tree_node: ModuleTreeNode,
    /// Node name
    pub name: String,
    /// Direct descendants of this (sub)tree
    pub children: Vec<ModuleTreeViewNode>,
}

impl ModuleTreeViewNode {
    #[inline]
    pub fn new(name: String, tree_node: ModuleTreeNode) -> Self {
        ModuleTreeViewNode {
            tree_node,
            name,
            children: Default::default(),
        }
    }

    #[inline]
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    #[inline]
    pub fn path(&self) -> &ModulePath {
        &self.tree_node.path
    }

    #[inline]
    pub fn module_info(&self) -> Option<ModuleInfo> {
        self.tree_node.module_info
    }
}

pub fn populate_tree_view(view_node: &mut ModuleTreeViewNode) {
    let tree_node_children = std::mem::take(&mut view_node.tree_node.children);
    match tree_node_children.len() {
        0 => {
            // Nothing to do
        }
        1 => {
            // Merge with unique child, if that child is not a leaf
            let (unique_child_name, unique_child_node) = tree_node_children
                .into_iter()
                .next()
                .expect("map should contain one element");

            let mut child_view_node = ModuleTreeViewNode::new(unique_child_name, unique_child_node);
            // Populate the child node
            populate_tree_view(&mut child_view_node);

            if child_view_node.is_leaf() {
                // Child is a leaf, keep it as a child
                view_node.children.push(child_view_node);
            } else {
                // Child isn't a leaf, merge with it
                view_node.tree_node = child_view_node.tree_node;
                view_node.name = format!(
                    "{}{}{}",
                    view_node.name, MODULE_PATH_SEPARATOR, child_view_node.name
                );
                view_node.children = child_view_node.children;
            }
        }
        _ => {
            // Merge children with their descendants
            for (child_name, child_node) in tree_node_children.into_iter() {
                let mut child_view_node = ModuleTreeViewNode::new(child_name, child_node);

                // Populate the child node
                populate_tree_view(&mut child_view_node);
                view_node.children.push(child_view_node);
            }
            // Sort children
            view_node.children.sort_by(sort_tree_view_leaves);
        }
    }
}

fn sort_tree_view_leaves(lhs: &ModuleTreeViewNode, rhs: &ModuleTreeViewNode) -> std::cmp::Ordering {
    if lhs.is_leaf() == rhs.is_leaf() {
        // Compare names when both nodes are leaves or inner nodes
        lhs.name.cmp(&rhs.name)
    } else {
        // Else, put inner nodes before leaves
        if lhs.is_leaf() {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Less
        }
    }
}
