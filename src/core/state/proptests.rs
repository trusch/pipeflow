use super::*;
use crate::domain::graph::Node;
use proptest::prelude::*;

proptest! {
    /// Adding and removing random sequences of nodes should never panic
    /// and leave consistent state.
    #[test]
    fn graph_add_remove_never_panics(
        ops in proptest::collection::vec(
            (1u32..100, proptest::bool::ANY),
            1..50,
        )
    ) {
        let mut graph = GraphState::default();
        for (id, is_add) in &ops {
            if *is_add {
                graph.add_node(Node::new(NodeId::new(*id), format!("N{}", id)));
            } else {
                graph.remove_node(&NodeId::new(*id));
            }
        }
        // Consistency: link meters should exist for every link
        for link in graph.links.values() {
            // Link meters should exist for every link
            assert!(graph.link_meters.contains_key(&link.id));
        }
    }

    /// Layer visibility toggle is always reversible.
    #[test]
    fn layer_visibility_toggle_reversible(
        hw in proptest::bool::ANY,
        pw in proptest::bool::ANY,
        sm in proptest::bool::ANY,
    ) {
        use crate::domain::graph::NodeLayer;
        let mut vis = LayerVisibility { hardware: hw, pipewire: pw, session: sm };
        let original = vis;

        vis.toggle(NodeLayer::Hardware);
        vis.toggle(NodeLayer::Hardware);
        assert_eq!(vis, original);
    }
}
