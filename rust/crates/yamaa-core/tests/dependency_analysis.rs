use yamaa_core::dependency_analysis::{analyze, Analysis, Error, Limits};

/// Read hand-authored declaration indices; this fixture notation is not a language grammar.
fn indices(text: &str) -> Vec<usize> {
    if text == "-" {
        vec![]
    } else {
        text.split(',').map(|n| n.parse().unwrap()).collect()
    }
}

/// Independent truth distinguishes first reachable cycles from globally earliest cycles.
#[test]
fn shared_dependency_graph_truth() {
    for line in include_str!("fixtures/dependency_analysis.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split('\t').collect();
        let mut graph = vec![vec![]; fields[1].parse().unwrap()];
        if fields[2] != "-" {
            for edges in fields[2].split(';') {
                let (node, dependencies) = edges.split_once(':').unwrap();
                graph[node.parse::<usize>().unwrap()] = indices(dependencies);
            }
        }
        let expected = Analysis {
            cycle: (fields[3] != "-").then(|| indices(fields[3])),
            order: indices(fields[4]),
        };
        assert_eq!(
            analyze(&graph, Limits::default()),
            Ok(expected),
            "{}",
            fields[0]
        );
    }
}

/// Node and written-edge budgets precede invalid references and duplicate removal.
#[test]
fn graph_admission_is_bounded_and_separate_from_cycles() {
    let graph = vec![vec![1, 1, 99], vec![]];
    assert_eq!(
        analyze(&graph, Limits { nodes: 1, edges: 0 }),
        Err(Error::NodeLimit {
            limit: 1,
            required: 2
        })
    );
    assert_eq!(
        analyze(&graph, Limits { nodes: 2, edges: 2 }),
        Err(Error::EdgeLimit {
            limit: 2,
            required: 3
        })
    );
    assert_eq!(
        analyze(&graph, Limits { nodes: 2, edges: 3 }),
        Err(Error::InvalidDependency {
            node: 0,
            dependency: 99
        })
    );
    assert_eq!(
        analyze(&[vec![0]], Limits { nodes: 1, edges: 1 }),
        Ok(Analysis {
            cycle: Some(vec![0, 0]),
            order: vec![]
        })
    );
}

/// A full-budget deep graph does not consume the host stack, and reuse has fresh state.
#[test]
fn deep_graphs_and_repeated_analysis_are_independent() {
    let count = Limits::default().nodes;
    let mut graph: Vec<_> = (0..count)
        .map(|node| {
            if node + 1 < count {
                vec![node + 1]
            } else {
                vec![]
            }
        })
        .collect();
    let expected = Analysis {
        cycle: None,
        order: (0..count).rev().collect(),
    };
    for _ in 0..2 {
        assert_eq!(analyze(&graph, Limits::default()), Ok(expected.clone()));
    }
    graph[count - 1].push(0);
    let mut cycle: Vec<_> = (0..count).collect();
    cycle.push(0);
    assert_eq!(
        analyze(&graph, Limits::default()),
        Ok(Analysis {
            cycle: Some(cycle),
            order: vec![]
        })
    );
    graph[count - 1].clear();
    assert_eq!(analyze(&graph, Limits::default()), Ok(expected));
}
