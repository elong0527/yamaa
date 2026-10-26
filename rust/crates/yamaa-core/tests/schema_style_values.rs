//! Public typed-value controls for document layout edits.
//!
//! The seam is `Document::same_style_value` over documents built node by
//! node, including 2,048-field mappings and scalar key kinds no YAML text
//! spells apart, a predicate no run reports.
use yamaa_core::schema::{Document, DocumentNode as N};

fn push(nodes: &mut Vec<N>, node: N) -> usize {
    let id = nodes.len();
    nodes.push(node);
    id
}

fn mapping(entries: Vec<(N, N)>) -> Document {
    let mut nodes = Vec::new();
    let entries = entries
        .into_iter()
        .map(|(key, value)| (push(&mut nodes, key), push(&mut nodes, value)))
        .collect();
    let root = push(&mut nodes, N::Mapping(entries));
    Document::new(nodes, root).unwrap()
}

fn wide_nested_mapping(reverse: bool, changed: Option<usize>) -> Document {
    let mut nodes = Vec::new();
    let mut entries = Vec::new();
    for position in 0..2048 {
        let key = push(&mut nodes, N::Text(format!("field_{position:04}")));
        let number_key = push(&mut nodes, N::Text("number".into()));
        let number = position + usize::from(changed == Some(position));
        let number_value = push(&mut nodes, N::Integer(number.to_string()));
        let text_key = push(&mut nodes, N::Text("text".into()));
        let text_value = push(&mut nodes, N::Text(format!("value_{position}")));
        let mut fields = vec![(number_key, number_value), (text_key, text_value)];
        if reverse {
            fields.reverse();
        }
        let value = push(&mut nodes, N::Mapping(fields));
        entries.push((key, value));
    }
    if reverse {
        entries.reverse();
    }
    let root = push(&mut nodes, N::Mapping(entries));
    Document::new(nodes, root).unwrap()
}

#[test]
fn wide_nested_mapping_order_preserves_values_but_changed_values_do_not() {
    let original = wide_nested_mapping(false, None);
    let reordered = wide_nested_mapping(true, None);
    assert!(original.same_style_value(&reordered));
    assert!(reordered.same_style_value(&original));

    // The final declared field changes while every key and other value agrees.
    let changed = wide_nested_mapping(true, Some(2047));
    assert!(!original.same_style_value(&changed));
    assert!(!changed.same_style_value(&original));
}

#[test]
fn mapping_key_scalar_kinds_remain_distinct_despite_numeric_key_equivalence() {
    for keys in [
        [
            N::Boolean(true),
            N::Integer("1".into()),
            N::Float(1.0),
            N::Text("1".into()),
        ],
        [
            N::Boolean(false),
            N::Integer("0".into()),
            N::Float(0.0),
            N::Text("0".into()),
        ],
    ] {
        for (left_kind, left) in keys.iter().enumerate() {
            let left = mapping(vec![(left.clone(), N::Text("held".into()))]);
            for (right_kind, right) in keys.iter().enumerate() {
                let right = mapping(vec![(right.clone(), N::Text("held".into()))]);
                assert_eq!(left.same_style_value(&right), left_kind == right_kind);
            }
        }
    }
}

#[test]
fn mixed_scalar_mapping_keys_can_be_reordered() {
    let entries = vec![
        (N::Null, N::Text("null".into())),
        (N::Boolean(true), N::Text("boolean".into())),
        (N::Integer("2".into()), N::Text("integer".into())),
        (N::Float(0.5), N::Text("fraction".into())),
        (N::Text("2".into()), N::Text("text".into())),
    ];
    let original = mapping(entries.clone());
    let reordered = mapping(entries.into_iter().rev().collect());
    assert!(original.same_style_value(&reordered));
    assert!(reordered.same_style_value(&original));
}

#[test]
fn sequence_order_and_scalar_value_kinds_remain_significant() {
    let sequence = |values: &[i64]| {
        let mut nodes = values
            .iter()
            .map(|value| N::Integer(value.to_string()))
            .collect::<Vec<_>>();
        let root = nodes.len();
        nodes.push(N::Sequence((0..root).collect()));
        Document::new(nodes, root).unwrap()
    };
    let original = sequence(&[1, 2, 3]);
    assert!(original.same_style_value(&sequence(&[1, 2, 3])));
    assert!(!original.same_style_value(&sequence(&[3, 2, 1])));

    let values = [N::Boolean(true), N::Integer("1".into()), N::Float(1.0)];
    for (left_kind, left) in values.iter().enumerate() {
        let left = Document::new(vec![left.clone()], 0).unwrap();
        for (right_kind, right) in values.iter().enumerate() {
            let right = Document::new(vec![right.clone()], 0).unwrap();
            assert_eq!(left.same_style_value(&right), left_kind == right_kind);
        }
    }
}
