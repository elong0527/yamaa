use yamaa_core::column_dependencies::{analyze, Analysis, Diagnostic, Error};
use yamaa_core::dependency_analysis::{Error as GraphError, Limits};

/// Decode hand-authored fixture indices, independently of the transport parser.
fn indices(text: &str) -> Vec<usize> {
    if text == "-" || text == "_" {
        vec![]
    } else {
        text.split(',').map(|n| n.parse().unwrap()).collect()
    }
}

/// Pin rule order, authored dependency/key order, and completed-phase scheduling.
#[test]
fn shared_column_dependency_truth() {
    for line in include_str!("fixtures/column_dependencies.tsv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split('\t').collect();
        let graph = if fields[1] == "~" {
            vec![]
        } else {
            fields[1]
                .split(';')
                .map(|edges| (edges != "-").then(|| indices(edges)))
                .collect()
        };
        let diagnostics = if fields[5] == "-" {
            vec![]
        } else {
            fields[5]
                .split(';')
                .map(|entry| {
                    let (kind, columns) = entry.split_once(':').unwrap();
                    let columns = indices(columns);
                    match kind {
                        "cycle" => Diagnostic::Cycle { columns },
                        "forward" => Diagnostic::ForwardReference {
                            column: columns[0],
                            dependency: columns[1],
                        },
                        "missing" => Diagnostic::MissingKeyDerivation { column: columns[0] },
                        "key" => Diagnostic::KeyDependency {
                            column: columns[0],
                            dependency: columns[1],
                        },
                        _ => panic!("unknown fixture diagnostic"),
                    }
                })
                .collect()
        };
        assert_eq!(
            analyze(
                &graph,
                &indices(fields[2]),
                fields[3].parse().unwrap(),
                Limits::default()
            ),
            Ok(Analysis {
                order: indices(fields[4]),
                diagnostics
            }),
            "{}",
            fields[0]
        );
    }
}

/// Limits precede index admission; malformed metadata never poisons the next call.
#[test]
fn metadata_admission_and_reuse() {
    let graph = [Some(vec![1, 1, 99]), None];
    for _ in 0..2 {
        assert_eq!(
            analyze(&graph, &[99], false, Limits { nodes: 1, edges: 0 }),
            Err(Error::Graph(GraphError::NodeLimit {
                limit: 1,
                required: 2
            }))
        );
        assert_eq!(
            analyze(&graph, &[99], false, Limits { nodes: 2, edges: 2 }),
            Err(Error::Graph(GraphError::EdgeLimit {
                limit: 2,
                required: 3
            }))
        );
        assert_eq!(
            analyze(&graph, &[99], false, Limits { nodes: 2, edges: 3 }),
            Err(Error::Graph(GraphError::InvalidDependency {
                node: 0,
                dependency: 99
            }))
        );
        assert_eq!(
            analyze(&[None], &[1], true, Limits::default()),
            Err(Error::InvalidKey { key: 1 })
        );
        assert_eq!(
            analyze(&[None], &[0, 0], true, Limits::default()),
            Err(Error::DuplicateKey { key: 0 })
        );
        assert_eq!(
            analyze(&[Some(vec![])], &[0], false, Limits::default()),
            Ok(Analysis {
                order: vec![0],
                diagnostics: vec![]
            })
        );
    }
}

/// Host path selection is explicit even where two failures share one condition.
#[test]
fn portable_diagnostic_metadata() {
    let cases = [
        (
            Diagnostic::Cycle {
                columns: vec![1, 2, 1],
            },
            "dependency_cycle",
            "REQ-0072",
            "operation",
            vec![1, 2, 1],
        ),
        (
            Diagnostic::ForwardReference {
                column: 0,
                dependency: 2,
            },
            "forward_reference",
            "REQ-0071",
            "operation",
            vec![0, 2],
        ),
        (
            Diagnostic::MissingKeyDerivation { column: 2 },
            "key_dependency",
            "REQ-0074",
            "declaration",
            vec![2],
        ),
        (
            Diagnostic::KeyDependency {
                column: 2,
                dependency: 0,
            },
            "key_dependency",
            "REQ-0074",
            "expression",
            vec![2, 0],
        ),
    ];
    for (diagnostic, condition, requirement, location, columns) in cases {
        assert_eq!(
            (
                diagnostic.condition(),
                diagnostic.requirement(),
                diagnostic.location(),
                diagnostic.columns()
            ),
            (condition, requirement, location, columns)
        );
    }
}
