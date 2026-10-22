//! Lower today's project, environment, function and codelist files into the
//! project roots of [`crate::ast::project`].
//!
//! A spec lowers totally and its analyses refuse what they cannot read. These
//! files are read as before instead: every written scalar of a definition or
//! a codelist converts to the value the engine runs with, and lowering
//! reports each one that does not, and each node whose shape the schema
//! should have refused, with the file's path to it. Those findings decide
//! preparation; nothing after lowering reads the file.
//!
//! A 0.5 project links an environment file for each host language. The one
//! its host reads lowers into the project root ([`environment`]): its lock
//! into that language's environment and its definitions into
//! [`Project::functions`]. Each of its paths keeps the environment file's
//! own spelling, `lock` and `functions.<name>`.
//!
//! What both surfaces write alike, codelist sources and a definition's
//! parameters and tests, lowers in [`crate::front_end::lowering::project`],
//! and the study and sections in [`crate::front_end::lowering::sections`].

use crate::ast::{
    Codelists, Function, FunctionDeclaration, FunctionSource, Header, HostEnvironment,
    Implementation, Meta, Named, Paths, Project, Surface, Text,
};
use crate::front_end::lowering::project::{
    cases, codelist_declarations, codelists_at, column_kind, comparison_decimals, parameters,
    prefix, text_field, text_of, Metas, Values,
};
use crate::front_end::lowering::{sections, Positions};
use crate::project_function::Language;
use crate::project_function_document::{Finding, Kind, Reader};
use crate::schema::{Document, DocumentNode as N};

/// `schema_version`, which the schema requires and nothing else reads.
fn header(document: &Document, metas: &mut Metas<'_>, root: usize, meta: Meta) -> Header {
    let node = document.field(root, "schema_version");
    let version = Text {
        text: node
            .and_then(|node| text_of(document, node))
            .unwrap_or_default()
            .into(),
        meta: match node {
            Some(node) => metas.field(meta, "schema_version", Some(node)),
            None => meta,
        },
    };
    Header {
        surface: Surface::V0_5,
        meta: version.meta,
        version,
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
    let header = header(document, &mut metas, root, meta);
    let mut environments = Vec::new();
    if let Some(node) = document.field(root, "environments") {
        let declared = metas.field(meta, "environments", Some(node));
        for language in [Language::Python, Language::R] {
            let key = language.key();
            let written = reader.optional_text(node, key, &format!("environments.{key}"));
            if let Some(written) = written {
                let meta = metas.field(declared, key, document.field(node, key));
                environments.push(HostEnvironment {
                    language,
                    file: Some(Text {
                        text: written,
                        meta,
                    }),
                    lock: None,
                    meta,
                });
            }
        }
    }
    let mut leaves = Vec::new();
    let codelists = codelist_declarations(
        document,
        positions,
        (&mut reader, &mut metas),
        (root, meta),
        &mut leaves,
    );
    let submission = sections::lower(document, root, &mut metas, meta, Surface::V0_5)
        .map_err(|_| crate::ast::SubmissionRefused);
    let mut findings = reader.finish();
    findings.extend(leaves);
    if findings.is_empty() {
        Ok(Project {
            header,
            functions: None,
            environments,
            codelists,
            submission,
            paths,
            meta,
        })
    } else {
        Err(findings)
    }
}

/// Lower the environment file a 0.5 project links for `language` into the
/// project root: its lock into that language's environment and its
/// definitions into [`Project::functions`]. On a finding the root is left
/// as it was.
pub fn environment(
    document: &Document,
    positions: Option<&Positions>,
    language: Language,
    project: &mut Project,
) -> Result<(), Vec<Finding>> {
    let root = document.root();
    let mut paths = project.paths.clone();
    let mut metas = Metas::new(positions, &mut paths);
    let meta = metas.root(root);
    let mut reader = Reader::new(document);
    if !matches!(document.nodes().get(root), Some(N::Mapping(_))) {
        reader.fault("", root, Kind::NormalizedShape);
    }
    let lock = reader.optional_text(root, "lock", "lock").map(|text| Text {
        text,
        meta: metas.field(meta, "lock", document.field(root, "lock")),
    });
    let mut leaves = Vec::new();
    let functions = if let Some(node) = document.field(root, "functions") {
        let declared = metas.field(meta, "functions", Some(node));
        let mut functions = Vec::new();
        match document.nodes().get(node) {
            Some(N::Mapping(entries)) => {
                for &(key, value) in entries {
                    let name = reader.text(key, "functions");
                    let path = format!("functions.{name}");
                    let meta = metas.key(declared, &name, Some(value));
                    let source = match document.nodes().get(value) {
                        Some(N::Text(written)) => Some(FunctionSource::File(Text {
                            text: written.clone(),
                            meta,
                        })),
                        Some(N::Mapping(_)) => {
                            match function_at(document, positions, value, &name, language) {
                                Ok(definition) => {
                                    Some(FunctionSource::Inline(Box::new(definition)))
                                }
                                Err(findings) => {
                                    leaves.extend(prefix(findings, &path));
                                    None
                                }
                            }
                        }
                        _ => {
                            reader.fault(&path, value, Kind::NormalizedShape);
                            None
                        }
                    };
                    if let Some(source) = source {
                        let key = Named {
                            name,
                            meta: metas.at(meta, key),
                        };
                        functions.push(FunctionDeclaration {
                            key: Some(key),
                            source,
                            meta,
                        });
                    }
                }
            }
            _ => reader.fault("functions", node, Kind::NormalizedShape),
        }
        Some(functions)
    } else {
        None
    };
    let mut findings = reader.finish();
    findings.extend(leaves);
    if !findings.is_empty() {
        return Err(findings);
    }
    project.paths = paths;
    match project
        .environments
        .iter_mut()
        .find(|environment| environment.language == language)
    {
        Some(environment) => environment.lock = lock,
        None => project.environments.push(HostEnvironment {
            language,
            file: None,
            lock,
            meta,
        }),
    }
    project.functions = functions;
    Ok(())
}

/// Lower a function file that a 0.5 environment file for `language` keys by
/// `name`.
pub fn function(
    document: &Document,
    positions: Option<&Positions>,
    name: &str,
    language: Language,
) -> Result<Function, Vec<Finding>> {
    function_at(document, positions, document.root(), name, language)
}

/// Lower a codelist file.
pub fn codelists(
    document: &Document,
    positions: Option<&Positions>,
) -> Result<Codelists, Vec<Finding>> {
    codelists_at(document, positions, document.root())
}

/// Decode all scalar leaves before returning any definition. Failed values
/// keep their exact node references; no partial definition is returned.
fn function_at(
    document: &Document,
    positions: Option<&Positions>,
    root: usize,
    name: &str,
    language: Language,
) -> Result<Function, Vec<Finding>> {
    let mut paths = Paths::new();
    let mut metas = Metas::new(positions, &mut paths);
    let meta = metas.root(root);
    let mut reader = Reader::new(document);
    let at = (root, meta);
    let callable = text_field(
        &mut reader,
        &mut metas,
        document,
        at,
        "function",
        "function",
    );
    let description = text_field(
        &mut reader,
        &mut metas,
        document,
        at,
        "description",
        "description",
    );
    let params = parameters(&mut reader, &mut metas, document, at, Values::Tagged);
    let returns = column_kind(&mut reader, root, "returns");
    let may_return_missing = reader.boolean(root, "may_return_missing", "may_return_missing");
    let comparison_decimals = comparison_decimals(&mut reader, root);
    let tests = cases(
        &mut reader,
        &mut metas,
        document,
        at,
        (&params, returns, Values::Tagged),
    );
    let findings = reader.finish();
    if !findings.is_empty() {
        return Err(findings);
    }
    Ok(Function {
        header: None,
        name: Named {
            name: name.into(),
            meta,
        },
        description,
        params,
        returns,
        may_return_missing,
        comparison_decimals,
        implementations: vec![Implementation { language, callable }],
        tests,
        paths,
        meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{CodelistSource, FunctionSource, Notation, Parameter};
    use crate::front_end::v1::project::tests::{body, codelist_source, function_file, with_header};
    use crate::test_document::Tree;
    use crate::value::{ColumnType, Value, ValueType};

    fn definition(result: Tree) -> Tree {
        Tree::map(vec![
            ("function", Tree::text("project.identity")),
            ("description", Tree::text("Identity")),
            (
                "params",
                Tree::list(vec![Tree::map(vec![
                    ("name", Tree::text("x")),
                    ("type", Tree::text("int")),
                    ("required", Tree::boolean(false)),
                    ("default", Tree::integer(1)),
                    ("accepts_missing", Tree::boolean(false)),
                ])]),
            ),
            ("returns", Tree::text("int")),
            ("may_return_missing", Tree::boolean(false)),
            ("comparison_decimals", Tree::integer(4)),
            (
                "tests",
                Tree::list(vec![Tree::map(vec![
                    ("id", Tree::text("ordinary")),
                    (
                        "covers",
                        Tree::list(vec![Tree::text("normal"), Tree::text("boundary")]),
                    ),
                    ("args", Tree::map(vec![("x", Tree::integer(2))])),
                    ("result", result),
                ])]),
            ),
        ])
    }
    fn project_tree() -> Tree {
        Tree::map(vec![
            ("schema_version", Tree::text("1.0")),
            (
                "environments",
                Tree::map(vec![("python", Tree::text("python/environment.yaml"))]),
            ),
            (
                "codelists",
                Tree::list(vec![
                    Tree::text("ct.yaml"),
                    Tree::map(vec![(
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
                    )]),
                ]),
            ),
        ])
    }

    #[test]
    fn every_lowered_node_renders_the_path_its_diagnostics_cite() {
        let function = function(
            &definition(Tree::integer(2)).document(),
            None,
            "identity",
            Language::Python,
        )
        .unwrap();
        let path = |meta: Meta| function.paths.render(meta.path, Notation::V0_5);
        assert_eq!(path(function.params[0].meta), "params[0]");
        assert_eq!(path(function.params[0].name.meta), "params[0].name");
        assert_eq!(path(function.tests[0].id.meta), "tests[0].id");
        assert_eq!(path(function.tests[0].covers[1].meta), "tests[0].covers[1]");
        assert_eq!(path(function.tests[0].args[0].0.meta), "tests[0].args.x");
        assert_eq!(path(function.implementations[0].callable.meta), "function");
        assert_eq!(function.name.name, "identity");
        assert_eq!(function.implementation(Language::R), None);
        let project = project(&project_tree().document(), None).unwrap();
        let path = |meta: Meta| project.paths.render(meta.path, Notation::V0_5);
        let python = project.environment(Language::Python).unwrap();
        assert_eq!(path(python.meta), "environments.python");
        assert!(project.environment(Language::R).is_none());
        assert_eq!(path(project.codelists[1].meta), "codelists[1]");
        let CodelistSource::Inline(source) = &project.codelists[1].source else {
            panic!("inline source")
        };
        // An inline source is a root of its own; diagnostics prefix its
        // declaration.
        let item = &source.codelists[0].items.as_ref().unwrap()[0];
        assert_eq!(
            source.paths.render(item.meta.path, Notation::V0_5),
            "codelists[0].items[0]"
        );
        assert_eq!(
            source
                .paths
                .render(source.codelists[0].id.meta.path, Notation::V0_5),
            "codelists[0].id"
        );
    }

    #[test]
    fn an_environment_file_that_fails_leaves_the_project_root_as_it_was() {
        let mut project = project(&project_tree().document(), None).unwrap();
        let before = project.clone();
        let environment = Tree::map(vec![
            ("schema_version", Tree::text("1.0")),
            ("lock", Tree::text("uv.lock")),
            (
                "functions",
                Tree::map(vec![
                    ("first", Tree::text("first.yaml")),
                    ("wide", definition(Tree::integer("9223372036854775808"))),
                ]),
            ),
        ]);
        let findings = super::environment(
            &environment.clone().document(),
            None,
            Language::Python,
            &mut project,
        )
        .unwrap_err();
        assert_eq!(project, before);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].path, "functions.wide.tests[0].result");
        assert_eq!(findings[0].value.as_deref(), Some("9223372036854775808"));
        // Once it lowers, its lock and definitions keep its own paths.
        let mut fixed = environment;
        fixed.set(&["functions", "wide"], definition(Tree::integer(2)));
        super::environment(&fixed.document(), None, Language::Python, &mut project).unwrap();
        let lock = project
            .environment(Language::Python)
            .unwrap()
            .lock
            .as_ref()
            .unwrap();
        assert_eq!(lock.text, "uv.lock");
        assert_eq!(project.paths.render(lock.meta.path, Notation::V0_5), "lock");
        let functions = project.functions.as_ref().unwrap();
        assert!(
            matches!(&functions[0].source, FunctionSource::File(path) if path.text == "first.yaml")
        );
        let FunctionSource::Inline(definition) = &functions[1].source else {
            panic!("inline definition")
        };
        assert_eq!(definition.name.name, "wide");
        assert_eq!(
            project.paths.render(functions[1].meta.path, Notation::V0_5),
            "functions.wide"
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
        let legacy = function(
            &Tree::map(legacy).document(),
            None,
            "shift",
            Language::Python,
        )
        .unwrap();
        let converted = crate::front_end::v1::project::function(
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
    fn a_codelist_file_lowers_to_the_source_its_0_5_spelling_does() {
        let file = crate::front_end::v1::project::codelists(
            &with_header("codelists", codelist_source()).document(),
            None,
        )
        .unwrap();
        let legacy = codelists(&Tree::map(codelist_source()).document(), None).unwrap();
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
    }
}
