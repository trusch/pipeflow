//! Performance benchmarks for graph operations.

use criterion::{criterion_group, criterion_main, Criterion, black_box};

use pipeflow::core::state::GraphState;
use pipeflow::domain::graph::{Link, Node, Port, PortDirection};
use pipeflow::util::id::{LinkId, NodeId, PortId};

/// Creates a graph with the given number of nodes, each with 2 ports,
/// and a chain of links connecting consecutive nodes.
fn create_graph(node_count: u32) -> GraphState {
    let mut graph = GraphState::default();
    for i in 0..node_count {
        graph.add_node(Node::new(NodeId::new(i), format!("Node_{}", i)));
        graph.add_port(Port::new(
            PortId::new(i * 10),
            NodeId::new(i),
            "out".into(),
            PortDirection::Output,
        ));
        graph.add_port(Port::new(
            PortId::new(i * 10 + 1),
            NodeId::new(i),
            "in".into(),
            PortDirection::Input,
        ));
    }
    for i in 0..node_count.saturating_sub(1) {
        graph.add_link(Link::new(
            LinkId::new(i + 10000),
            PortId::new(i * 10),
            PortId::new((i + 1) * 10 + 1),
            NodeId::new(i),
            NodeId::new(i + 1),
        ));
    }
    graph
}

fn bench_graph_add_200_nodes(c: &mut Criterion) {
    c.bench_function("add_200_nodes_with_ports", |b| {
        b.iter(|| {
            let graph = create_graph(black_box(200));
            black_box(graph.nodes.len());
        });
    });
}

fn bench_graph_query_ports_for_node(c: &mut Criterion) {
    let graph = create_graph(200);
    c.bench_function("query_ports_for_node_200", |b| {
        b.iter(|| {
            let ports = graph.ports_for_node(black_box(&NodeId::new(100)));
            black_box(ports.len());
        });
    });
}

fn bench_graph_query_links_for_node(c: &mut Criterion) {
    let graph = create_graph(200);
    c.bench_function("query_links_for_node_200", |b| {
        b.iter(|| {
            let links = graph.links_for_node(black_box(&NodeId::new(100)));
            black_box(links.len());
        });
    });
}

fn bench_graph_remove_node_cascade(c: &mut Criterion) {
    c.bench_function("remove_node_cascade_200", |b| {
        b.iter_with_setup(
            || create_graph(200),
            |mut graph| {
                graph.remove_node(black_box(&NodeId::new(100)));
                black_box(graph.nodes.len());
            },
        );
    });
}

fn bench_graph_clear_500(c: &mut Criterion) {
    c.bench_function("clear_500_node_graph", |b| {
        b.iter_with_setup(
            || create_graph(500),
            |mut graph| {
                graph.clear();
                black_box(graph.nodes.len());
            },
        );
    });
}

fn bench_filter_200_nodes(c: &mut Criterion) {
    use pipeflow::domain::filters::{FilterPredicate, FilterSet};
    use pipeflow::domain::graph::MediaClass;

    let graph = create_graph(200);
    // Set half the nodes as audio sinks
    let mut graph = graph;
    for (i, node) in graph.nodes.values_mut().enumerate() {
        if i % 2 == 0 {
            node.media_class = Some(MediaClass::AudioSink);
        }
    }

    let mut filters = FilterSet::new();
    filters.add_include(FilterPredicate::AudioOnly);

    c.bench_function("filter_200_nodes_audio_only", |b| {
        b.iter(|| {
            let count = graph.nodes.values()
                .filter(|n| filters.matches_with_ports(n, &graph.ports))
                .count();
            black_box(count);
        });
    });
}

criterion_group!(
    benches,
    bench_graph_add_200_nodes,
    bench_graph_query_ports_for_node,
    bench_graph_query_links_for_node,
    bench_graph_remove_node_cascade,
    bench_graph_clear_500,
    bench_filter_200_nodes,
);
criterion_main!(benches);
