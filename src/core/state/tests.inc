#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::graph::PortDirection;

    #[test]
    fn test_graph_state_node_operations() {
        let mut state = GraphState::default();

        let node = Node::new(NodeId::new(1), "Test Node".to_string());
        state.add_node(node);

        assert!(state.get_node(&NodeId::new(1)).is_some());
        assert!(state.meters.contains_key(&NodeId::new(1)));
        assert!(state.volumes.contains_key(&NodeId::new(1)));

        state.remove_node(&NodeId::new(1));
        assert!(state.get_node(&NodeId::new(1)).is_none());
        assert!(!state.meters.contains_key(&NodeId::new(1)));
    }

    #[test]
    fn test_graph_state_port_operations() {
        let mut state = GraphState::default();

        let node = Node::new(NodeId::new(1), "Test Node".to_string());
        state.add_node(node);

        let port = Port::new(
            PortId::new(10),
            NodeId::new(1),
            "Test Port".to_string(),
            PortDirection::Output,
        );
        state.add_port(port);

        assert!(state.get_port(&PortId::new(10)).is_some());
        assert!(state
            .get_node(&NodeId::new(1))
            .unwrap()
            .port_ids
            .contains(&PortId::new(10)));

        state.remove_port(&PortId::new(10));
        assert!(state.get_port(&PortId::new(10)).is_none());
    }

    #[test]
    fn test_graph_state_link_removal_on_node_removal() {
        let mut state = GraphState::default();

        state.add_node(Node::new(NodeId::new(1), "Node 1".to_string()));
        state.add_node(Node::new(NodeId::new(2), "Node 2".to_string()));

        state.add_port(Port::new(
            PortId::new(10),
            NodeId::new(1),
            "Out".to_string(),
            PortDirection::Output,
        ));
        state.add_port(Port::new(
            PortId::new(20),
            NodeId::new(2),
            "In".to_string(),
            PortDirection::Input,
        ));

        let link = Link::new(
            LinkId::new(100),
            PortId::new(10),
            PortId::new(20),
            NodeId::new(1),
            NodeId::new(2),
        );
        state.add_link(link);

        assert_eq!(state.links.len(), 1);

        state.remove_node(&NodeId::new(1));
        assert_eq!(state.links.len(), 0);
    }

    #[test]
    fn test_ui_state_selection() {
        let mut state = UiState::default();

        state.select_node(NodeId::new(1));
        assert!(state.selected_nodes.contains(&NodeId::new(1)));
        assert_eq!(state.selected_nodes.len(), 1);

        state.add_to_selection(NodeId::new(2));
        assert!(state.selected_nodes.contains(&NodeId::new(2)));
        assert_eq!(state.selected_nodes.len(), 2);

        state.toggle_selection(NodeId::new(1));
        assert!(!state.selected_nodes.contains(&NodeId::new(1)));
        assert_eq!(state.selected_nodes.len(), 1);

        state.clear_selection();
        assert_eq!(state.selected_nodes.len(), 0);
    }

    #[test]
    fn test_ui_state_positions() {
        let mut state = UiState::default();

        state.set_node_position(NodeId::new(1), Position::new(100.0, 200.0));
        let pos = state.get_node_position(&NodeId::new(1));
        assert_eq!(pos.x, 100.0);
        assert_eq!(pos.y, 200.0);

        let default_pos = state.get_node_position(&NodeId::new(99));
        assert_eq!(default_pos.x, 0.0);
        assert_eq!(default_pos.y, 0.0);
    }

    #[test]
    fn test_connection_state() {
        assert!(!ConnectionState::Disconnected.is_connected());
        assert!(!ConnectionState::Connecting.is_connected());
        assert!(ConnectionState::Connected.is_connected());
        assert!(!ConnectionState::Error.is_connected());
    }

    #[test]
    fn test_persistent_positions_serialization() {
        use crate::util::id::NodeIdentifier;
        use crate::util::spatial::Position;

        let mut ui_state = UiState::default();
        ui_state.persistent_positions.insert(
            NodeIdentifier::new(
                "test-node".to_string(),
                Some("TestApp".to_string()),
                Some("Audio/Sink".to_string()),
            ),
            Position::new(100.0, 200.0),
        );

        let json =
            serde_json::to_string_pretty(&ui_state).expect("UiState serialization should not fail");
        let deserialized: UiState =
            serde_json::from_str(&json).expect("UiState deserialization should not fail");
        assert_eq!(deserialized.persistent_positions.len(), 1);
    }

    /// Integration test: simulates a full node lifecycle (add, configure, remove).
    #[test]
    fn test_node_lifecycle_with_ports_and_links() {
        let mut app_state = AppState::default();
        let graph = &mut app_state.graph;

        // Add two nodes with ports
        let mut src = Node::new(NodeId::new(1), "Firefox".to_string());
        src.media_class = Some(crate::domain::graph::MediaClass::StreamOutputAudio);
        src.application_name = Some("Firefox".to_string());
        graph.add_node(src);

        let mut sink = Node::new(NodeId::new(2), "Speakers".to_string());
        sink.media_class = Some(crate::domain::graph::MediaClass::AudioSink);
        graph.add_node(sink);

        // Add ports
        graph.add_port(Port::new(
            PortId::new(10),
            NodeId::new(1),
            "output_FL".to_string(),
            PortDirection::Output,
        ));
        graph.add_port(Port::new(
            PortId::new(11),
            NodeId::new(1),
            "output_FR".to_string(),
            PortDirection::Output,
        ));
        graph.add_port(Port::new(
            PortId::new(20),
            NodeId::new(2),
            "input_FL".to_string(),
            PortDirection::Input,
        ));
        graph.add_port(Port::new(
            PortId::new(21),
            NodeId::new(2),
            "input_FR".to_string(),
            PortDirection::Input,
        ));

        // Verify port assignment to nodes
        assert_eq!(graph.get_node(&NodeId::new(1)).unwrap().port_ids.len(), 2);
        assert_eq!(graph.get_node(&NodeId::new(2)).unwrap().port_ids.len(), 2);

        // Create links
        graph.add_link(Link::new(
            LinkId::new(100),
            PortId::new(10),
            PortId::new(20),
            NodeId::new(1),
            NodeId::new(2),
        ));
        graph.add_link(Link::new(
            LinkId::new(101),
            PortId::new(11),
            PortId::new(21),
            NodeId::new(1),
            NodeId::new(2),
        ));
        assert_eq!(graph.links.len(), 2);
        assert_eq!(graph.links_for_node(&NodeId::new(1)).len(), 2);

        // Remove source node - should cascade delete ports and links
        graph.remove_node(&NodeId::new(1));
        assert!(graph.get_node(&NodeId::new(1)).is_none());
        assert_eq!(
            graph.links.len(),
            0,
            "Links should be removed when node is removed"
        );
        assert!(
            graph.get_port(&PortId::new(10)).is_none(),
            "Ports should be removed when node is removed"
        );

        // Sink node should still exist with its ports
        assert!(graph.get_node(&NodeId::new(2)).is_some());
        assert_eq!(graph.get_node(&NodeId::new(2)).unwrap().port_ids.len(), 2);
    }

    /// Integration test: simulates UI state persistence across "restarts".
    #[test]
    fn test_persistent_state_restoration() {
        use crate::util::spatial::Position;

        let mut ui = UiState::default();
        let identifier = NodeIdentifier::new(
            "SuperCollider".to_string(),
            Some("scide".to_string()),
            Some("Audio Output".to_string()),
        );

        // Simulate first session: user positions a node and marks it uninteresting
        let node_id = NodeId::new(42);
        ui.update_position(node_id, &identifier, Position::new(300.0, 400.0));
        ui.update_uninteresting(node_id, &identifier, true);
        ui.set_custom_name(node_id, &identifier, "My SuperCollider".to_string());

        // Simulate restart: runtime state is cleared
        ui.node_positions.clear();
        ui.uninteresting_nodes.clear();
        ui.custom_names.clear();

        // New node appears with different ID but same identifier
        let new_node_id = NodeId::new(99);
        let restored_pos = ui.restore_position_for_node(new_node_id, &identifier);
        let restored_uninteresting = ui.restore_uninteresting_for_node(new_node_id, &identifier);
        let restored_name = ui.restore_custom_name_for_node(new_node_id, &identifier);

        assert!(
            restored_pos,
            "Position should be restored from persistent state"
        );
        assert!(
            restored_uninteresting,
            "Uninteresting status should be restored"
        );
        assert!(restored_name, "Custom name should be restored");

        assert_eq!(
            ui.get_node_position(&new_node_id),
            Position::new(300.0, 400.0)
        );
        assert!(ui.is_uninteresting(&new_node_id));
        assert_eq!(ui.get_custom_name(&new_node_id), Some("My SuperCollider"));
    }

    /// Integration test: volume state persistence.
    #[test]
    fn test_volume_persistence() {
        use crate::domain::audio::VolumeControl;

        let mut ui = UiState::default();
        let identifier = NodeIdentifier::new(
            "Firefox".to_string(),
            Some("Firefox".to_string()),
            Some("Audio Output".to_string()),
        );

        // Set volume and persist it
        let vol = VolumeControl {
            master: 0.75,
            muted: true,
            channels: vec![0.7, 0.8],
            step: 0.01,
        };
        ui.persist_volume(&identifier, vol.clone());

        // Restore for a new node
        let restored = ui.restore_volume_for_node(&identifier);
        assert!(restored.is_some());
        let restored = restored.unwrap();
        assert_eq!(restored.master, 0.75);
        assert!(restored.muted);
        assert_eq!(restored.channels, vec![0.7, 0.8]);
    }

    /// Test: node cleanup preserves persistent state.
    #[test]
    fn test_cleanup_preserves_persistent_state() {
        use crate::util::spatial::Position;

        let mut ui = UiState::default();
        let node_id = NodeId::new(1);
        let identifier = NodeIdentifier::new("node".to_string(), None, None);

        // Set up both runtime and persistent state
        ui.update_position(node_id, &identifier, Position::new(100.0, 200.0));
        ui.update_uninteresting(node_id, &identifier, true);
        ui.set_custom_name(node_id, &identifier, "custom".to_string());

        // Cleanup removes runtime state only
        ui.cleanup_removed_node(&node_id);

        // Runtime state gone
        assert!(!ui.node_positions.contains_key(&node_id));
        assert!(!ui.uninteresting_nodes.contains(&node_id));
        assert!(!ui.custom_names.contains_key(&node_id));

        // Persistent state preserved
        assert!(ui.persistent_positions.contains_key(&identifier));
        assert!(ui.persistent_uninteresting.contains(&identifier));
        assert!(ui.persistent_custom_names.contains_key(&identifier));
    }

    /// Test: animation lifecycle.
    #[test]
    fn test_animation_lifecycle() {
        use crate::util::spatial::Position;

        let mut ui = UiState::default();
        let node_id = NodeId::new(1);

        // Set an initial position so the animation has a start point
        ui.set_node_position(node_id, Position::new(10.0, 10.0));

        // Start animation to a distant target
        ui.animate_to_position(node_id, Position::new(500.0, 300.0), false);
        assert!(!ui.position_animations.is_empty());

        // Run enough frames to complete (~200ms at 60fps = 12 frames)
        for _ in 0..20 {
            ui.update_animations(1.0 / 60.0);
        }

        // Animation should be complete
        assert!(ui.position_animations.is_empty());
        let final_pos = ui.get_node_position(&node_id);
        assert!((final_pos.x - 500.0).abs() < 1.0);
        assert!((final_pos.y - 300.0).abs() < 1.0);
    }

    /// Test: layer visibility filtering.
    #[test]
    fn test_layer_visibility() {
        use crate::domain::graph::NodeLayer;

        let mut vis = LayerVisibility::default();
        assert!(vis.is_visible(NodeLayer::Hardware));
        assert!(vis.is_visible(NodeLayer::Pipewire));
        assert!(vis.is_visible(NodeLayer::Session));

        vis.toggle(NodeLayer::Pipewire);
        assert!(!vis.is_visible(NodeLayer::Pipewire));
        assert!(vis.is_visible(NodeLayer::Hardware));

        vis.toggle(NodeLayer::Pipewire);
        assert!(vis.is_visible(NodeLayer::Pipewire));
    }

    /// Test: GraphState port removal cascades to links.
    #[test]
    fn test_port_removal_cascades_to_links() {
        let mut graph = GraphState::default();

        graph.add_node(Node::new(NodeId::new(1), "Node1".to_string()));
        graph.add_node(Node::new(NodeId::new(2), "Node2".to_string()));
        graph.add_port(Port::new(
            PortId::new(10),
            NodeId::new(1),
            "out".to_string(),
            PortDirection::Output,
        ));
        graph.add_port(Port::new(
            PortId::new(20),
            NodeId::new(2),
            "in".to_string(),
            PortDirection::Input,
        ));
        graph.add_link(Link::new(
            LinkId::new(100),
            PortId::new(10),
            PortId::new(20),
            NodeId::new(1),
            NodeId::new(2),
        ));

        assert_eq!(graph.links.len(), 1);
        graph.remove_port(&PortId::new(10));
        assert_eq!(
            graph.links.len(),
            0,
            "Link should be removed when port is removed"
        );
        // Link meters should also be cleaned up
        assert!(!graph.link_meters.contains_key(&LinkId::new(100)));
    }

    /// Test: graph clear resets everything.
    #[test]
    fn test_graph_clear() {
        let mut graph = GraphState::default();
        graph.add_node(Node::new(NodeId::new(1), "N".to_string()));
        graph.add_port(Port::new(
            PortId::new(10),
            NodeId::new(1),
            "p".to_string(),
            PortDirection::Output,
        ));

        assert!(!graph.nodes.is_empty());
        graph.clear();

        assert!(graph.nodes.is_empty());
        assert!(graph.ports.is_empty());
        assert!(graph.links.is_empty());
        assert!(graph.meters.is_empty());
        assert!(graph.volumes.is_empty());
    }

    /// Stress test: 500-node graph operations should not have quadratic blowup.
    #[test]
    fn test_500_node_graph_operations() {
        let mut graph = GraphState::default();
        let start = std::time::Instant::now();

        // Add 500 nodes with 2 ports each
        for i in 0..500u32 {
            let node = Node::new(NodeId::new(i), format!("Node_{}", i));
            graph.add_node(node);
            let out_port = Port::new(
                PortId::new(i * 10),
                NodeId::new(i),
                "out".into(),
                PortDirection::Output,
            );
            let in_port = Port::new(
                PortId::new(i * 10 + 1),
                NodeId::new(i),
                "in".into(),
                PortDirection::Input,
            );
            graph.add_port(out_port);
            graph.add_port(in_port);
        }

        // Create a chain of links: node[0] -> node[1] -> ... -> node[499]
        for i in 0..499u32 {
            let link = Link::new(
                LinkId::new(i + 10000),
                PortId::new(i * 10),
                PortId::new((i + 1) * 10 + 1),
                NodeId::new(i),
                NodeId::new(i + 1),
            );
            graph.add_link(link);
        }

        assert_eq!(graph.nodes.len(), 500);
        assert_eq!(graph.ports.len(), 1000);
        assert_eq!(graph.links.len(), 499);

        // Query operations
        let ports = graph.ports_for_node(&NodeId::new(250));
        assert_eq!(ports.len(), 2);
        let links = graph.links_for_node(&NodeId::new(250));
        assert_eq!(links.len(), 2); // one incoming, one outgoing

        // Remove a node in the middle - should cascade
        graph.remove_node(&NodeId::new(250));
        assert!(graph.get_node(&NodeId::new(250)).is_none());
        assert_eq!(graph.nodes.len(), 499);
        assert_eq!(graph.links.len(), 497); // 2 links removed

        let elapsed = start.elapsed();
        assert!(
            elapsed.as_secs() < 2,
            "500-node operations should complete in <2s, took {:?}",
            elapsed
        );
    }

    /// Test: concurrent SharedState access (basic soundness).
    #[test]
    fn test_shared_state_concurrent_access() {
        let state = create_shared_state();
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let s = state.clone();
                std::thread::spawn(move || {
                    // Writer: add a node
                    {
                        let mut write = s.write();
                        write
                            .graph
                            .add_node(Node::new(NodeId::new(i), format!("thread_{}", i)));
                    }
                    // Reader: count nodes
                    {
                        let read = s.read();
                        let _ = read.graph.nodes.len();
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("Thread should not panic");
        }

        let state = state.read();
        assert_eq!(state.graph.nodes.len(), 10);
    }

    /// Test: UiState snapshot save/restore roundtrip.
    #[test]
    fn test_ui_state_serialization_roundtrip() {
        use crate::util::spatial::Position;

        let mut ui = UiState::default();
        let id = NodeIdentifier::new("node".into(), Some("app".into()), None);
        ui.persistent_positions
            .insert(id.clone(), Position::new(42.0, 99.0));
        ui.persistent_uninteresting.insert(id.clone());
        ui.persistent_custom_names.insert(id, "My Name".to_string());

        let json = serde_json::to_string(&ui).expect("Serialization should not fail");
        let restored: UiState =
            serde_json::from_str(&json).expect("Deserialization should not fail");

        assert_eq!(ui.persistent_positions, restored.persistent_positions);
        assert_eq!(
            ui.persistent_uninteresting,
            restored.persistent_uninteresting
        );
        assert_eq!(ui.persistent_custom_names, restored.persistent_custom_names);
    }

    /// Test: add_port to nonexistent node should not panic.
    #[test]
    fn test_add_port_to_missing_node() {
        let mut graph = GraphState::default();
        let port = Port::new(
            PortId::new(10),
            NodeId::new(999),
            "orphan".into(),
            PortDirection::Input,
        );
        graph.add_port(port);
        // Port is still stored, just not linked to any node
        assert!(graph.get_port(&PortId::new(10)).is_some());
    }

    /// Test: remove nonexistent entities should not panic.
    #[test]
    fn test_remove_nonexistent_entities() {
        let mut graph = GraphState::default();
        assert!(graph.remove_node(&NodeId::new(999)).is_none());
        assert!(graph.remove_port(&PortId::new(999)).is_none());
        assert!(graph.remove_link(&LinkId::new(999)).is_none());
    }

    /// Test: link meters are cleaned up on link removal.
    #[test]
    fn test_link_meters_cleaned_on_removal() {
        let mut graph = GraphState::default();
        graph.add_node(Node::new(NodeId::new(1), "A".into()));
        graph.add_node(Node::new(NodeId::new(2), "B".into()));
        graph.add_link(Link::new(
            LinkId::new(1),
            PortId::new(1),
            PortId::new(2),
            NodeId::new(1),
            NodeId::new(2),
        ));
        assert!(graph.link_meters.contains_key(&LinkId::new(1)));
        graph.remove_link(&LinkId::new(1));
        assert!(!graph.link_meters.contains_key(&LinkId::new(1)));
    }
}

#[cfg(test)]
mod proptests {
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
}
