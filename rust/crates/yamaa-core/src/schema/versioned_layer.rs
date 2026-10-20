//! Admission of one 1.x specification layer at the language version it
//! declares, before parent traversal or composition (LAYER-4, P-VER-1).

use super::{
    path_render, validation::unknown_field, Document, DocumentNode as N, SchemaContext as C,
    SchemaDiagnostic, SchemaStructure, ValidationError,
};
use crate::diagnostic::ConditionCode;

/// Root collections whose members compose by identifier; inside a member a
/// layer defers requiredness to the resolved specification (LAYER-4).
const COLLECTIONS: [&str; 5] = ["input", "windows", "intermediates", "columns", "rows"];

/// The keyed collections written as lists, and the field each member is
/// identified by.
const IDENTITIES: [(&str, &str); 3] =
    [("columns", "name"), ("rows", "id"), ("intermediates", "id")];

impl SchemaStructure {
    /// The findings of one 1.x specification layer, checked against this view
    /// of the 1.0 closure: the one its declared version selects
    /// ([`SchemaStructure::at_version`]). The header has already been read
    /// (`language::read_header`).
    ///
    /// A layer need not be complete: a root field may be absent, and inside a
    /// member of a keyed collection requiredness waits for the resolved
    /// specification. Every other value a layer writes is checked where it is
    /// written: a root field replaces the value it inherits whole, so it is
    /// complete. YAML null is an ordinary value, admitted only where a field's
    /// type admits it. Two members of one keyed collection must not share an
    /// identifier (LAYER-4). Known fields are checked in schema order, then
    /// unknown fields in written order.
    pub fn validate_layer(
        &self,
        input: &Document,
    ) -> Result<Vec<SchemaDiagnostic>, ValidationError> {
        let class = self.root_class();
        let root = input.root();
        let entries = match &input.nodes()[root] {
            N::Mapping(entries) if !entries.is_empty() => entries,
            other => {
                return Ok(vec![SchemaDiagnostic {
                    path: "$".into(),
                    code: ConditionCode::SchemaInvalidFieldType,
                    context: vec![
                        ("expected", C::Text(class.name.clone())),
                        ("actual", C::ValueKind(other.kind())),
                    ],
                }]);
            }
        };
        let mut findings = Vec::new();
        for field in &class.fields {
            let Some(value) = input.field(root, &field.name) else {
                continue;
            };
            let fragment = COLLECTIONS.contains(&field.name.as_str());
            findings.extend(self.validate_descriptor(
                field.descriptor,
                input,
                value,
                &field.name,
                fragment,
            )?);
        }
        for &(key, _) in entries {
            let known = matches!(&input.nodes()[key], N::Text(name)
                if class.fields.iter().any(|field| field.name == *name));
            if known {
                continue;
            }
            let label = path_render::label(input, key)?;
            let (path, later) = match &input.nodes()[key] {
                N::Text(name) => (label.clone(), self.later_field(&class.name, name)),
                // An integer or Boolean name takes the bracketed path spelling.
                _ => (format!("[{label}]"), None),
            };
            findings.push(unknown_field(&path, &label, &class.name, later));
        }
        findings.extend(repeated_identifiers(input));
        Ok(findings)
    }
}

/// Members of one keyed list in one layer that share an identifier, at the
/// repeated member's identity.
fn repeated_identifiers(input: &Document) -> Vec<SchemaDiagnostic> {
    let mut findings = Vec::new();
    for (collection, identity) in IDENTITIES {
        let Some(N::Sequence(members)) = input
            .field(input.root(), collection)
            .map(|node| &input.nodes()[node])
        else {
            continue;
        };
        let mut seen: Vec<&str> = Vec::new();
        for &member in members {
            let Some(N::Text(name)) = input.field(member, identity).map(|id| &input.nodes()[id])
            else {
                continue;
            };
            if seen.contains(&name.as_str()) {
                findings.push(SchemaDiagnostic {
                    path: format!("{collection}.{name}.{identity}"),
                    code: ConditionCode::SchemaLayerIdentifierRepeated,
                    context: vec![("identifier", C::Text(name.clone()))],
                });
            } else {
                seen.push(name);
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::LanguageVersion;
    use crate::schema::{DocumentKind, SchemaModule};
    use crate::test_document::Tree as T;

    fn descriptor(kind: &str, since: Option<&str>) -> T {
        let mut entries = vec![("type", T::text(kind))];
        if let Some(since) = since {
            entries.push(("since", T::text(since)));
        }
        T::map(entries)
    }

    /// A root with one plain field, one keyed collection and one expression
    /// field whose `mapping` operation gained a field in 1.1.
    fn schema() -> SchemaStructure {
        let field = |name: &str, kind: &str, since| T::map(vec![(name, descriptor(kind, since))]);
        SchemaStructure::admit(
            vec![SchemaModule {
                name: "schema_spec.yaml".into(),
                document: T::map(vec![
                    ("version", T::text("1.0")),
                    (
                        "root_class",
                        T::List(vec![
                            field("domain", "str", None),
                            field("columns", "list[column_class]", None),
                            field("value", "expression", None),
                        ]),
                    ),
                    (
                        "column_class",
                        T::List(vec![T::map(vec![(
                            "name",
                            T::map(vec![
                                ("type", T::text("str")),
                                ("required", T::boolean(true)),
                            ]),
                        )])]),
                    ),
                    (
                        "expression",
                        T::map(vec![("registry", T::text("operations"))]),
                    ),
                    (
                        "operations",
                        T::map(vec![(
                            "mapping",
                            T::List(vec![
                                field("source", "str", None),
                                field("added", "str", Some("1.1")),
                            ]),
                        )]),
                    ),
                ])
                .document(),
            }],
            0,
            "root_class",
        )
        .unwrap()
    }

    #[test]
    fn a_layer_is_a_non_empty_mapping_whose_unknown_names_keep_their_spelling() {
        let schema = schema();
        for (root, kind) in [
            (T::list(vec![]), DocumentKind::Sequence),
            (T::map(vec![]), DocumentKind::Mapping),
        ] {
            let findings = schema.validate_layer(&root.document()).unwrap();
            assert_eq!(findings.len(), 1);
            assert_eq!(findings[0].path, "$");
            assert_eq!(findings[0].context[1], ("actual", C::ValueKind(kind)));
        }
        let layer = T::Pairs(vec![
            (T::integer(1), T::text("x")),
            (T::text("domain"), T::text("DM")),
        ]);
        let findings = schema.validate_layer(&layer.document()).unwrap();
        assert_eq!(
            findings
                .iter()
                .map(|finding| (finding.code, finding.path.as_str()))
                .collect::<Vec<_>>(),
            [(ConditionCode::SchemaUnknownField, "[1]")]
        );
    }

    #[test]
    fn a_version_view_leaves_out_a_later_operation_field() {
        let schema = schema();
        let layer = T::map(vec![(
            "value",
            T::map(vec![(
                "mapping",
                T::map(vec![("source", T::text("A")), ("added", T::text("B"))]),
            )]),
        )])
        .document();
        let older = schema.at_version(LanguageVersion::new(0));
        assert_eq!(
            older.later_field("mapping", "added"),
            Some(LanguageVersion::new(1))
        );
        let findings = older.validate_layer(&layer).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, ConditionCode::SchemaUnknownFieldSince);
        assert_eq!(findings[0].path, "value.mapping.added");
        assert_eq!(
            findings[0].context,
            [
                ("field", C::Text("added".into())),
                ("class", C::Text("mapping".into())),
                ("since", C::Text("1.1".into())),
            ]
        );
        let current = schema.at_version(LanguageVersion::new(1));
        assert_eq!(current.later_field("mapping", "added"), None);
        assert!(current.validate_layer(&layer).unwrap().is_empty());
        // The full closure and its views share descriptor indices.
        assert_eq!(schema.descriptors().len(), older.descriptors().len());
    }
}
