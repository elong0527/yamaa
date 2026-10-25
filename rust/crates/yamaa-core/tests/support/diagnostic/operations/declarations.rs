//! The declarations' refusals. A specification's intermediates, record
//! operations and flags, its producer links, copy entries and project
//! functions, a project's scope findings, the schema language's validation
//! of a document or a layer, and the headers a call reads.
use super::{
    codelist_source, column_type,
    document::{prepared, schemas, tree},
    language, text, value, File,
};
use crate::support::{
    diagnostic::{json::Json, Record},
    project_call_compiler::definition,
    resolved::intermediates,
    specification::{
        prepare, prepare_in as prepare_in_environment, prepare_linked as prepare_linked_with,
    },
};
use std::{collections::BTreeMap, convert::Infallible};
use yamaa_core::{
    copy_expansion::{producer_cycle, Copies},
    dataset::flag_condition_diagnostic,
    diagnostic::{Diagnostic, InheritanceFinding},
    expression::{compile, Context, Resolver},
    inheritance::Named,
    json::{Number, Value as JsonValue},
    language::{
        read_declared_header, read_header, unknown_file_type, FileType, LanguageVersion, PathBase,
        Reference, ReferenceFault, LANGUAGE_VERSION,
    },
    producer_contract::{prepare_document, Link, Links},
    project_environment::{Draft, Environment},
    project_function_document::Kind,
    project_scope,
    run_report::{Recorded, Run},
    schema::{
        admit_schema_diagnostics, schema_diagnostic_kind_label, Document, DocumentNode,
        NormalizationError, SchemaContext, SchemaDiagnostic, SchemaDiagnosticValue, SchemaModule,
        SchemaStructure,
    },
    specification::{
        BindError, FunctionCause, FunctionFinding, PreflightFinding, PrepareError,
        PreparedSpecification,
    },
    submission_staleness::Guard,
    table::TableSchema,
    value::{Selection, Value},
};

pub(super) const FILES: &[File] = &[
    fixture!("function_arguments.json"),
    fixture!("intermediates.json"),
    fixture!("language.json"),
    fixture!("producers.json"),
    fixture!("projects.json"),
    fixture!("records.json"),
    fixture!("rows.json"),
    fixture!("schema.json"),
];

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "copy_columns" => copy_columns(input),
        "copy_cycle" => copy_cycle(input),
        "copy_datasets" => copy_datasets(input),
        "file_type_argument" => file_type_argument(input),
        "flag_condition" => flag_condition(input),
        "function_finding" => function_finding(input),
        "language_header" => language_header(input),
        "layer_header" => layer_header(input),
        "prepare_in" => prepare_in(input),
        "prepare_linked" => prepare_linked(input),
        "prepare_or_bind" => prepare_or_bind(input),
        "producer_header" => producer_header(input),
        "project_scope" => scope_finding(input),
        "reference_check" => reference_check(input),
        "reference_refuse" => reference_refuse(input),
        "schema_document" => schema_document(input),
        "schema_layer" => schema_layer(input),
        "schema_types" => schema_types(input),
        "schema_windows" => schema_windows(input),
        "staleness" => staleness(input),
        "unique_verification" => unique_verification(input),
        _ => return None,
    })
}

/// The findings preparation refuses `document` with, else those binding it
/// to `sources` refuses with, `{document, sources}`: the reading of a table
/// that did not say which of the two refuses.
fn prepare_or_bind(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["document", "sources"]);
    match prepare(&tree(input.field("document")).document()) {
        Ok(prepared) => bound(&prepared, &schemas(input)),
        Err(error) => refusal(error),
    }
}

/// The findings a preparation refuses with.
fn refusal(error: PrepareError) -> Vec<Diagnostic> {
    match error {
        PrepareError::Invalid(findings) => {
            findings.iter().map(PreflightFinding::diagnostic).collect()
        }
        other => panic!("preparation refuses with no language finding: {other:?}"),
    }
}

/// The findings binding `prepared` to `schemas` refuses with.
fn bound(prepared: &PreparedSpecification, schemas: &[TableSchema]) -> Vec<Diagnostic> {
    match prepared.bind_sources(&schemas.iter().collect::<Vec<_>>()) {
        Ok(_) => Vec::new(),
        Err(BindError::Invalid(findings)) => findings
            .iter()
            .flat_map(|finding| {
                finding
                    .diagnostics(prepared.source())
                    .expect("a finding with its diagnostics")
            })
            .collect(),
        Err(other) => panic!("binding refuses with no language finding: {other:?}"),
    }
}

/// The finding of the first unique verification of the first intermediate
/// of `document`, which binds to `sources`, when the input `dataset` holds
/// `duplicate_count` records that share its columns, `{document, sources,
/// dataset, duplicate_count}` (LOOKUP-17).
fn unique_verification(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["document", "sources", "dataset", "duplicate_count"]);
    let schemas = schemas(input);
    let dataset = input.field("dataset").text();
    let source = input
        .field("sources")
        .members()
        .iter()
        .position(|(name, _)| name == dataset)
        .unwrap_or_else(|| panic!("{dataset} is one of the sources"));
    let plan = prepared(input)
        .bind_sources(&schemas.iter().collect::<Vec<_>>())
        .unwrap_or_else(|error| panic!("the document binds: {error:?}"));
    let item = intermediates(&plan)[0];
    vec![item.unique[0].diagnostic(
        item,
        &schemas[source],
        dataset,
        input.field("duplicate_count").integer(),
    )]
}

/// The finding a flag's condition reports when its evaluation fails,
/// `{expression, path, value}`, where every name the condition reads holds
/// `value` (EXPR-8).
fn flag_condition(input: &Json) -> Vec<Diagnostic> {
    struct Every(Value);
    impl Resolver for Every {
        type Error = Infallible;
        fn resolve(&mut self, _: &str) -> Result<Selection, Infallible> {
            Ok(Selection::Present(self.0.clone()))
        }
    }
    let input = input.closed(&["expression", "path", "value"]);
    let plan = compile(
        text(input, "expression"),
        Context::Predicate,
        text(input, "path"),
    )
    .expect("a condition that compiles");
    match plan.evaluate_truth(&mut Every(value(input.field("value")))) {
        Ok(_) => Vec::new(),
        Err(error) => vec![flag_condition_diagnostic(&error).expect("a flag's finding")],
    }
}

/// The findings preparing `document` in a project refuses with, `{document,
/// project}`: `project` is `{language, functions, codelists}`, the host
/// language (`"python"`, `"r"` or `null` for an engine that claims none),
/// each function file by its name, an integer identity with the tests its
/// contract needs (`support::project_call_compiler::definition`), mapping
/// each language to the callable that implements it, and its codelist files,
/// each the codelist source [`codelist_source`] reads. A project without
/// `functions` lists no function file, and one without `codelists` no
/// codelist file. With `sources`, as `bind` reads them, the document must
/// prepare, and the findings are those binding it to their schemas refuses
/// with.
fn prepare_in(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["document", "project", "sources"]);
    let project = input
        .field("project")
        .closed(&["language", "functions", "codelists"]);
    let host = match project.field("language") {
        Json::Null => None,
        written => Some(language(written.text())),
    };
    let functions = project.get("functions").map(|files| {
        files
            .members()
            .iter()
            .map(|(name, implementations)| {
                let implementations = implementations
                    .members()
                    .iter()
                    .map(|(written, callable)| (language(written), callable.text()))
                    .collect::<Vec<_>>();
                definition(name).file(&implementations)
            })
            .collect()
    });
    let codelists = project
        .get("codelists")
        .map_or(&[][..], Json::items)
        .iter()
        .map(codelist_source)
        .collect();
    let environment = Environment::admit(
        host,
        Draft {
            language: host,
            lock: None,
            functions,
            codelists,
        },
    )
    .unwrap_or_else(|findings| panic!("the project is admitted: {findings:?}"))
    .into_execution();
    let prepared = prepare_in_environment(&tree(input.field("document")).document(), &environment);
    match input.get("sources") {
        None => prepared.err().map_or_else(Vec::new, refusal),
        Some(_) => bound(
            &prepared.unwrap_or_else(|error| panic!("the document prepares: {error:?}")),
            &schemas(input),
        ),
    }
}

/// A project's scope finding, `{finding, ...}`: `duplicate_name` with `name`
/// and `listings`, each a `[path, specification]` pair, `unknown_name` with
/// `name` and `available`, `unknown_section` with `section` and `declared`,
/// `unlinked_producer` with `dataset`, `path` and `producer`,
/// `producer_not_completed` with `dataset` and `producer`, `workflow_cycle`
/// with `dataset` and `cycle`, and `run_nothing_to_save` and
/// `submission_nothing_to_save` alone.
fn scope_finding(input: &Json) -> Vec<Diagnostic> {
    let texts = |key: &str| {
        input
            .field(key)
            .items()
            .iter()
            .map(Json::text)
            .collect::<Vec<_>>()
    };
    vec![match text(input, "finding") {
        "duplicate_name" => {
            input.closed(&["finding", "name", "listings"]);
            let listings = input
                .field("listings")
                .items()
                .iter()
                .map(|listing| match listing.items() {
                    [path, specification] => (path.text().into(), specification.text().into()),
                    other => panic!("a listing is a [path, specification] pair: {other:?}"),
                })
                .collect::<Vec<_>>();
            project_scope::duplicate_name(text(input, "name"), &listings)
        }
        "unknown_name" => {
            input.closed(&["finding", "name", "available"]);
            project_scope::unknown_name(text(input, "name"), &texts("available"))
        }
        "unknown_section" => {
            input.closed(&["finding", "section", "declared"]);
            project_scope::unknown_section(text(input, "section"), &texts("declared"))
        }
        "unlinked_producer" => {
            input.closed(&["finding", "dataset", "path", "producer"]);
            project_scope::unlinked_producer(
                text(input, "dataset"),
                text(input, "path"),
                text(input, "producer"),
            )
        }
        "producer_not_completed" => {
            input.closed(&["finding", "dataset", "producer"]);
            project_scope::producer_not_completed(text(input, "dataset"), text(input, "producer"))
        }
        "workflow_cycle" => {
            input.closed(&["finding", "dataset", "cycle"]);
            project_scope::workflow_cycle(text(input, "dataset"), &texts("cycle"))
        }
        "run_nothing_to_save" => {
            input.closed(&["finding"]);
            project_scope::run_nothing_to_save()
        }
        "submission_nothing_to_save" => {
            input.closed(&["finding"]);
            project_scope::submission_nothing_to_save()
        }
        other => panic!("no project scope finding {other:?}"),
    }]
}

/// The staleness of the artifact a run report records against the one a
/// section composes, `{path, dataset, report, recorded, output}`: the guard
/// of the listing's report at `path` for `dataset`'s `report`, the report's
/// `recorded` artifact and the rows its run built, `{artifact, rows}`, and the
/// section's `output` (PROJECT-15).
fn staleness(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["path", "dataset", "report", "recorded", "output"]);
    let recorded = input.field("recorded").closed(&["artifact", "rows"]);
    let guard = Guard {
        path: text(input, "path").into(),
        dataset: text(input, "dataset"),
        report: text(input, "report"),
    };
    let recorded = Recorded {
        artifact: text(recorded, "artifact").into(),
        run: Run::Succeeded {
            rows: recorded.field("rows").integer(),
        },
        inputs: Vec::new(),
    };
    guard
        .run(&recorded, text(input, "output"))
        .err()
        .map(|finding| finding.record())
        .into_iter()
        .collect()
}

/// The findings preparing the consuming `document`, held at `identity`,
/// refuses with when each of its `links` names the producing specification
/// a `schema` link reads, `{document, identity, links}`: `links` lists each
/// link, in order, as `{dataset, identity, document}`, the input it serves
/// and the producing specification held at `identity`, so two links may
/// name one input (INPUT-6, INPUT-10).
fn prepare_linked(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["document", "identity", "links"]);
    let producers = input
        .field("links")
        .items()
        .iter()
        .map(|link| {
            let link = link.closed(&["dataset", "identity", "document"]);
            (
                text(link, "dataset"),
                text(link, "identity"),
                tree(link.field("document")).document(),
            )
        })
        .collect::<Vec<_>>();
    let links = producers
        .iter()
        .map(|(dataset, identity, document)| Link {
            dataset,
            identity,
            document,
        })
        .collect::<Vec<_>>();
    let links = Links {
        consumer: text(input, "identity"),
        links: &links,
    };
    prepare_linked_with(&tree(input.field("document")).document(), None, links)
        .err()
        .map_or_else(Vec::new, refusal)
}

/// The finding of a call of the project function `function` at `path`
/// whose decoding failed, `{path, function, argument, cause, value}`: a
/// fault of the call's `argument`, or of the call itself when it is `null`,
/// of the `cause` `integer_range`, `invalid_date`, `invalid_datetime` or
/// `invalid_shape`, with the text its value is written as, or `null`. A case
/// reads it for a cause no specification reaches, since the grammar reads
/// a `DATE` or `DATETIME` literal's text before the call does.
fn function_finding(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["path", "function", "argument", "cause", "value"]);
    let written = |key: &str| match input.field(key) {
        Json::Null => None,
        written => Some(written.text().to_owned()),
    };
    let kind = match text(input, "cause") {
        "integer_range" => Kind::IntegerRange,
        "invalid_date" => Kind::InvalidDate,
        "invalid_datetime" => Kind::InvalidDateTime,
        "invalid_shape" => Kind::NormalizedShape,
        other => panic!("no decoding cause {other:?}"),
    };
    let finding = FunctionFinding {
        path: text(input, "path").into(),
        function: text(input, "function").into(),
        argument: written("argument"),
        cause: FunctionCause::Scalar {
            kind,
            value: written("value"),
        },
    };
    vec![finding.diagnostic()]
}

/// The finding of stored metadata that the contract of the `producer`
/// specification does not admit for the input `dataset`, `{producer,
/// dataset, header, types}` (INPUT-11): the stored `header` alone, or, with
/// `types`, one for each of its names, the typed fields a self-describing
/// artifact stores.
fn producer_header(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["producer", "dataset", "header", "types"]);
    let header = input
        .field("header")
        .items()
        .iter()
        .map(Json::text)
        .collect::<Vec<_>>();
    let contract =
        prepare_document(&tree(input.field("producer")).document()).expect("a producer contract");
    let dataset = text(input, "dataset");
    match input.get("types") {
        None => contract.validate_header(dataset, &header),
        Some(types) => {
            let types = types.items();
            assert_eq!(types.len(), header.len(), "a type for each stored field");
            let fields = header
                .iter()
                .zip(types)
                .map(|(name, kind)| (*name, column_type(kind)))
                .collect::<Vec<_>>();
            contract.validate_typed_fields(dataset, &fields)
        }
    }
    .expect("metadata the contract reads")
    .into_iter()
    .collect()
}

/// The layers of a chain, `{layers}`, as the documents they are, and their
/// copy entries.
fn layers(input: &Json) -> Vec<Document> {
    input
        .field("layers")
        .items()
        .iter()
        .map(|layer| tree(layer).document())
        .collect()
}

/// The findings of the first layer whose copy entries name a dataset that
/// is no input or an input without a `schema` link, `{layers}` (SPEC-9).
fn copy_datasets(input: &Json) -> Vec<Diagnostic> {
    let layers = layers(input.closed(&["layers"]));
    let copies = Copies::of(&layers.iter().collect::<Vec<_>>()).expect("copy entries");
    copied(&layers, copies.dataset_findings())
}

/// The findings of the first layer whose copy entries list a column its
/// producer does not write, `{layers, producers}`: `producers` maps each
/// linked input to its producing specification (SPEC-9).
fn copy_columns(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["layers", "producers"]);
    let layers = layers(input);
    let contracts = input
        .field("producers")
        .members()
        .iter()
        .map(|(dataset, producer)| {
            (
                dataset.as_str(),
                prepare_document(&tree(producer).document()).expect("a producer contract"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let copies = Copies::of(&layers.iter().collect::<Vec<_>>()).expect("copy entries");
    copied(&layers, copies.column_findings(&contracts))
}

/// The findings in the layer at the index the copies name.
fn copied(
    layers: &[Document],
    findings: Option<(usize, Vec<SchemaDiagnostic>)>,
) -> Vec<Diagnostic> {
    findings.map_or_else(Vec::new, |(index, findings)| {
        findings
            .iter()
            .map(|finding| located(&layers[index], finding))
            .collect()
    })
}

/// The refusal of a producer whose copies lead back to a specification being
/// resolved, `{dataset, cycle}` (SPEC-9).
fn copy_cycle(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["dataset", "cycle"]);
    let cycle = input
        .field("cycle")
        .items()
        .iter()
        .map(Json::text)
        .collect::<Vec<_>>();
    vec![producer_cycle(text(input, "dataset"), &cycle)]
}

/// The schema of one module, written `{name: document}`, whose entry class
/// is `root_class`.
fn structure(json: &Json) -> SchemaStructure {
    let [(name, document)] = json.members() else {
        panic!("a schema is one module: {json:?}")
    };
    SchemaStructure::admit(
        vec![SchemaModule {
            name: name.clone(),
            document: tree(document).document(),
        }],
        0,
        "root_class",
    )
    .expect("an admitted schema")
}

/// The findings of validating `document` against `schema`, `{schema,
/// document}`.
fn schema_document(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["schema", "document"]);
    let schema = structure(input.field("schema"));
    let document = tree(input.field("document")).document();
    let findings = schema.validate_document(&document).expect("a pass");
    projected(&schema, &document, &findings)
}

/// The findings of validating the layer `document` against `schema` as the
/// partial document a layer is, `{schema, document}` (LAYER-4).
fn schema_layer(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["schema", "document"]);
    let schema = structure(input.field("schema"));
    let document = tree(input.field("document")).document();
    let findings = schema.validate_layer(&document).expect("a pass");
    projected(&schema, &document, &findings)
}

/// The findings of expanding the window references of `document` against
/// `schema`, strictly or not, `{schema, document, strict}` (WINDOW-1).
fn schema_windows(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["schema", "document", "strict"]);
    let schema = structure(input.field("schema"));
    let document = tree(input.field("document")).document();
    match schema.expand_named_windows(&document, input.field("strict").boolean()) {
        Ok(_) => Vec::new(),
        Err(NormalizationError::Invalid(findings)) => projected(&schema, &document, &findings),
        Err(other) => panic!("expansion refuses with no finding: {other:?}"),
    }
}

/// The findings of validating the root of `document` as the `types` of
/// `schema` read at the language `version` a document declares, `{schema,
/// version, types, document}` (P-VER-1).
fn schema_types(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["schema", "version", "types", "document"]);
    let schema = structure(input.field("schema"));
    let types = schema
        .parse_query_types(
            &input
                .field("types")
                .items()
                .iter()
                .map(|kind| kind.text().to_owned())
                .collect::<Vec<_>>(),
        )
        .expect("type expressions");
    let version = LanguageVersion::parse(text(input, "version")).expect("a 1.x version");
    let schema = schema.at_version(version);
    let document = tree(input.field("document")).document();
    let findings = schema
        .validate_types(&types, &document, document.root(), "", false)
        .expect("a pass");
    projected(&schema, &document, &findings)
}

/// `findings` of a pass over `document` as the records their issues carry,
/// each context value resolved as the engine's projection resolves it: a
/// node against `document`, a descriptor's constraint against `schema`
/// (`admit_schema_diagnostics`).
fn projected(
    schema: &SchemaStructure,
    document: &Document,
    findings: &[SchemaDiagnostic],
) -> Vec<Diagnostic> {
    let report = admit_schema_diagnostics(schema, Some(document), findings, &[])
        .expect("findings that resolve");
    findings
        .iter()
        .zip(report.iter())
        .map(|(finding, resolved)| Diagnostic {
            code: finding.code,
            spec_paths: vec![resolved.path.into()],
            context: resolved
                .context
                .map(|(name, value)| (name.into(), json_value(value)))
                .collect(),
            source_span: None,
            operand_route: None,
        })
        .collect()
}

/// A finding in the `document` a header or a copy entry was read from, which
/// names no schema descriptor, as the record its issue carries: a node it
/// names is that node's value, as the engine's projection reads it.
fn located(document: &Document, finding: &SchemaDiagnostic) -> Diagnostic {
    Diagnostic {
        code: finding.code,
        spec_paths: vec![finding.path.clone()],
        context: finding
            .context
            .iter()
            .map(|(name, context)| ((*name).into(), json_value(node_value(document, context))))
            .collect(),
        source_span: None,
        operand_route: None,
    }
}

/// A context value of a finding in `document`, a node it names read as that
/// node's value.
fn node_value<'a>(document: &'a Document, context: &'a SchemaContext) -> SchemaDiagnosticValue<'a> {
    use SchemaDiagnosticValue as V;
    match context {
        SchemaContext::Text(text) => V::Text(text),
        SchemaContext::Boolean(value) => V::Boolean(*value),
        SchemaContext::Count(count) => V::Count(*count),
        SchemaContext::Null => V::Null,
        SchemaContext::TextList(values) => V::TextList(values),
        SchemaContext::ValueKind(_) => {
            V::Text(schema_diagnostic_kind_label(context).expect("a kind has a label"))
        }
        SchemaContext::InputValue(node) => match &document.nodes()[*node] {
            DocumentNode::Null => V::Null,
            DocumentNode::Boolean(value) => V::Boolean(*value),
            DocumentNode::Integer(text) => V::Integer(text),
            DocumentNode::Float(value) => V::Float(*value),
            DocumentNode::Text(text) => V::Text(text),
            other => panic!("a node a finding names is a scalar, not {other:?}"),
        },
        other => panic!("a finding outside a schema pass names no descriptor: {other:?}"),
    }
}

/// A resolved context value as the JSON an issue writes.
fn json_value(value: SchemaDiagnosticValue<'_>) -> JsonValue {
    use SchemaDiagnosticValue as V;
    match value {
        V::Text(text) => JsonValue::String(text.into()),
        V::Count(count) => JsonValue::from(count),
        V::Null => JsonValue::Null,
        V::Boolean(value) => JsonValue::Bool(value),
        V::Integer(text) => JsonValue::Number(text.parse().expect("an integer spelling")),
        V::Float(value) => JsonValue::Number(Number::from_f64(value).expect("a finite float")),
        V::TextList(values) => {
            JsonValue::Array(values.iter().cloned().map(JsonValue::String).collect())
        }
    }
}

/// A file type as a `type` header spells it.
#[track_caller]
fn file_type(json: &Json) -> FileType {
    FileType::parse(json.text()).unwrap_or_else(|| panic!("no file type {json:?}"))
}

/// The header of `document`, read by this engine as a file of `file_type`,
/// `{document, file_type}`, or by the type it declares, `{document}`
/// (SCHEMA-2).
fn language_header(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["document", "file_type"]);
    let document = tree(input.field("document")).document();
    let finding = match input.get("file_type") {
        Some(expected) => read_header(&document, Some(LANGUAGE_VERSION), file_type(expected))
            .err()
            .map(|fault| fault.finding()),
        None => read_declared_header(&document, LANGUAGE_VERSION)
            .err()
            .map(|fault| fault.finding()),
    };
    finding
        .iter()
        .map(|finding| located(&document, finding))
        .collect()
}

/// The finding for a file type a call takes as its argument, `{type}`
/// (SCHEMA-2).
fn file_type_argument(input: &Json) -> Vec<Diagnostic> {
    vec![unknown_file_type(text(input.closed(&["type"]), "type"))]
}

/// The members a referencing field's operations read.
const REFERENCE: [&str; 6] = [
    "field",
    "written",
    "expected",
    "relative_to",
    "source",
    "document",
];

/// A field that names the yamaa file `document`, read from `source`,
/// `{field, written, expected, relative_to, source, document}`: `expected`
/// is the type the file must declare and `relative_to` the directory
/// `written` resolves from, the `declaring` file's or the `entry`'s.
fn reference(input: &Json) -> (Reference, Document) {
    let input = input.closed(&REFERENCE);
    (
        Reference {
            field: text(input, "field").into(),
            written: text(input, "written").into(),
            expected: file_type(input.field("expected")),
            base: match text(input, "relative_to") {
                "declaring" => PathBase::Declaring,
                "entry" => PathBase::Entry,
                other => panic!("no path base {other:?}"),
            },
        },
        tree(input.field("document")).document(),
    )
}

/// How a referencing field reports the file it names, as the field checks
/// its header (SCHEMA-2).
fn reference_check(input: &Json) -> Vec<Diagnostic> {
    let (reference, document) = reference(input);
    refused(
        &document,
        reference.check(text(input, "source"), &document, LANGUAGE_VERSION),
    )
}

/// How a referencing field reports the file it names, as a reader that
/// decoded the file and kept its header's fault before it knew the field
/// reports it (SCHEMA-2).
fn reference_refuse(input: &Json) -> Vec<Diagnostic> {
    let (reference, document) = reference(input);
    refused(
        &document,
        read_header(&document, Some(LANGUAGE_VERSION), reference.expected)
            .map_err(|fault| reference.refuse(text(input, "source"), &fault)),
    )
}

/// A referencing field's fault as its issue: a diagnostic at the field, or a
/// finding in the named `document`.
fn refused<T>(document: &Document, result: Result<T, ReferenceFault>) -> Vec<Diagnostic> {
    match result {
        Ok(_) => Vec::new(),
        Err(ReferenceFault::Field(diagnostic)) => vec![diagnostic],
        Err(ReferenceFault::Finding(finding)) => vec![located(document, &finding)],
    }
}

/// A layer's header refusal on a chain, `{finding, source, entry, version,
/// reads, file_type, named, declaring}`: `finding` is `language_version`,
/// with the `version` the layer declares and the versions the engine
/// `reads`, or `file_type`, with the type it declares; `named` is `entry` or
/// `parent`, and `declaring` the layer that names it, or `null` (LAYER-4).
fn layer_header(input: &Json) -> Vec<Diagnostic> {
    let named = match text(input, "named") {
        "entry" => Named::Entry,
        "parent" => Named::Parent,
        other => panic!("no named layer {other:?}"),
    };
    let declaring = match input.field("declaring") {
        Json::Null => None,
        written => Some(written.text()),
    };
    let reads;
    let finding = match text(input, "finding") {
        "language_version" => {
            input.closed(&[
                "finding",
                "source",
                "entry",
                "version",
                "reads",
                "named",
                "declaring",
            ]);
            reads = input
                .field("reads")
                .items()
                .iter()
                .map(|version| version.text().to_owned())
                .collect::<Vec<_>>();
            InheritanceFinding::LanguageVersion {
                source: text(input, "source"),
                entry: text(input, "entry"),
                version: text(input, "version"),
                reads: &reads,
                named,
                declaring,
            }
        }
        "file_type" => {
            input.closed(&[
                "finding",
                "source",
                "entry",
                "file_type",
                "named",
                "declaring",
            ]);
            InheritanceFinding::FileType {
                source: text(input, "source"),
                entry: text(input, "entry"),
                file_type: text(input, "file_type"),
                named,
                declaring,
            }
        }
        other => panic!("no layer header finding {other:?}"),
    };
    vec![finding.diagnostic().expect("a complete context")]
}
