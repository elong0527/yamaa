use super::*;
#[path = "scalar.rs"]
mod scalar;
use crate::yaml_source::{scan_strict, LocatedEvent, ScanFailure, SourceEvent, SourceText};
use std::collections::{BTreeSet, VecDeque};
use yamaa_core::limits::Limits;
use yamaa_core::schema::{
    scalar_diagnostic_label, DocumentError, DocumentNode, DocumentResource, ScalarKey,
};
use yamaa_saphyr_parser::ScalarStyle;

impl From<ScanFailure> for DecodeFailure {
    fn from(error: ScanFailure) -> Self {
        match error {
            ScanFailure::NonAscii(position) => Self::NonAscii(position),
            ScanFailure::Syntax { position, reason } => Self::InvalidYaml { position, reason },
            ScanFailure::Limit(resource) => Self::Limit(resource),
            ScanFailure::Reconstruction => Self::Internal,
        }
    }
}

enum Value {
    Scalar(DocumentNode),
    InvalidText(Vec<u32>),
    Sequence(Vec<usize>),
    Mapping(Vec<(usize, usize)>),
}

struct Node {
    value: Value,
    start: SourcePosition,
    end: SourcePosition,
    merge: bool,
}

struct Frame {
    mapping: bool,
    start: SourcePosition,
    children: Vec<usize>,
}

fn invalid_yaml(position: SourcePosition, reason: &str) -> DecodeFailure {
    DecodeFailure::InvalidYaml {
        position,
        reason: reason.into(),
    }
}

fn build(events: Vec<LocatedEvent>) -> Result<(Vec<Node>, usize), DecodeFailure> {
    let mut nodes = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    let mut root = None;
    for LocatedEvent { event, start, end } in events {
        if matches!(
            event,
            SourceEvent::Scalar(..)
                | SourceEvent::SequenceStart(..)
                | SourceEvent::MappingStart(..)
        ) && stack.len() + 1 > Limits::YAML_DEPTH
        {
            return Err(DecodeFailure::Limit(LimitExceeded {
                resource: "yaml_depth",
                limit: Limits::YAML_DEPTH,
                actual: Some(stack.len() + 1),
            }));
        }
        let (value, start, merge) = match event {
            SourceEvent::Scalar(text, style, 0, None) => {
                let merge = style == ScalarStyle::Plain && text == SourceText::Unicode("<<".into());
                let value = match text {
                    SourceText::Unicode(text) => Value::Scalar(if style == ScalarStyle::Plain {
                        scalar::decode(text)?
                    } else {
                        DocumentNode::Text(text)
                    }),
                    SourceText::Invalid(points) => Value::InvalidText(points),
                };
                (value, start, merge)
            }
            SourceEvent::SequenceStart(0, None) => {
                stack.push(Frame {
                    mapping: false,
                    start,
                    children: Vec::new(),
                });
                continue;
            }
            SourceEvent::MappingStart(0, None) => {
                stack.push(Frame {
                    mapping: true,
                    start,
                    children: Vec::new(),
                });
                continue;
            }
            SourceEvent::SequenceEnd | SourceEvent::MappingEnd => {
                let frame = stack.pop().ok_or(DecodeFailure::Internal)?;
                if frame.mapping != matches!(event, SourceEvent::MappingEnd) {
                    return Err(DecodeFailure::Internal);
                }
                let value = if frame.mapping {
                    let pairs = frame.children.chunks_exact(2);
                    if !pairs.remainder().is_empty() {
                        return Err(DecodeFailure::Internal);
                    }
                    Value::Mapping(pairs.map(|pair| (pair[0], pair[1])).collect())
                } else {
                    Value::Sequence(frame.children)
                };
                (value, frame.start, false)
            }
            SourceEvent::StreamStart
            | SourceEvent::StreamEnd
            | SourceEvent::DocumentStart(_)
            | SourceEvent::DocumentEnd => continue,
            _ => return Err(DecodeFailure::Internal),
        };
        let id = nodes.len();
        nodes.push(Node {
            value,
            start,
            end,
            merge,
        });
        if let Some(parent) = stack.last_mut() {
            parent.children.push(id);
        } else if root.replace(id).is_some() {
            return Err(DecodeFailure::Internal);
        }
    }
    if !stack.is_empty() {
        return Err(DecodeFailure::Internal);
    }
    if let Some(root) = root {
        return Ok((nodes, root));
    }
    nodes.push(Node {
        value: Value::Scalar(DocumentNode::Null),
        start: SourcePosition {
            offset: 0,
            line: 1,
            column: 1,
        },
        end: SourcePosition {
            offset: 0,
            line: 1,
            column: 1,
        },
        merge: false,
    });
    Ok((nodes, 0))
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum MappingKey<'a> {
    Scalar(ScalarKey<'a>),
    InvalidText(&'a [u32]),
}

/// Match the reference constructor's deferred container order. A containing
/// mapping's merge/unhashable/duplicate checks finish before nested containers.
fn check_mappings(nodes: &[Node], root: usize) -> Result<(), DecodeFailure> {
    let mut queue = VecDeque::from([root]);
    while let Some(id) = queue.pop_front() {
        match &nodes[id].value {
            Value::Sequence(items) => queue.extend(items),
            Value::Mapping(entries) => {
                if let Some(&(key, _)) = entries.iter().find(|&&(key, _)| nodes[key].merge) {
                    return Err(invalid_yaml(
                        nodes[key].start,
                        "YAML merge keys are not allowed",
                    ));
                }
                let mut keys = BTreeSet::new();
                for &(key, value) in entries {
                    let identity = match &nodes[key].value {
                        Value::Scalar(value) => {
                            MappingKey::Scalar(value.scalar_key().ok_or(DecodeFailure::Internal)?)
                        }
                        Value::InvalidText(points) => MappingKey::InvalidText(points),
                        _ => {
                            return Err(invalid_yaml(
                                nodes[key].start,
                                "a mapping key must be a scalar",
                            ))
                        }
                    };
                    if !keys.insert(identity) {
                        let mut reason = String::new();
                        reason.push_str("found duplicate key '");
                        let label = key_label(&nodes[key])?;
                        reason.push_str(&label);
                        reason.push('\'');
                        return Err(DecodeFailure::InvalidYaml {
                            position: nodes[key].start,
                            reason,
                        });
                    }
                    queue.push_back(value);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn escape(points: impl Iterator<Item = u32>) -> String {
    let mut output = String::new();
    for point in points {
        let text = match point {
            9 => "\\t".into(),
            10 => "\\n".into(),
            13 => "\\r".into(),
            92 => "\\\\".into(),
            32..=126 => char::from_u32(point).expect("ASCII code point").to_string(),
            0..=255 => format!("\\x{point:02x}"),
            256..=65535 => format!("\\u{point:04x}"),
            _ => format!("\\U{point:08x}"),
        };
        output.push_str(&text);
    }
    output
}
fn key_label(node: &Node) -> Result<String, DecodeFailure> {
    match &node.value {
        Value::InvalidText(points) => Ok(escape(points.iter().copied())),
        Value::Scalar(value) => {
            let label = scalar_diagnostic_label(value).ok_or(DecodeFailure::Internal)?;
            Ok(escape(label.chars().map(u32::from)))
        }
        _ => Err(DecodeFailure::Internal),
    }
}

// Keep only borrowed node identities and small indices on the traversal stack.
// A long ancestor key is rendered once per actual issue, never for each valid
// descendant or queued sibling. Source admission has already bounded depth.
enum PathSegment {
    Key,
    Field(usize),
    Index(usize),
}

fn unicode_path(nodes: &[Node], segments: &[PathSegment]) -> Result<String, DecodeFailure> {
    use std::fmt::Write;
    let mut path = String::from("$");
    for segment in segments {
        match *segment {
            PathSegment::Key => path.push_str(".<key>"),
            PathSegment::Field(key) => {
                path.push('.');
                path.push_str(&key_label(&nodes[key])?);
            }
            PathSegment::Index(index) => {
                write!(path, "[{index}]").expect("writing to an owned string");
            }
        }
    }
    Ok(path)
}

fn unicode_issues(
    nodes: &[Node],
    id: usize,
    segments: &mut Vec<PathSegment>,
    issues: &mut Vec<UnicodeIssue>,
) -> Result<(), DecodeFailure> {
    match &nodes[id].value {
        Value::InvalidText(points) => {
            let (offset, &code_point) = points
                .iter()
                .enumerate()
                .find(|(_, point)| (0xd800..=0xdfff).contains(*point))
                .ok_or(DecodeFailure::Internal)?;
            issues.push(UnicodeIssue {
                path: unicode_path(nodes, segments)?,
                code_point,
                offset,
            });
        }
        Value::Sequence(items) => {
            for (index, &item) in items.iter().enumerate() {
                segments.push(PathSegment::Index(index));
                unicode_issues(nodes, item, segments, issues)?;
                segments.pop();
            }
        }
        Value::Mapping(entries) => {
            for &(key, value) in entries {
                segments.push(PathSegment::Key);
                unicode_issues(nodes, key, segments, issues)?;
                segments.pop();
                segments.push(PathSegment::Field(key));
                unicode_issues(nodes, value, segments, issues)?;
                segments.pop();
            }
        }
        _ => {}
    }
    Ok(())
}

fn check_unicode(nodes: &[Node], root: usize) -> Result<(), DecodeFailure> {
    if !nodes
        .iter()
        .any(|node| matches!(node.value, Value::InvalidText(_)))
    {
        return Ok(());
    }
    let mut issues = Vec::new();
    unicode_issues(nodes, root, &mut Vec::new(), &mut issues)?;
    Err(DecodeFailure::InvalidText(issues))
}

/// Decode source without host scalar conversion, filesystem IO or schema rules.
/// A result is published only after syntax, mapping and Unicode admission.
pub fn decode_yaml(source: &[u8]) -> Result<DecodedYaml, DecodeFailure> {
    let events = scan_strict(source)?;
    let (nodes, root) = build(events)?;
    check_mappings(&nodes, root)?;
    check_unicode(&nodes, root)?;
    let mut document_nodes = Vec::with_capacity(nodes.len());
    let mut locations = Vec::with_capacity(nodes.len());
    let mut end_locations = Vec::with_capacity(nodes.len());
    for node in nodes {
        locations.push(node.start);
        end_locations.push(node.end);
        document_nodes.push(match node.value {
            Value::Scalar(value) => value,
            Value::Sequence(items) => DocumentNode::Sequence(items),
            Value::Mapping(entries) => DocumentNode::Mapping(entries),
            Value::InvalidText(_) => return Err(DecodeFailure::Internal),
        });
    }
    let document = Document::new(document_nodes, root).map_err(|error| match error {
        DocumentError::Limit { resource, limit } => DecodeFailure::Limit(LimitExceeded {
            resource: match resource {
                DocumentResource::Depth => "yaml_depth",
            },
            limit,
            actual: None,
        }),
        _ => DecodeFailure::Internal,
    })?;
    Ok(DecodedYaml {
        document,
        locations,
        end_locations,
    })
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
