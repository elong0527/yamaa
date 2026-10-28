//! Admission of one 1.x specification layer at the language version it
//! declares, before parent traversal or composition (LAYER-4, P-VER-1), and
//! its normalization before it composes.

use super::{
    path_render, spec_path,
    spec_path::SpecPath,
    validation::{one_parent_hint, unknown_field},
    Document, DocumentNode as N, NormalizationError, NormalizedDocument, SchemaContext as C,
    SchemaDiagnostic, SchemaOrigin, SchemaSource, SchemaStructure, SiteRole, ValidationError,
    WrittenSite,
};
use crate::diagnostic::ConditionCode;

/// Root collections whose members compose by identifier; inside a member a
/// layer defers requiredness to the resolved specification (LAYER-4).
pub(super) const COLLECTIONS: [&str; 5] = ["input", "windows", "intermediates", "columns", "rows"];

/// The sections of a project, each of which composes as one member, merging
/// with the section it inherits (PROJECT-16); inside one, as inside a member
/// of a specification's keyed collection, a layer defers requiredness and
/// defaults to the composed project (LAYER-4, LAYER-7).
const SECTIONS: [&str; 5] = ["environments", "study", "sdtm", "adam", "send"];

/// The keyed collections written as lists, and the field each member is
/// identified by.
pub(super) const IDENTITIES: [(&str, &str); 3] =
    [("columns", "name"), ("rows", "id"), ("intermediates", "id")];

/// The field each member of the root field `field` carries when it is a
/// keyed list (LAYER-4).
fn identity(field: &str) -> Option<&'static str> {
    IDENTITIES
        .iter()
        .find(|&&(collection, _)| collection == field)
        .map(|&(_, identity)| identity)
}

impl SchemaStructure {
    /// Whether the root field `field` of a layer read against this view
    /// composes member by member with the value it inherits, so that a layer
    /// may write it in part: a keyed collection of a specification (LAYER-5)
    /// or a section of a project (PROJECT-16). Every other root field is
    /// complete where it is written and replaces what it inherits.
    pub(super) fn composes_members(&self, field: &str) -> bool {
        if self.root_class().name == crate::language::FileType::Project.root_class() {
            SECTIONS.contains(&field)
        } else {
            COLLECTIONS.contains(&field)
        }
    }

    /// The findings of one 1.x specification layer, checked against this view
    /// of the 1.0 closure: the one its declared version selects
    /// ([`SchemaStructure::at_version`]). The header has already been read
    /// (`language::read_header`).
    ///
    /// A layer need not be complete: a root field may be absent, and inside a
    /// member of a keyed collection requiredness waits for the resolved
    /// specification. A member of a keyed list still carries the identity,
    /// `name` or `id`, that composition matches it by, at
    /// `<collection>[i].<identity>` when it does not, and a copy entry, which
    /// has none, is complete (SPEC-9). Every other value a layer writes is
    /// checked where it is written: a root field replaces the value it
    /// inherits whole, so it is complete. YAML null is an ordinary value,
    /// admitted only where a field's
    /// type admits it: a value or expression field (P-NULL-1). Two members of
    /// one keyed collection must not share an identifier (LAYER-4). Each
    /// `clear` entry is a spec path, and no entry covers another; a fault
    /// fails as `invalid_clear` at `clear[i]` (LAYER-9). A list written at
    /// `parent` fails as `invalid_field_type` with the hint an undeclared
    /// `parents` key carries (LAYER-2). Known fields are checked in schema
    /// order, then unknown fields in written order. Paths are written in the
    /// spec path notation (P-CLR-2).
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
            let fragment = self.composes_members(&field.name);
            let mut found = self.validate_versioned_descriptor(
                field.descriptor,
                input,
                value,
                &field.name,
                fragment,
                identity(&field.name),
            )?;
            if found.is_empty() && field.name == "clear" {
                findings.extend(clear_entries(input, value));
            }
            if field.name == "parent" && matches!(input.nodes()[value], N::Sequence(_)) {
                for finding in &mut found {
                    if finding.code == ConditionCode::SchemaInvalidFieldType {
                        finding.context.push(one_parent_hint(&class.name));
                    }
                }
            }
            findings.extend(found);
        }
        for &(key, _) in entries {
            let known = matches!(&input.nodes()[key], N::Text(name)
                if class.fields.iter().any(|field| field.name == *name));
            if known {
                continue;
            }
            let label = path_render::label(input, key)?;
            let (path, later) = match &input.nodes()[key] {
                N::Text(name) => (
                    spec_path::join("", &label),
                    self.later_field(&class.name, name),
                ),
                // An integer or Boolean name takes the bracketed path spelling.
                _ => (format!("[{label}]"), None),
            };
            findings.push(unknown_field(&path, &label, &class.name, later));
        }
        findings.extend(repeated_identifiers(input));
        Ok(findings)
    }
}

impl SchemaStructure {
    /// The written values of a 1.x document as validation reads them against
    /// this view, in walk order: each value of a class field, each value read
    /// as a named value type, and each long form that restates its shorthand
    /// (SCHEMA-15), with its spec path (P-CLR-2). A specification is read as
    /// [`SchemaStructure::validate_layer`] reads one layer, field by root
    /// field, and any other file type against its root class as a complete
    /// document. Where a union's members all fail, the sites are those of the
    /// member whose findings validation reports, so a document validation
    /// refuses still has the sites of the structure it was read as. The lint
    /// reads a file's defaults, expression fields and long forms from them.
    pub fn written_sites(
        &self,
        input: &Document,
        layer: bool,
    ) -> Result<Vec<WrittenSite>, ValidationError> {
        let class = self.root_class();
        let root = input.root();
        if !layer {
            return Ok(self.versioned_root_sites(input)?.1);
        }
        let mut sites = Vec::new();
        if !matches!(&input.nodes()[root], N::Mapping(_)) {
            return Ok(sites);
        }
        for field in &class.fields {
            let Some(value) = input.field(root, &field.name) else {
                continue;
            };
            sites.push(WrittenSite {
                node: value,
                path: field.name.clone(),
                role: SiteRole::Field {
                    owner: class.name.clone(),
                    field: field.name.clone(),
                    descriptor: field.descriptor,
                },
            });
            let fragment = self.composes_members(&field.name);
            let (_, found) = self.versioned_descriptor_sites(
                field.descriptor,
                input,
                value,
                &field.name,
                fragment,
                identity(&field.name),
            )?;
            sites.extend(found);
        }
        Ok(sites)
    }
}

/// Members of one keyed list in one layer that share an identifier, at the
/// repeated member's identity. A copy entry declares each column it lists,
/// so a name it lists that the layer declares again, or that an earlier
/// column declares, is repeated too, at the later of the two (SPEC-9).
fn repeated_identifiers(input: &Document) -> Vec<SchemaDiagnostic> {
    let mut findings = Vec::new();
    for (collection, identity) in IDENTITIES {
        let Some(N::Sequence(members)) = input
            .field(input.root(), collection)
            .map(|node| &input.nodes()[node])
        else {
            continue;
        };
        // Each name seen, and whether a copy entry declared it.
        let mut seen: Vec<(&str, bool)> = Vec::new();
        for (index, &member) in members.iter().enumerate() {
            if let Some(copy) = (collection == "columns")
                .then(|| input.field(member, "copy"))
                .flatten()
            {
                for (listed, (name, _)) in crate::copy_expansion::listed(input, copy)
                    .into_iter()
                    .enumerate()
                {
                    if seen.iter().any(|(prior, _)| *prior == name) {
                        findings.push(SchemaDiagnostic {
                            path: format!("columns[{index}].copy.columns[{listed}]"),
                            code: ConditionCode::CopyDuplicateIdentifier,
                            context: vec![("identifier", C::Text(name.into()))],
                        });
                    } else {
                        seen.push((name, true));
                    }
                }
                continue;
            }
            let Some(N::Text(name)) = input.field(member, identity).map(|id| &input.nodes()[id])
            else {
                continue;
            };
            match seen.iter().find(|(prior, _)| *prior == name) {
                Some(&(_, copied)) => findings.push(SchemaDiagnostic {
                    path: spec_path::join(&spec_path::join(collection, name), identity),
                    code: if copied {
                        ConditionCode::CopyDuplicateIdentifier
                    } else {
                        ConditionCode::SchemaLayerIdentifierRepeated
                    },
                    context: vec![("identifier", C::Text(name.clone()))],
                }),
                None => seen.push((name, false)),
            }
        }
    }
    findings
}

/// A finding about the entry `clear[index]` names, at that entry (LAYER-9):
/// today's `invalid_clear`, whose context names the field the entry ends at.
pub(super) fn invalid_clear(index: usize, field: &str) -> SchemaDiagnostic {
    SchemaDiagnostic {
        path: format!("clear[{index}]"),
        code: ConditionCode::SchemaLayerInvalidClear,
        context: vec![("field", C::Text(field.into()))],
    }
}

/// The written faults of a layer's `clear` list, already checked to be a list
/// of text: an entry outside the spec path notation, and an entry that a
/// path before it covers or that covers a path before it, which names its own
/// value or one beside another entry's (P-CLR-1).
fn clear_entries(input: &Document, list: usize) -> Vec<SchemaDiagnostic> {
    let N::Sequence(items) = &input.nodes()[list] else {
        return Vec::new();
    };
    let mut findings = Vec::new();
    let mut paths: Vec<SpecPath> = Vec::new();
    for (index, &item) in items.iter().enumerate() {
        let N::Text(written) = &input.nodes()[item] else {
            continue;
        };
        let Some(path) = SpecPath::parse(written) else {
            findings.push(invalid_clear(index, written));
            continue;
        };
        if paths
            .iter()
            .any(|earlier| earlier.covers(&path) || path.covers(earlier))
        {
            findings.push(invalid_clear(index, path.last()));
            continue;
        }
        paths.push(path);
    }
    findings
}

impl SchemaStructure {
    /// Normalize one admitted 1.x layer, checked by
    /// [`SchemaStructure::validate_layer`] against this same view, before it
    /// composes (LAYER-4): shorthand is expanded in every field, so equivalent
    /// spellings contribute the same value. A member of a keyed collection is
    /// a fragment whose defaults wait for the resolved specification; every
    /// other root field is complete, so its defaults apply here (LAYER-7).
    /// YAML null stays the value it is. Root fields follow schema order.
    pub fn normalize_versioned_layer(
        &self,
        input: &Document,
    ) -> Result<NormalizedDocument, NormalizationError> {
        let root = input.root();
        let N::Mapping(entries) = &input.nodes()[root] else {
            return Err(ValidationError::InvalidDescriptor.into());
        };
        let origin = |node| SchemaOrigin {
            source: SchemaSource::Input,
            node,
            generated: false,
        };
        let mut nodes = Vec::new();
        let mut origins = Vec::new();
        let mut fields = Vec::new();
        for field in &self.root_class().fields {
            let Some(&(key, value)) = entries.iter().find(
                |&&(key, _)| matches!(&input.nodes()[key], N::Text(name) if *name == field.name),
            ) else {
                continue;
            };
            let fragment = self.composes_members(&field.name);
            let normalized =
                self.normalize_versioned_descriptor(field.descriptor, input, value, fragment)?;
            let offset = nodes.len();
            for node in normalized.document.nodes() {
                nodes.push(match node {
                    N::Sequence(items) => N::Sequence(items.iter().map(|n| n + offset).collect()),
                    N::Mapping(items) => N::Mapping(
                        items
                            .iter()
                            .map(|(k, v)| (k + offset, v + offset))
                            .collect(),
                    ),
                    node => node.clone(),
                });
            }
            origins.extend(normalized.origins);
            nodes.push(N::Text(field.name.clone()));
            origins.push(origin(key));
            fields.push((nodes.len() - 1, offset + normalized.document.root()));
        }
        nodes.push(N::Mapping(fields));
        origins.push(origin(root));
        let document =
            Document::new(nodes, origins.len() - 1).map_err(NormalizationError::Document)?;
        Ok(NormalizedDocument { document, origins })
    }
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
                ("identifier", C::Text("added".into())),
                ("since", C::Text("1.1".into())),
            ]
        );
        let current = schema.at_version(LanguageVersion::new(1));
        assert_eq!(current.later_field("mapping", "added"), None);
        assert!(current.validate_layer(&layer).unwrap().is_empty());
        // The full closure and its views share descriptor indices.
        assert_eq!(schema.descriptors().len(), older.descriptors().len());
    }

    /// A root whose `terms` list holds a shorthand union: a bare variable or
    /// a class whose `direction` defaults to `asc`.
    fn terms_schema() -> SchemaStructure {
        let field = |name: &str, entries: Vec<(&str, T)>| T::map(vec![(name, T::map(entries))]);
        SchemaStructure::admit(
            vec![SchemaModule {
                name: "schema_spec.yaml".into(),
                document: T::map(vec![
                    ("version", T::text("1.0")),
                    (
                        "root_class",
                        T::List(vec![field("terms", vec![("type", T::text("list[term]"))])]),
                    ),
                    (
                        "term",
                        T::map(vec![(
                            "type",
                            T::list(vec![T::text("str"), T::text("term_class")]),
                        )]),
                    ),
                    (
                        "term_class",
                        T::List(vec![
                            field(
                                "variable",
                                vec![("type", T::text("str")), ("required", T::boolean(true))],
                            ),
                            field(
                                "direction",
                                vec![("type", T::text("str")), ("default", T::text("asc"))],
                            ),
                        ]),
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
    fn written_sites_follow_the_structure_validation_chooses() {
        let schema = terms_schema();
        let document = T::map(vec![(
            "terms",
            T::list(vec![
                T::text("A"),
                T::map(vec![("variable", T::text("B"))]),
                T::map(vec![
                    ("variable", T::text("C")),
                    ("direction", T::text("asc")),
                ]),
                T::map(vec![("variable", T::integer(1))]),
            ]),
        )])
        .document();
        let sites = schema.written_sites(&document, true).unwrap();
        let label = |site: &WrittenSite| match &site.role {
            SiteRole::Field { owner, field, .. } => format!("{} field {owner}.{field}", site.path),
            SiteRole::Alias { name } => format!("{} alias {name}", site.path),
            SiteRole::LongForm {
                field, shorthand, ..
            } => format!(
                "{} long form {field} {}",
                site.path,
                shorthand.map_or("keyword".into(), |node| format!(
                    "{:?}",
                    document.nodes()[node]
                ))
            ),
        };
        assert_eq!(
            sites.iter().map(label).collect::<Vec<_>>(),
            [
                "terms field root_class.terms",
                "terms[0] alias term",
                // `{variable: B}` restates its shorthand, which validation refuses.
                "terms[1] alias term",
                "terms[1].variable field term_class.variable",
                "terms[1] long form variable Text(\"B\")",
                // Writing the default keeps the class form valid.
                "terms[2] alias term",
                "terms[2].variable field term_class.variable",
                "terms[2].direction field term_class.direction",
                // Both members fail on a number: the class is the member
                // whose findings validation reports, so its sites stay.
                "terms[3] alias term",
                "terms[3].variable field term_class.variable",
            ]
        );
        let SiteRole::Field { descriptor, .. } = &sites[7].role else {
            panic!("a field site");
        };
        let default = schema.descriptors()[*descriptor].descriptor.default_node();
        assert!(default.is_some(), "the site names the field's descriptor");
        // A complete document of another type is read against its root class.
        assert_eq!(
            schema.written_sites(&document, false).unwrap().len(),
            sites.len()
        );
    }
}
