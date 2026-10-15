use yamaa_core::schema::{
    Document, DocumentError, DocumentKind, DocumentLimits, DocumentNode as N, DocumentResource,
};

#[test]
fn decoded_tree_preserves_order_width_and_unicode_scalar_lengths() {
    let nodes = vec![
        N::Text("z".into()),
        N::Integer("9223372036854775808".into()),
        N::Text("a".into()),
        N::Text("e\u{301}😀".into()),
        N::Mapping(vec![(0, 1), (2, 3)]),
    ];
    let document = Document::new(nodes.clone(), 4, DocumentLimits::default()).unwrap();
    assert_eq!(document.nodes(), nodes);
    assert_eq!(document.nodes()[3].length(), Some(3));
    assert_eq!(document.nodes()[1].kind(), DocumentKind::Integer);
    assert_eq!(document.nodes()[1].kind().name(), "integer");
    assert_eq!(document.field(4, "a"), Some(3));
    assert_eq!(document.field(4, "missing"), None);
}

#[test]
fn source_adapters_share_scalar_identity_and_diagnostic_labels() {
    use yamaa_core::schema::scalar_diagnostic_label;
    let integer = N::Integer("9007199254740993".into());
    let rounded = N::Float(9007199254740992.0);
    assert!(integer.scalar_key() != rounded.scalar_key());
    assert!(N::Boolean(true).scalar_key() == N::Float(1.0).scalar_key());
    for invalid in [
        N::Integer("01".into()),
        N::Float(f64::NAN),
        N::Sequence(vec![]),
    ] {
        assert!(invalid.scalar_key().is_none());
    }
    assert_eq!(
        scalar_diagnostic_label(&integer).as_deref(),
        Some("9007199254740993")
    );
    assert_eq!(
        scalar_diagnostic_label(&N::Float(-0.0)).as_deref(),
        Some("-0.0")
    );
    assert_eq!(
        scalar_diagnostic_label(&N::Float(1e20)).as_deref(),
        Some("1e+20")
    );
    assert!(scalar_diagnostic_label(&N::Float(f64::INFINITY)).is_none());
}

#[test]
fn malformed_arenas_do_not_enter_schema_interpretation() {
    for (nodes, root, error) in [
        (vec![], 0, DocumentError::InvalidRoot),
        (vec![N::Sequence(vec![0])], 0, DocumentError::InvalidChild),
        (
            vec![N::Null, N::Sequence(vec![0, 0])],
            1,
            DocumentError::SharedChild,
        ),
        (vec![N::Null, N::Null], 1, DocumentError::UnreachableNode),
        (vec![N::Float(f64::INFINITY)], 0, DocumentError::NonFinite),
        (
            vec![N::Sequence(vec![]), N::Null, N::Mapping(vec![(0, 1)])],
            2,
            DocumentError::NonScalarKey,
        ),
    ] {
        assert_eq!(
            Document::new(nodes, root, DocumentLimits::default()),
            Err(error)
        );
    }
    for text in ["", "-", "+1", "00", "-0", "01", "1.0", " 1", "1a", "١"] {
        assert_eq!(
            Document::new(vec![N::Integer(text.into())], 0, DocumentLimits::default()),
            Err(DocumentError::InvalidInteger),
            "{text:?}"
        );
    }
}

#[test]
fn scalar_mapping_key_equality_is_exact_without_lossy_integer_promotion() {
    for (left, right) in [
        (N::Boolean(true), N::Integer("1".into())),
        (N::Float(-0.0), N::Boolean(false)),
        (N::Float(1.0), N::Integer("1".into())),
        (N::Float(-1.0), N::Integer("-1".into())),
        (
            N::Float(9_007_199_254_740_992.0),
            N::Integer("9007199254740992".into()),
        ),
        (
            N::Float(18_446_744_073_709_551_616.0),
            N::Integer("18446744073709551616".into()),
        ),
        (N::Float(0.5), N::Float(0.5)),
        (N::Null, N::Null),
    ] {
        assert_eq!(
            Document::new(
                vec![
                    left,
                    N::Null,
                    right,
                    N::Null,
                    N::Mapping(vec![(0, 1), (2, 3)])
                ],
                4,
                DocumentLimits::default()
            ),
            Err(DocumentError::DuplicateKey)
        );
    }
    for (left, right) in [
        (
            N::Float(9_007_199_254_740_992.0),
            N::Integer("9007199254740993".into()),
        ),
        (N::Float(f64::from_bits(1)), N::Integer("0".into())),
        (N::Float(-0.5), N::Float(0.5)),
        (N::Text("1".into()), N::Integer("1".into())),
    ] {
        assert!(Document::new(
            vec![
                left,
                N::Null,
                right,
                N::Null,
                N::Mapping(vec![(0, 1), (2, 3)])
            ],
            4,
            DocumentLimits::default()
        )
        .is_ok());
    }
}

#[test]
fn storage_limits_are_independent_and_do_not_leak_between_documents() {
    for (limits, resource, limit) in [
        (
            DocumentLimits {
                nodes: 1,
                ..DocumentLimits::default()
            },
            DocumentResource::Nodes,
            1,
        ),
        (
            DocumentLimits {
                text_bytes: 1,
                ..DocumentLimits::default()
            },
            DocumentResource::TextBytes,
            1,
        ),
        (
            DocumentLimits {
                edges: 0,
                ..DocumentLimits::default()
            },
            DocumentResource::Edges,
            0,
        ),
        (
            DocumentLimits {
                depth: 1,
                ..DocumentLimits::default()
            },
            DocumentResource::Depth,
            1,
        ),
    ] {
        let nodes = vec![N::Text("😀".into()), N::Sequence(vec![0])];
        assert_eq!(
            Document::new(nodes.clone(), 1, limits),
            Err(DocumentError::Limit { resource, limit })
        );
        assert!(Document::new(nodes, 1, DocumentLimits::default()).is_ok());
    }
    let mut nodes = vec![N::Null];
    for i in 0..64 {
        nodes.push(N::Sequence(vec![i]));
    }
    assert_eq!(
        Document::new(
            nodes,
            64,
            DocumentLimits {
                depth: usize::MAX,
                ..DocumentLimits::default()
            }
        ),
        Err(DocumentError::Limit {
            resource: DocumentResource::Depth,
            limit: 64
        })
    );
}
