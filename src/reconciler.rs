/// Keyed widget reconciliation for incremental UI updates
///
/// This module implements a React-like reconciliation algorithm that:
/// - Assigns stable keys to widgets based on AST node type and identity
/// - Diffs old vs new widget trees
/// - Reuses unchanged widgets (preserves input focus!)
/// - Only rebuilds changed subtrees

use crate::parser::ast::Node;
use crate::renderer;
use masonry::core::NewWidget;
use masonry::widgets::Flex;
use std::collections::HashMap;

/// Stable key for a widget, used to track identity across rebuilds
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum WidgetKey {
    /// Static key for non-dynamic content
    Static(String),

    /// Input field by name (preserves focus!)
    Input(String),

    /// List item by source and index
    /// Example: ListItem("queries.feed", 0)
    ListItem(String, usize),

    /// Each iteration instance
    /// Example: Each("queries.feed", 0)
    Each(String, usize),

    /// Component by path
    /// Example: Component("vstack.0.hstack.1")
    Component(String),
}

impl WidgetKey {
    /// Derive a stable key from an AST node and its position in the tree
    pub fn from_node(node: &Node, path: &str, index: usize) -> Self {
        match node {
            Node::Input { name, .. } => {
                // Inputs keyed by name - preserves focus when content updates!
                WidgetKey::Input(name.clone())
            }

            Node::Each { from, .. } => {
                // Each loops keyed by data source and iteration index
                WidgetKey::Each(from.clone(), index)
            }

            Node::Expr { expression } => {
                // Expressions keyed by the expression itself
                WidgetKey::Static(format!("expr:{}", expression))
            }

            Node::Button { .. } => {
                // Buttons keyed by position (could be improved with on_click)
                WidgetKey::Component(format!("{}:button:{}", path, index))
            }

            Node::VStack { .. } | Node::HStack { .. } | Node::Grid { .. } => {
                // Layout containers keyed by position
                WidgetKey::Component(format!("{}:{}", path, index))
            }

            Node::Heading { level, .. } => {
                // Headings keyed by level and position
                WidgetKey::Static(format!("h{}:{}", level, index))
            }

            _ => {
                // Other nodes keyed by position
                WidgetKey::Component(format!("{}:{}", path, index))
            }
        }
    }
}

/// Track widget state for reconciliation (without storing actual widgets)
#[derive(Clone)]
pub struct WidgetState {
    /// Stable key for tracking across rebuilds
    pub key: WidgetKey,

    /// The AST node this widget was built from
    pub node: Node,

    /// Path in the tree (for deriving child keys)
    pub path: String,
}

impl WidgetState {
    pub fn new(key: WidgetKey, node: Node, path: String) -> Self {
        Self { key, node, path }
    }
}

/// Result of reconciliation with rebuild instructions
pub enum ReconcileOp {
    /// Keep existing widget at this index (no rebuild needed)
    Keep,
    /// Rebuild widget at this index (content changed)
    Rebuild,
    /// Add new widget
    Add,
    /// Remove widget at this index
    Remove,
}

/// Reconcile old and new AST nodes, returning which operations to perform
pub fn reconcile_nodes(
    old: &[WidgetState],
    new_nodes: &[Node],
    parent_path: &str,
) -> (Vec<WidgetState>, Vec<ReconcileOp>) {
    let mut new_states = Vec::new();
    let mut ops = Vec::new();
    let mut old_by_key: HashMap<WidgetKey, &WidgetState> = old
        .iter()
        .map(|w| (w.key.clone(), w))
        .collect();

    for (index, new_node) in new_nodes.iter().enumerate() {
        let path = if parent_path.is_empty() {
            index.to_string()
        } else {
            format!("{}.{}", parent_path, index)
        };

        let new_key = WidgetKey::from_node(new_node, parent_path, index);

        // Try to find matching old widget by key
        if let Some(old_state) = old_by_key.remove(&new_key) {
            // Found a widget with the same key
            if nodes_equal(&old_state.node, new_node) {
                // Node unchanged - keep widget
                new_states.push(old_state.clone());
                ops.push(ReconcileOp::Keep);
            } else {
                // Node changed - rebuild
                new_states.push(WidgetState::new(new_key, new_node.clone(), path));
                ops.push(ReconcileOp::Rebuild);
            }
        } else {
            // New widget - add it
            new_states.push(WidgetState::new(new_key, new_node.clone(), path));
            ops.push(ReconcileOp::Add);
        }
    }

    // Widgets remaining in old_by_key were removed
    for _ in old_by_key.values() {
        ops.push(ReconcileOp::Remove);
    }

    (new_states, ops)
}

/// Build a fresh widget tree from nodes and return the states
pub fn build_widget_tree(nodes: &[Node], parent_path: &str) -> Vec<WidgetState> {
    nodes
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let path = if parent_path.is_empty() {
                index.to_string()
            } else {
                format!("{}.{}", parent_path, index)
            };
            let key = WidgetKey::from_node(node, parent_path, index);
            WidgetState::new(key, node.clone(), path)
        })
        .collect()
}

/// Check if two AST nodes are semantically equal
/// This determines whether we can reuse a widget or need to rebuild
fn nodes_equal(a: &Node, b: &Node) -> bool {
    use Node::*;

    match (a, b) {
        (Text { value: v1 }, Text { value: v2 }) => v1 == v2,

        (Heading { level: l1, children: c1 }, Heading { level: l2, children: c2 }) => {
            l1 == l2 && children_equal(c1, c2)
        }

        (Paragraph { children: c1 }, Paragraph { children: c2 }) => children_equal(c1, c2),

        (Strong { children: c1 }, Strong { children: c2 }) => children_equal(c1, c2),

        (Emphasis { children: c1 }, Emphasis { children: c2 }) => children_equal(c1, c2),

        (Link { url: u1, children: c1 }, Link { url: u2, children: c2 }) => {
            u1 == u2 && children_equal(c1, c2)
        }

        (Image { src: s1, alt: a1 }, Image { src: s2, alt: a2 }) => s1 == s2 && a1 == a2,

        (List { ordered: o1, items: i1 }, List { ordered: o2, items: i2 }) => {
            o1 == o2 && i1.len() == i2.len() && i1.iter().zip(i2).all(|(a, b)| children_equal(&a.children, &b.children))
        }

        (Expr { expression: e1 }, Expr { expression: e2 }) => e1 == e2,

        (Button { on_click: oc1, children: c1 }, Button { on_click: oc2, children: c2 }) => {
            oc1 == oc2 && children_equal(c1, c2)
        }

        (Input { name: n1, placeholder: p1 }, Input { name: n2, placeholder: p2 }) => {
            n1 == n2 && p1 == p2
        }

        (VStack { children: c1, flex: f1, .. }, VStack { children: c2, flex: f2, .. }) => {
            f1 == f2 && children_equal(c1, c2)
        }

        (HStack { children: c1, flex: f1, .. }, HStack { children: c2, flex: f2, .. }) => {
            f1 == f2 && children_equal(c1, c2)
        }

        (Grid { children: c1, columns: col1 }, Grid { children: c2, columns: col2 }) => {
            col1 == col2 && children_equal(c1, c2)
        }

        (Spacer { size: s1 }, Spacer { size: s2 }) => s1 == s2,

        (Each { from: f1, as_name: a1, children: c1 }, Each { from: f2, as_name: a2, children: c2 }) => {
            f1 == f2 && a1 == a2 && children_equal(c1, c2)
        }

        (If { value: v1, children: c1, else_children: e1 }, If { value: v2, children: c2, else_children: e2 }) => {
            v1 == v2 && children_equal(c1, c2) &&
            match (e1, e2) {
                (Some(ec1), Some(ec2)) => children_equal(ec1, ec2),
                (None, None) => true,
                _ => false,
            }
        }

        _ => false, // Different node types
    }
}

fn children_equal(a: &[Node], b: &[Node]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| nodes_equal(a, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::Node;

    #[test]
    fn test_key_derivation() {
        let input = Node::input("message");
        let key = WidgetKey::from_node(&input, "", 0);
        assert_eq!(key, WidgetKey::Input("message".to_string()));

        let button = Node::button(None, vec![Node::text("Click")]);
        let key = WidgetKey::from_node(&button, "root", 5);
        assert!(matches!(key, WidgetKey::Component(_)));
    }

    #[test]
    fn test_nodes_equal() {
        let text1 = Node::text("hello");
        let text2 = Node::text("hello");
        let text3 = Node::text("world");

        assert!(nodes_equal(&text1, &text2));
        assert!(!nodes_equal(&text1, &text3));

        let heading1 = Node::heading(1, vec![Node::text("Title")]);
        let heading2 = Node::heading(1, vec![Node::text("Title")]);
        let heading3 = Node::heading(2, vec![Node::text("Title")]);

        assert!(nodes_equal(&heading1, &heading2));
        assert!(!nodes_equal(&heading1, &heading3));
    }

    #[test]
    fn test_reconciliation_ops() {
        // Create old tree state - use Inputs so keys are unique
        let old_states = build_widget_tree(&vec![
            Node::input("field1"),
            Node::input("field2"),
            Node::input("field3"),
        ], "");

        // New tree with changes
        let new_nodes = vec![
            Node::input("field1"),      // Same - Keep
            Node::input("field2_new"),  // Different key - Add (field2 removed)
            Node::input("field4"),      // New - Add
        ];

        let (new_states, ops) = reconcile_nodes(&old_states, &new_nodes, "");

        assert_eq!(new_states.len(), 3);
        assert_eq!(ops.len(), 5); // 1 Keep, 2 Add, 2 Remove

        // First input unchanged (same key and content) - should Keep
        assert!(matches!(ops[0], ReconcileOp::Keep));
        assert_eq!(new_states[0].node, old_states[0].node);

        // Second input has different key - should Add
        assert!(matches!(ops[1], ReconcileOp::Add));

        // Third input is new - should Add
        assert!(matches!(ops[2], ReconcileOp::Add));

        // Old field2 and field3 removed
        assert!(matches!(ops[3], ReconcileOp::Remove));
        assert!(matches!(ops[4], ReconcileOp::Remove));
    }
}
