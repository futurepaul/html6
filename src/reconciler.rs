/// Keyed widget reconciliation for incremental UI updates
///
/// This module implements a React-like reconciliation algorithm that:
/// - Assigns stable keys to widgets based on AST node type and identity
/// - Diffs old vs new widget trees
/// - Reuses unchanged widgets (preserves input focus!)
/// - Only rebuilds changed subtrees

use crate::parser::ast::Node;
use crate::renderer::RenderContext;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Track widget state for reconciliation using generational indices
#[derive(Clone)]
pub struct WidgetState {
    /// The AST node this widget was built from
    pub node: Node,

    /// Generation number (increments when this slot is reused)
    pub generation: u32,

    /// Hash of evaluated expression value (for Expr nodes)
    /// This allows detecting when expression output changes even if expression itself doesn't
    pub expr_value_hash: Option<u64>,
}

impl WidgetState {
    pub fn new(node: Node, generation: u32) -> Self {
        Self {
            node,
            generation,
            expr_value_hash: None,
        }
    }

    pub fn new_with_expr_hash(node: Node, generation: u32, expr_value_hash: Option<u64>) -> Self {
        Self {
            node,
            generation,
            expr_value_hash,
        }
    }
}

/// Generational arena for tracking widgets across rebuilds
pub struct WidgetArena {
    /// States at each index, with generation tracking
    pub states: Vec<WidgetState>,

    /// Generations for each slot (grows but never shrinks)
    pub generations: Vec<u32>,
}

impl WidgetArena {
    pub fn new() -> Self {
        Self {
            states: Vec::new(),
            generations: Vec::new(),
        }
    }

    pub fn from_nodes(nodes: &[Node]) -> Self {
        let states = nodes.iter().map(|node| WidgetState::new(node.clone(), 0)).collect();
        let generations = vec![0; nodes.len()];
        Self { states, generations }
    }
}

/// Result of reconciliation with rebuild instructions
#[derive(Debug, Clone, Copy)]
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

/// Reconcile using Xilem-style generational arena with expression value tracking
/// Returns new arena and operations to apply
pub fn reconcile_arena(
    arena: &WidgetArena,
    new_nodes: &[Node],
    ctx: &mut Option<RenderContext>,
) -> (WidgetArena, Vec<ReconcileOp>) {
    let mut new_states = Vec::new();
    let mut ops = Vec::new();

    // Ensure generations vec is large enough (never shrinks)
    let mut generations = arena.generations.clone();
    if generations.len() < new_nodes.len() {
        generations.resize(new_nodes.len(), 0);
    }

    // Process each position - pure Xilem approach
    for (idx, new_node) in new_nodes.iter().enumerate() {
        let old_state = arena.states.get(idx);

        if let Some(old_state) = old_state {
            // There was a widget at this position before
            // Create new state with expr hash computed
            let new_state = build_state_with_expr_hash(new_node, ctx, old_state.generation);

            if nodes_equal_with_state(old_state, &new_state) {
                // Same node - keep it
                new_states.push(old_state.clone());
                ops.push(ReconcileOp::Keep);
            } else {
                // Different node - increment generation and rebuild
                if matches!(new_node, Node::Expr { .. }) {
                    println!("  🔄 Expr changed: old_hash={:?}, new_hash={:?}",
                        old_state.expr_value_hash, new_state.expr_value_hash);
                }
                generations[idx] += 1;
                let rebuild_state = build_state_with_expr_hash(new_node, ctx, generations[idx]);
                new_states.push(rebuild_state);
                ops.push(ReconcileOp::Rebuild);
            }
        } else {
            // New position (list grew)
            let new_state = build_state_with_expr_hash(new_node, ctx, generations[idx]);
            new_states.push(new_state);
            ops.push(ReconcileOp::Add);
        }
    }

    let new_arena = WidgetArena {
        states: new_states,
        generations,
    };

    (new_arena, ops)
}

/// Build widget tree with expression value hashing for reactive updates
pub fn build_widget_tree(
    nodes: &[Node],
    ctx: &mut Option<RenderContext>,
) -> Vec<WidgetState> {
    nodes
        .iter()
        .map(|node| build_state_with_expr_hash(node, ctx, 0))
        .collect()
}

/// Recursively build widget state with expr_value_hash computed
fn build_state_with_expr_hash(
    node: &Node,
    ctx: &mut Option<RenderContext>,
    generation: u32,
) -> WidgetState {
    let expr_hash = if let Node::Expr { expression } = node {
        // Evaluate expression and hash the result
        if let Some(context) = ctx {
            match context.eval(expression) {
                Ok(value) => Some(hash_json_value(&value)),
                Err(_) => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    WidgetState::new_with_expr_hash(node.clone(), generation, expr_hash)
}

/// Check if two widget states are equal (considers expr_value_hash for Expr nodes)
fn nodes_equal_with_state(a: &WidgetState, b: &WidgetState) -> bool {
    // For Expr nodes, compare the evaluated value hash
    if matches!(a.node, Node::Expr { .. }) && matches!(b.node, Node::Expr { .. }) {
        return a.expr_value_hash == b.expr_value_hash;
    }

    // For nodes with children, we need to check if any contain Expr nodes that might have changed
    // For now, we'll rebuild any node that contains expressions to be safe
    if node_contains_expr(&a.node) || node_contains_expr(&b.node) {
        // If either node contains expressions, we need to rebuild to re-evaluate them
        // This is conservative but correct - we'll optimize later
        return false;
    }

    // For all other nodes, use structural equality
    nodes_equal(&a.node, &b.node)
}

/// Check if a node or its children contain any Expr nodes
fn node_contains_expr(node: &Node) -> bool {
    match node {
        Node::Expr { .. } => true,
        Node::Heading { children, .. } => children.iter().any(node_contains_expr),
        Node::Paragraph { children } => children.iter().any(node_contains_expr),
        Node::Strong { children } => children.iter().any(node_contains_expr),
        Node::Emphasis { children } => children.iter().any(node_contains_expr),
        Node::Link { children, .. } => children.iter().any(node_contains_expr),
        Node::List { items, .. } => items.iter().any(|item| item.children.iter().any(node_contains_expr)),
        Node::VStack { children, .. } => children.iter().any(node_contains_expr),
        Node::HStack { children, .. } => children.iter().any(node_contains_expr),
        Node::Grid { children, .. } => children.iter().any(node_contains_expr),
        Node::Each { children, .. } => children.iter().any(node_contains_expr),
        Node::If { children, else_children, .. } => {
            children.iter().any(node_contains_expr) ||
            else_children.as_ref().map(|ec| ec.iter().any(node_contains_expr)).unwrap_or(false)
        }
        Node::Button { children, .. } => children.iter().any(node_contains_expr),
        _ => false,
    }
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

/// Compute hash of a JSON value for expression comparison
fn hash_json_value(value: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Hash the JSON string representation (stable across runs)
    value.to_string().hash(&mut hasher);
    hasher.finish()
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::Node;

    #[test]
    fn test_generation_tracking() {
        let old_nodes = vec![Node::text("A"), Node::text("B")];
        let arena = WidgetArena::from_nodes(&old_nodes);

        assert_eq!(arena.states.len(), 2);
        assert_eq!(arena.generations, vec![0, 0]);

        // Change second node
        let new_nodes = vec![Node::text("A"), Node::text("C")];
        let (new_arena, ops) = reconcile_arena(&arena, &new_nodes, &mut None);

        assert_eq!(ops.len(), 2);
        assert!(matches!(ops[0], ReconcileOp::Keep));
        assert!(matches!(ops[1], ReconcileOp::Rebuild));

        // Generation incremented for changed node
        assert_eq!(new_arena.generations[0], 0); // Unchanged
        assert_eq!(new_arena.generations[1], 1); // Incremented
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
    fn test_positional_reconciliation() {
        let old_nodes = vec![Node::text("A"), Node::text("B"), Node::text("C")];
        let arena = WidgetArena::from_nodes(&old_nodes);

        // Insert a new node at position 1
        let new_nodes = vec![Node::text("A"), Node::text("NEW"), Node::text("B"), Node::text("C")];
        let (new_arena, ops) = reconcile_arena(&arena, &new_nodes, &mut None);

        assert_eq!(ops.len(), 4);
        // Position 0: "A" → "A" = Keep
        assert!(matches!(ops[0], ReconcileOp::Keep));
        // Position 1: "B" → "NEW" = Rebuild (different content at same position)
        assert!(matches!(ops[1], ReconcileOp::Rebuild));
        // Position 2: "C" → "B" = Rebuild
        assert!(matches!(ops[2], ReconcileOp::Rebuild));
        // Position 3: nothing → "C" = Add
        assert!(matches!(ops[3], ReconcileOp::Add));
    }
}
