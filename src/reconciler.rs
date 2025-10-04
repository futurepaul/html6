/// Keyed widget reconciliation for incremental UI updates
///
/// This module implements a React-like reconciliation algorithm that:
/// - Assigns stable keys to widgets based on AST node type and identity
/// - Diffs old vs new widget trees
/// - Reuses unchanged widgets (preserves input focus!)
/// - Only rebuilds changed subtrees

use crate::parser::ast::Node;

/// Track widget state for reconciliation using generational indices
#[derive(Clone)]
pub struct WidgetState {
    /// The AST node this widget was built from
    pub node: Node,

    /// Generation number (increments when this slot is reused)
    pub generation: u32,
}

impl WidgetState {
    pub fn new(node: Node, generation: u32) -> Self {
        Self { node, generation }
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

/// Reconcile using Xilem-style generational arena (pure positional)
/// Returns new arena and operations to apply
pub fn reconcile_arena(
    arena: &WidgetArena,
    new_nodes: &[Node],
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
            if nodes_equal(&old_state.node, new_node) {
                // Same node - keep it
                new_states.push(old_state.clone());
                ops.push(ReconcileOp::Keep);
            } else {
                // Different node - increment generation and rebuild
                generations[idx] += 1;
                new_states.push(WidgetState::new(new_node.clone(), generations[idx]));
                ops.push(ReconcileOp::Rebuild);
            }
        } else {
            // New position (list grew)
            new_states.push(WidgetState::new(new_node.clone(), generations[idx]));
            ops.push(ReconcileOp::Add);
        }
    }

    let new_arena = WidgetArena {
        states: new_states,
        generations,
    };

    (new_arena, ops)
}

/// Legacy reconcile for compatibility
pub fn reconcile_nodes(
    old: &[WidgetState],
    new_nodes: &[Node],
    _parent_path: &str,
) -> (Vec<WidgetState>, Vec<ReconcileOp>) {
    let arena = WidgetArena {
        states: old.to_vec(),
        generations: vec![0; old.len().max(new_nodes.len())],
    };
    let (new_arena, ops) = reconcile_arena(&arena, new_nodes);
    (new_arena.states, ops)
}

/// Build a fresh widget tree from nodes and return the states
pub fn build_widget_tree(nodes: &[Node], _parent_path: &str) -> Vec<WidgetState> {
    nodes
        .iter()
        .map(|node| WidgetState::new(node.clone(), 0))
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
    fn test_generation_tracking() {
        let old_nodes = vec![Node::text("A"), Node::text("B")];
        let arena = WidgetArena::from_nodes(&old_nodes);

        assert_eq!(arena.states.len(), 2);
        assert_eq!(arena.generations, vec![0, 0]);

        // Change second node
        let new_nodes = vec![Node::text("A"), Node::text("C")];
        let (new_arena, ops) = reconcile_arena(&arena, &new_nodes);

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
        let (new_arena, ops) = reconcile_arena(&arena, &new_nodes);

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
