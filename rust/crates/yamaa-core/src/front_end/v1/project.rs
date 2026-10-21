//! Lower 1.0 project, function and codelist files into the project roots of
//! [`crate::ast::project`] (D8, D16, P-TYPE-1).
//!
//! Each file was admitted against the 1.0 closure its `type` selects, at the
//! version its `yamaa` declares, before it lowers here. It lowers as a 0.5
//! file does: every written scalar of a definition or a codelist converts to
//! the value the engine runs with, and lowering reports each one that does
//! not, and each node whose shape the schema should have refused, as a
//! finding at the file's own path to it, keeping the text it is written as.
//! Those findings decide preparation; nothing after lowering reads the file.
//!
//! - A project file ([`project`]) lists its function files in `functions` and
//!   holds each host language's lock in `environments.<host>.lock`, both
//!   relative to the project file (D8). A section writes `standards` and
//!   `transport` (D16, P-PRJ-1).
//! - A function file ([`function`]) names its function and writes the
//!   callable of each host language under `implementations`. Its test
//!   arguments, results and defaults are typed by the parameter and return
//!   types it declares, so a date is written as text (P-FN-1).
//! - A codelist file ([`codelists`]) holds one codelist source (P-TYPE-1).

use crate::ast::{
    Codelists, Function, FunctionDeclaration, FunctionSource, Header, HostEnvironment,
    Implementation, Meta, Named, Paths, Project, SubmissionRefused, Surface, Text,
};
use crate::front_end::v0_5::project::{
    cases, codelist_declarations, codelist_source, column_kind, comparison_decimals, parameters,
    text_field, text_of, Metas, Values,
};
use crate::front_end::v0_5::{sections, Positions};
use crate::language::LanguageVersion;
use crate::project_function::Language;
use crate::project_function_document::{Finding, Kind, Reader};
use crate::schema::{Document, DocumentNode as N};

/// The header `yamaa` declares. The file's `type` selected the closure it was
/// admitted against, and so the root it lowers into.
fn header(
    reader: &mut Reader<'_>,
    metas: &mut Metas<'_>,
    document: &Document,
    (root, meta): (usize, Meta),
) -> Header {
    let node = document.field(root, "yamaa");
    let written = node.and_then(|node| text_of(document, node));
    let version = written.and_then(LanguageVersion::parse);
    if version.is_none() {
        reader.fault("yamaa", node.unwrap_or(root), Kind::NormalizedShape);
    }
    let meta = match node {
        Some(node) => metas.field(meta, "yamaa", Some(node)),
        None => meta,
    };
    Header {
        surface: Surface::V1 {
            minor: version.map_or(0, LanguageVersion::minor),
        },
        version: Text {
            text: written.unwrap_or_default().into(),
            meta,
        },
        meta,
    }
}

/// Lower a project file. Its study and sections lower beside it: a project
/// whose declarations Define-XML composition could not read still prepares,
/// and a specification still builds with it, so their refusal is kept in
/// [`Project::submission`].
pub fn project(
    document: &Document,
    positions: Option<&Positions>,
) -> Result<Project, Vec<Finding>> {
    let root = document.root();
    let mut paths = Paths::new();
    let mut metas = Metas::new(positions, &mut paths);
    let meta = metas.root(root);
    let mut reader = Reader::new(document);
    if !matches!(document.nodes().get(root), Some(N::Mapping(_))) {
        reader.fault("", root, Kind::NormalizedShape);
    }
    let at = (root, meta);
    let header = header(&mut reader, &mut metas, document, at);
    let functions = document.field(root, "functions").map(|list| {
        let declared = metas.field(meta, "functions", Some(list));
        reader
            .sequence(root, "functions", "functions")
            .into_iter()
            .enumerate()
            .map(|(index, node)| {
                let meta = metas.index(declared, index, node);
                let text = reader.text(node, &format!("functions[{index}]"));
                FunctionDeclaration {
                    key: None,
                    source: FunctionSource::File(Text { text, meta }),
                    meta,
                }
            })
            .collect()
    });
    let environments = environments(&mut reader, &mut metas, document, at);
    let mut leaves = Vec::new();
    let codelists = codelist_declarations(
        document,
        positions,
        (&mut reader, &mut metas),
        at,
        &mut leaves,
    );
    let submission = sections::lower(document, root, &mut metas, meta, header.surface)
        .map_err(|_| SubmissionRefused);
    let mut findings = reader.finish();
    findings.extend(leaves);
    if !findings.is_empty() {
        return Err(findings);
    }
    Ok(Project {
        header,
        functions,
        environments,
        codelists,
        submission,
        paths,
        meta,
    })
}

/// Each host language's `environments` entry, which holds only its lock. A
/// host that declares no lock has no entry (PROJECT-2).
fn environments(
    reader: &mut Reader<'_>,
    metas: &mut Metas<'_>,
    document: &Document,
    (root, meta): (usize, Meta),
) -> Vec<HostEnvironment> {
    let Some(node) = document.field(root, "environments") else {
        return Vec::new();
    };
    let declared = metas.field(meta, "environments", Some(node));
    let mut environments = Vec::new();
    for language in [Language::Python, Language::R] {
        let key = language.key();
        let Some(entry) = document.field(node, key) else {
            continue;
        };
        let meta = metas.field(declared, key, Some(entry));
        let lock = text_field(
            reader,
            metas,
            document,
            (entry, meta),
            "lock",
            &format!("environments.{key}.lock"),
        );
        environments.push(HostEnvironment {
            language,
            file: None,
            lock: Some(lock),
            meta,
        });
    }
    environments
}

/// Lower a function file: one definition, which names its function and the
/// callable each host language binds (FUNC-4, FUNC-8).
pub fn function(
    document: &Document,
    positions: Option<&Positions>,
) -> Result<Function, Vec<Finding>> {
    let root = document.root();
    let mut paths = Paths::new();
    let mut metas = Metas::new(positions, &mut paths);
    let meta = metas.root(root);
    let mut reader = Reader::new(document);
    let at = (root, meta);
    let header = header(&mut reader, &mut metas, document, at);
    let name = text_field(&mut reader, &mut metas, document, at, "name", "name");
    let description = text_field(
        &mut reader,
        &mut metas,
        document,
        at,
        "description",
        "description",
    );
    let params = parameters(&mut reader, &mut metas, document, at, Values::Declared);
    let returns = column_kind(&mut reader, root, "returns");
    let may_return_missing = reader.boolean(root, "may_return_missing", "may_return_missing");
    let comparison_decimals = comparison_decimals(&mut reader, root);
    let implementations = implementations(&mut reader, &mut metas, document, at);
    let tests = cases(
        &mut reader,
        &mut metas,
        document,
        at,
        (&params, returns, Values::Declared),
    );
    let findings = reader.finish();
    if !findings.is_empty() {
        return Err(findings);
    }
    Ok(Function {
        header: Some(header),
        name: Named {
            name: name.text,
            meta: name.meta,
        },
        description,
        params,
        returns,
        may_return_missing,
        comparison_decimals,
        implementations,
        tests,
        paths,
        meta,
    })
}

/// `implementations`: the callable of each host language the file writes,
/// in written order.
fn implementations(
    reader: &mut Reader<'_>,
    metas: &mut Metas<'_>,
    document: &Document,
    (root, meta): (usize, Meta),
) -> Vec<Implementation> {
    let Some(node) = reader.field(root, "implementations", "implementations") else {
        return Vec::new();
    };
    let declared = metas.field(meta, "implementations", Some(node));
    let Some(N::Mapping(entries)) = document.nodes().get(node) else {
        reader.fault("implementations", node, Kind::NormalizedShape);
        return Vec::new();
    };
    let mut implementations = Vec::new();
    for &(key, value) in entries {
        let host = reader.text(key, "implementations");
        let Some(language) = [Language::Python, Language::R]
            .into_iter()
            .find(|language| language.key() == host)
        else {
            reader.fault(
                &format!("implementations.{host}"),
                key,
                Kind::NormalizedShape,
            );
            continue;
        };
        let path = format!("implementations.{host}");
        implementations.push(Implementation {
            language,
            callable: Text {
                text: reader.text(value, &path),
                meta: metas.field(declared, language.key(), Some(value)),
            },
        });
    }
    implementations
}

/// Lower a codelist file: one codelist source under its own header (CT-1,
/// P-TYPE-1).
pub fn codelists(
    document: &Document,
    positions: Option<&Positions>,
) -> Result<Codelists, Vec<Finding>> {
    let root = document.root();
    let mut paths = Paths::new();
    let mut metas = Metas::new(positions, &mut paths);
    let meta = metas.root(root);
    let mut reader = Reader::new(document);
    let at = (root, meta);
    let header = header(&mut reader, &mut metas, document, at);
    let (standard, codelists) = codelist_source(&mut reader, &mut metas, document, at);
    let findings = reader.finish();
    if !findings.is_empty() {
        return Err(findings);
    }
    Ok(Codelists {
        header: Some(header),
        standard,
        codelists,
        paths,
        meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{CodelistSource, DefineContext, GuideStatus, Notation, Parameter, Transport};
    use crate::test_document::Tree;
    use crate::value::{ColumnType, Value, ValueType};

    fn header(file_type: &str) -> Vec<(&'static str, Tree)> {
        vec![
            ("yamaa", Tree::text("1.0")),
            ("type", Tree::text(file_type)),
        ]
    }

    fn with_header(file_type: &str, fields: Vec<(&'static str, Tree)>) -> Tree {
        Tree::map(header(file_type).into_iter().chain(fields).collect())
    }

    fn codelist_source() -> Vec<(&'static str, Tree)> {
        vec![
            (
                "standard",
                Tree::map(vec![
                    ("name", Tree::text("CDISC/NCI")),
                    ("publishing_set", Tree::text("SDTM")),
                    ("version", Tree::text("2024-03-29")),
                ]),
            ),
            (
                "codelists",
                Tree::list(vec![Tree::map(vec![
                    ("id", Tree::text("SEX")),
                    ("name", Tree::text("Sex")),
                    ("data_type", Tree::text("text")),
                    ("extensible", Tree::boolean(false)),
                    (
                        "items",
                        Tree::list(vec![Tree::map(vec![
                            ("value", Tree::text("F")),
                            ("extended", Tree::boolean(false)),
                        ])]),
                    ),
                ])]),
            ),
        ]
    }

    /// A definition's body, which both surfaces write alike.
    fn body(default: Tree, argument: Tree, result: Tree) -> Vec<(&'static str, Tree)> {
        vec![
            ("description", Tree::text("Days from a date")),
            (
                "params",
                Tree::list(vec![Tree::map(vec![
                    ("name", Tree::text("start")),
                    ("type", Tree::text("date")),
                    ("required", Tree::boolean(false)),
                    ("default", default),
                    ("accepts_missing", Tree::boolean(false)),
                ])]),
            ),
            ("returns", Tree::text("date")),
            ("may_return_missing", Tree::boolean(false)),
            ("comparison_decimals", Tree::integer(4)),
        ]
        .into_iter()
        .chain([(
            "tests",
            Tree::list(vec![Tree::map(vec![
                ("id", Tree::text("ordinary")),
                (
                    "covers",
                    Tree::list(vec![Tree::text("normal"), Tree::text("boundary")]),
                ),
                ("args", Tree::map(vec![("start", argument)])),
                ("result", result),
            ])]),
        )])
        .collect()
    }

    fn function_file(default: Tree, argument: Tree, result: Tree) -> Tree {
        let mut fields = vec![("name", Tree::text("shift"))];
        fields.extend(body(default, argument, result));
        // `implementations` is written before `tests`, as the converter does.
        let tests = fields.pop().unwrap();
        fields.push((
            "implementations",
            Tree::map(vec![
                ("r", Tree::text("shift::shift")),
                ("python", Tree::text("shift.shift")),
            ]),
        ));
        fields.push(tests);
        with_header("function", fields)
    }

    fn date(text: &str) -> Value {
        Value::Date(text.parse().unwrap())
    }

    #[test]
    fn a_project_lists_function_files_and_holds_each_host_lock() {
        let tree = with_header(
            "project",
            vec![
                (
                    "functions",
                    Tree::list(vec![
                        Tree::text("functions/bmi.yaml"),
                        Tree::text("functions/shift.yaml"),
                    ]),
                ),
                (
                    "environments",
                    Tree::map(vec![(
                        "r",
                        Tree::map(vec![("lock", Tree::text("r/renv.lock"))]),
                    )]),
                ),
                (
                    "codelists",
                    Tree::list(vec![Tree::text("ct.yaml"), Tree::map(codelist_source())]),
                ),
                (
                    "adam",
                    Tree::map(vec![
                        ("specs", Tree::list(vec![Tree::text("adsl.yaml")])),
                        (
                            "standards",
                            Tree::list(vec![
                                Tree::map(vec![
                                    ("name", Tree::text("ADaMIG")),
                                    ("version", Tree::text("1.3")),
                                    ("status", Tree::text("Final")),
                                ]),
                                Tree::map(vec![
                                    ("name", Tree::text("ADaMIG-MD")),
                                    ("version", Tree::text("1.0")),
                                    ("status", Tree::text("Provisional")),
                                ]),
                            ]),
                        ),
                        ("transport", Tree::text("dataset_json")),
                        ("context", Tree::text("Submission")),
                        ("xml_lang", Tree::text("en")),
                    ]),
                ),
            ],
        );
        let project = project(&tree.document(), None).unwrap();
        let path = |meta: Meta| project.paths.render(meta.path, Notation::V0_5);
        assert_eq!(project.header.surface, Surface::V1 { minor: 0 });
        assert_eq!(project.header.version.text, "1.0");
        assert_eq!(path(project.header.meta), "yamaa");
        // A function file names its own function, so its declaration has no
        // key; it is relative to the project file.
        let functions = project.functions.as_ref().unwrap();
        assert_eq!(functions.len(), 2);
        assert_eq!(functions[1].key, None);
        let FunctionSource::File(written) = &functions[1].source else {
            panic!("a function file")
        };
        assert_eq!(written.text, "functions/shift.yaml");
        assert_eq!(path(functions[1].meta), "functions[1]");
        // Only the host that declares a lock has an entry, and it holds no
        // environment file.
        assert!(project.environment(Language::Python).is_none());
        let r = project.environment(Language::R).unwrap();
        assert_eq!(r.file, None);
        let lock = r.lock.as_ref().unwrap();
        assert_eq!(lock.text, "r/renv.lock");
        assert_eq!(path(lock.meta), "environments.r.lock");
        assert!(matches!(
            &project.codelists[0].source,
            CodelistSource::File(path) if path.text == "ct.yaml"
        ));
        let CodelistSource::Inline(inline) = &project.codelists[1].source else {
            panic!("an inline source")
        };
        assert_eq!(inline.header, None);
        assert_eq!(inline.codelists[0].id.name, "SEX");
        // A section's `standards` is a list, its first entry the default, and
        // `transport` names the transport (D16, P-PRJ-1).
        let sections = &project.submission.as_ref().unwrap().sections;
        let adam = &sections[0];
        assert_eq!(
            adam.standards
                .iter()
                .map(|guide| (guide.name.text.as_str(), guide.status))
                .collect::<Vec<_>>(),
            [
                ("ADaMIG", GuideStatus::Final),
                ("ADaMIG-MD", GuideStatus::Provisional)
            ]
        );
        assert_eq!(path(adam.standards[1].meta), "adam.standards[1]");
        assert_eq!(adam.transport, Some(Transport::DatasetJson));
        assert_eq!(adam.context, DefineContext::Submission);
    }

    #[test]
    fn a_section_keeps_to_its_own_surface_spellings() {
        // 0.5's `standard` and `datasets` are not 1.0 section fields; the
        // schema refuses them first, and the section lowering refuses them
        // too.
        for (field, value) in [
            (
                "standard",
                Tree::map(vec![
                    ("name", Tree::text("SDTMIG")),
                    ("version", Tree::text("3.4")),
                    ("status", Tree::text("Final")),
                ]),
            ),
            ("datasets", Tree::text("xpt")),
            ("transport", Tree::text("json")),
        ] {
            let tree = with_header(
                "project",
                vec![(
                    "sdtm",
                    Tree::map(vec![
                        ("specs", Tree::list(vec![Tree::text("dm.yaml")])),
                        (field, value),
                    ]),
                )],
            );
            let project = project(&tree.document(), None).unwrap();
            assert_eq!(project.submission, Err(SubmissionRefused), "{field}");
        }
    }

    #[test]
    fn define_requires_a_1_0_section_s_standards_by_their_1_0_name() {
        // An omitted list and an empty one both leave the section without the
        // guide Define-XML composition requires (DEFINE-28).
        for standards in [None, Some(Tree::list(vec![]))] {
            let mut section = vec![
                ("specs", Tree::list(vec![Tree::text("adsl.yaml")])),
                ("define", Tree::text("define.xml")),
            ];
            section.extend(standards.map(|list| ("standards", list)));
            let tree = with_header(
                "project",
                vec![
                    (
                        "study",
                        Tree::map(vec![
                            ("id", Tree::text("STUDY01")),
                            ("name", Tree::text("STUDY01")),
                            ("description", Tree::text("Study")),
                            ("protocol_name", Tree::text("STUDY-01")),
                        ]),
                    ),
                    ("adam", Tree::map(section)),
                ],
            );
            let project = project(&tree.document(), None).unwrap();
            let declarations = project.submission.as_ref().unwrap();
            let Err(findings) = crate::submission_composition::Required::admit(
                declarations.study.as_ref(),
                declarations.sections.first(),
                crate::project_environment::Submission::Adam,
                project.header.surface,
            ) else {
                panic!("a section without a guide")
            };
            assert_eq!(
                findings
                    .iter()
                    .map(|finding| finding.path.as_str())
                    .collect::<Vec<_>>(),
                ["adam.standards"]
            );
        }
    }

    #[test]
    fn define_names_a_1_0_section_s_guide_by_its_place_in_standards() {
        // The guide composition binds is the first of `standards` (D16); one
        // outside the section's model is reported there (SUB-3).
        let tree = with_header(
            "project",
            vec![
                (
                    "study",
                    Tree::map(vec![
                        ("id", Tree::text("STUDY01")),
                        ("name", Tree::text("STUDY01")),
                        ("description", Tree::text("Study")),
                        ("protocol_name", Tree::text("STUDY-01")),
                    ]),
                ),
                (
                    "sdtm",
                    Tree::map(vec![
                        ("specs", Tree::list(vec![Tree::text("dm.yaml")])),
                        (
                            "standards",
                            Tree::list(vec![Tree::map(vec![
                                ("name", Tree::text("ADaMIG")),
                                ("version", Tree::text("1.3")),
                                ("status", Tree::text("Final")),
                            ])]),
                        ),
                        ("define", Tree::text("out/define.xml")),
                    ]),
                ),
            ],
        );
        let project = project(&tree.document(), None).unwrap();
        let declarations = project.submission.as_ref().unwrap();
        let required = crate::submission_composition::Required::admit(
            declarations.study.as_ref(),
            declarations.sections.first(),
            crate::project_environment::Submission::Sdtm,
            project.header.surface,
        )
        .unwrap();
        let Err(findings) = crate::submission_composition::compose(
            required,
            &crate::project_terminology::Catalogue::from_admitted(Vec::new()),
            &[],
            &[],
            "1.0",
            "2026-10-20T00:00:00",
        ) else {
            panic!("a guide of another model")
        };
        // A 1.0 section reports it as `standard_model_mismatch` (D58), naming
        // the guide, the section and the section's model.
        let finding = findings
            .iter()
            .find(|finding| finding.path == "sdtm.standards[0].name")
            .unwrap_or_else(|| panic!("{findings:?}"));
        assert_eq!(
            finding.code,
            crate::diagnostic::ConditionCode::SubmissionModelMismatch
        );
        let text = |value: &str| crate::json::Value::String(value.into());
        assert_eq!(
            finding.context,
            [
                ("model".to_string(), text("SDTM")),
                ("section".to_string(), text("sdtm")),
                ("value".to_string(), text("ADaMIG")),
            ]
            .into()
        );
    }

    #[test]
    fn a_function_file_names_its_function_and_types_its_values_by_declaration() {
        let tree = function_file(
            Tree::text("2024-01-01"),
            Tree::text("2024-02-29"),
            Tree::text("2024-03-01"),
        );
        let function = function(&tree.document(), None).unwrap();
        let path = |meta: Meta| function.paths.render(meta.path, Notation::V0_5);
        let header = function.header.as_ref().unwrap();
        assert_eq!(header.surface, Surface::V1 { minor: 0 });
        assert_eq!(function.name.name, "shift");
        assert_eq!(path(function.name.meta), "name");
        // Each host language's callable, in written order.
        assert_eq!(
            function
                .implementations
                .iter()
                .map(|implementation| (
                    implementation.language,
                    implementation.callable.text.as_str()
                ))
                .collect::<Vec<_>>(),
            [
                (Language::R, "shift::shift"),
                (Language::Python, "shift.shift")
            ]
        );
        let python = function.implementation(Language::Python).unwrap();
        assert_eq!(path(python.callable.meta), "implementations.python");
        // A date is written as text and typed by the declared parameter and
        // return types (P-FN-1).
        assert_eq!(function.params[0].default, Some(date("2024-01-01")));
        assert_eq!(function.tests[0].args[0].1, date("2024-02-29"));
        assert_eq!(function.tests[0].result, date("2024-03-01"));
        assert_eq!(
            path(function.tests[0].args[0].0.meta),
            "tests[0].args.start"
        );
        assert_eq!(path(function.params[0].meta), "params[0]");
    }

    #[test]
    fn a_value_its_declared_type_cannot_read_is_a_finding_with_its_text() {
        let mut tree = function_file(
            Tree::text("2024-02-30"),
            Tree::integer("9223372036854775808"),
            Tree::text("2024-03-01"),
        );
        tree.set(&["returns"], Tree::text("datetime"));
        let findings = function(&tree.document(), None).unwrap_err();
        let found = findings
            .iter()
            .map(|finding| {
                (
                    finding.path.as_str(),
                    finding.kind,
                    finding.value.as_deref(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            found,
            [
                ("params[0].default", Kind::InvalidDate, Some("2024-02-30")),
                (
                    "tests[0].args.start",
                    Kind::IntegerRange,
                    Some("9223372036854775808")
                ),
                ("tests[0].result", Kind::InvalidDateTime, Some("2024-03-01")),
            ]
        );
        // A tagged value is a 0.5 spelling, which the 1.0 schema refuses: no
        // value of a function file is a mapping.
        let tagged = function_file(
            Tree::map(vec![("date", Tree::text("2024-01-01"))]),
            Tree::text("2024-02-29"),
            Tree::text("2024-03-01"),
        );
        let findings = function(&tagged.document(), None).unwrap_err();
        assert_eq!(findings.len(), 1);
        assert_eq!(
            (findings[0].path.as_str(), findings[0].kind),
            ("params[0].default", Kind::NormalizedShape)
        );
    }

    /// What a definition declares, without its positions.
    #[allow(clippy::type_complexity)]
    fn declared(
        function: &Function,
    ) -> (
        &str,
        &str,
        Vec<(&str, ValueType, bool, Option<&Value>, bool)>,
        ColumnType,
        bool,
        i64,
        Vec<(&str, Vec<&str>, Vec<(&str, &Value)>, &Value)>,
    ) {
        (
            &function.name.name,
            &function.description.text,
            function
                .params
                .iter()
                .map(|parameter: &Parameter| {
                    (
                        parameter.name.name.as_str(),
                        parameter.kind,
                        parameter.required,
                        parameter.default.as_ref(),
                        parameter.accepts_missing,
                    )
                })
                .collect(),
            function.returns,
            function.may_return_missing,
            function.comparison_decimals,
            function
                .tests
                .iter()
                .map(|case| {
                    (
                        case.id.text.as_str(),
                        case.covers.iter().map(|tag| tag.text.as_str()).collect(),
                        case.args
                            .iter()
                            .map(|(name, value)| (name.name.as_str(), value))
                            .collect(),
                        &case.result,
                    )
                })
                .collect(),
        )
    }

    #[test]
    fn a_function_file_lowers_to_the_definition_its_0_5_spelling_does() {
        let tagged = |text: &str| Tree::map(vec![("date", Tree::text(text))]);
        let mut legacy = vec![("function", Tree::text("shift.shift"))];
        legacy.extend(body(
            tagged("2024-01-01"),
            tagged("2024-02-29"),
            tagged("2024-03-01"),
        ));
        let legacy = crate::front_end::v0_5::project::function(
            &Tree::map(legacy).document(),
            None,
            "shift",
            Language::Python,
        )
        .unwrap();
        let converted = function(
            &function_file(
                Tree::text("2024-01-01"),
                Tree::text("2024-02-29"),
                Tree::text("2024-03-01"),
            )
            .document(),
            None,
        )
        .unwrap();
        assert_eq!(declared(&converted), declared(&legacy));
        // The 0.5 definition binds the one callable of its environment file's
        // language; the function file names both.
        assert_eq!(
            converted
                .implementation(Language::Python)
                .map(|i| &i.callable.text),
            legacy
                .implementation(Language::Python)
                .map(|i| &i.callable.text)
        );
        assert_eq!(legacy.implementation(Language::R), None);
        assert_eq!(legacy.header, None);
    }

    #[test]
    fn a_codelist_file_holds_one_source_under_its_own_header() {
        let file = codelists(
            &with_header("codelists", codelist_source()).document(),
            None,
        )
        .unwrap();
        let header = file.header.as_ref().unwrap();
        assert_eq!(file.paths.render(header.meta.path, Notation::V0_5), "yamaa");
        let legacy = crate::front_end::v0_5::project::codelists(
            &Tree::map(codelist_source()).document(),
            None,
        )
        .unwrap();
        assert_eq!(legacy.header, None);
        assert_eq!(
            file.standard
                .as_ref()
                .map(|standard| &standard.version.text),
            legacy
                .standard
                .as_ref()
                .map(|standard| &standard.version.text)
        );
        assert_eq!(file.codelists.len(), legacy.codelists.len());
        let item = &file.codelists[0].items.as_ref().unwrap()[0];
        assert_eq!(item.value, Value::Str("F".into()));
        assert_eq!(
            file.paths.render(item.meta.path, Notation::V0_5),
            "codelists[0].items[0]"
        );
    }
}
