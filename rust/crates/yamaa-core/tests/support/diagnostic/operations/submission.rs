//! The submission and front-end readers. A study is what composition reads,
//! written as one object: the project file, the specifications its first
//! submission section lists and those its other sections list, the codelist
//! sources, the installed version and the creation time. Its operations
//! compose the study's Define-XML metadata, admit held rows as a composed
//! dataset's published rows, and make the findings the adapters' Define-XML
//! and XPT encoders report, and project the Define service's resource and
//! publication refusals. `front_end_checks` runs the 1.0 front end's own
//! checks on a lowered specification.
use super::{codelist_source, column_type, document::tree, text, value, File};
use crate::support::{
    diagnostic::{json::Json, Record},
    submission::compose as compose_study,
};
use std::convert::Infallible;
use yamaa_core::{
    application_issue::Fault,
    define,
    define_diagnostics::{self, TargetRelation},
    diagnostic::Diagnostic,
    front_end::v1::spec as front_end,
    limits::LimitExceeded,
    project_terminology as ct,
    submission_composition::{Dataset, Finding, Study},
    submission_transport,
    table::{CellError, Column, TableAccess, TableSchema, ValueRef},
    value::Value,
};

pub(super) const FILES: &[File] = &[fixture!("front_end.json"), fixture!("submission.json")];

pub(super) fn run(operation: &str, input: &Json) -> Option<Vec<Diagnostic>> {
    Some(match operation {
        "admit_relation" => admit_relation(input),
        "artifact_outside_document" => artifact_outside_document(input),
        "compose" => compose(input),
        "created_text" => created_text(input),
        "define_publication" => define_publication(input),
        "define_resource" => define_resource(input),
        "front_end_checks" => front_end_checks(input),
        "transport" => transport(input),
        "xml_invalid" => xml_invalid(input),
        "xml_whitespace" => xml_whitespace(input),
        "xpt_file_too_large" => xpt_file_too_large(input),
        "xpt_last_row_blank" => xpt_last_row_blank(input),
        _ => return None,
    })
}

/// Findings as the issues a run reports, each at its one path.
fn issues<'a>(findings: impl IntoIterator<Item = &'a Finding>) -> Vec<Diagnostic> {
    findings.into_iter().map(Record::record).collect()
}

/// The findings composing the study `document` refuses with, `{document}`.
fn compose(input: &Json) -> Vec<Diagnostic> {
    match study(input.closed(&["document"])) {
        Ok(_) => Vec::new(),
        Err(findings) => issues(&findings),
    }
}

/// The composition of the study `document` of `input`: `{project, specs,
/// others, terminology, installed, created}`, with `specs` the
/// specifications the project's first submission section lists, in its
/// order, and `others` those its other sections list.
fn study(input: &Json) -> Result<Study, Vec<Finding>> {
    let study = input.field("document").closed(&[
        "project",
        "specs",
        "others",
        "terminology",
        "installed",
        "created",
    ]);
    let documents = |key| study.field(key).items().iter().map(tree).collect();
    compose_study(
        tree(study.field("project")),
        documents("specs"),
        documents("others"),
        &catalogue(study.field("terminology")),
        text(study, "installed"),
        text(study, "created"),
    )
}

/// The study of `input`, which must compose.
fn composed(input: &Json) -> Study {
    study(input).unwrap_or_else(|findings| panic!("the study composes: {findings:?}"))
}

/// The dataset `dataset` of `input` names in `study`.
fn dataset<'a>(study: &'a Study, input: &Json) -> &'a Dataset {
    let id = text(input, "dataset");
    study
        .datasets()
        .iter()
        .find(|dataset| dataset.id() == id)
        .unwrap_or_else(|| panic!("the study composes no dataset {id}"))
}

/// The refusal of admitting `table` as the published rows of `dataset` of
/// the study `document`, `{document, dataset, table}` (XPT-7, XPT-8, JSON-3,
/// JSON-4, CT-11).
fn transport(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["document", "dataset", "table"]);
    let study = composed(input);
    let table = held(input.field("table"));
    issues(&submission_transport::admit(&study, dataset(&study, input), &table).err())
}

/// XPT-10's refusal of the transport file of `dataset` of the study
/// `document`, whose last row, `row`, encodes to blanks only, `{document,
/// dataset, row}`.
fn xpt_last_row_blank(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["document", "dataset", "row"]);
    let study = composed(input);
    let row = input.field("row").integer();
    issues([&define_diagnostics::xpt_last_row_blank(
        dataset(&study, input),
        row,
    )])
}

/// XPT-10's refusal of the transport file of `dataset` of the study
/// `document`, which would hold `bytes`, `{document, dataset, bytes}`.
fn xpt_file_too_large(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["document", "dataset", "bytes"]);
    let study = composed(input);
    let bytes = input.field("bytes").integer();
    issues([&define_diagnostics::xpt_file_too_large(
        dataset(&study, input),
        bytes,
    )])
}

/// DEFINE-3's refusal of the creation time `created` as written, `{created}`.
fn created_text(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["created"]);
    issues(&define_diagnostics::created_text(text(input, "created")).err())
}

/// The Define service's issue for the resource at the section's field
/// `path`, written `written`, that it observed as `observation`, `{path,
/// written, observation}`: `missing`, `unreadable`, `not_regular_file`,
/// `invalid_root`, `invalid_base`, `invalid_path`, `outside_roots`,
/// `symlink` or `changed`, or `{"limit": {resource, limit, actual}}`.
fn define_resource(input: &Json) -> Vec<Diagnostic> {
    use define::ResourceObservation as R;
    let input = input.closed(&["path", "written", "observation"]);
    let observation = match input.field("observation") {
        Json::Object(_) => {
            let limit = input
                .field("observation")
                .closed(&["limit"])
                .field("limit")
                .closed(&["resource", "limit", "actual"]);
            R::Limit(LimitExceeded {
                resource: text(limit, "resource").to_owned().leak(),
                limit: limit.field("limit").integer(),
                actual: Some(limit.field("actual").integer()),
            })
        }
        named => match named.text() {
            "missing" => R::Missing,
            "unreadable" => R::Unreadable,
            "not_regular_file" => R::NotRegularFile,
            "invalid_root" => R::InvalidRoot,
            "invalid_base" => R::InvalidBase,
            "invalid_path" => R::InvalidPath,
            "outside_roots" => R::OutsideRoots,
            "symlink" => R::Symlink,
            "changed" => R::Changed,
            other => panic!("no resource observation {other:?}"),
        },
    };
    vec![define::resource_diagnostic(
        text(input, "path"),
        text(input, "written"),
        observation,
    )]
}

/// The Define service's issue for the failed publication of artifact
/// `artifact`, written `written`, at `stage`, `{artifact, written, stage,
/// cause}`: `publication`, or `{"target_admission": <fault>}`, the engine
/// fault by its spelling.
fn define_publication(input: &Json) -> Vec<Diagnostic> {
    use define::PublicationFailure as P;
    let input = input.closed(&["artifact", "written", "stage", "cause"]);
    let stage = match input.field("stage") {
        Json::Object(_) => {
            let code = text(
                input.field("stage").closed(&["target_admission"]),
                "target_admission",
            );
            P::TargetAdmission(
                *Fault::ALL
                    .iter()
                    .find(|known| known.as_str() == code)
                    .unwrap_or_else(|| panic!("no engine fault {code:?}")),
            )
        }
        named => match named.text() {
            "publication" => P::Publication,
            other => panic!("no publication stage {other:?}"),
        },
    };
    vec![define::publication_diagnostic(
        input.field("artifact").integer(),
        text(input, "written"),
        stage,
        text(input, "cause").into(),
    )]
}

/// JSON-2's refusal of the listing at `path` whose file `target` has the
/// `relation` (`independent`, `artifact_collision` or `transport_collision`)
/// to another, described by `detail`, `{relation, path, target, detail}`.
fn admit_relation(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["relation", "path", "target", "detail"]);
    let relation = match text(input, "relation") {
        "independent" => TargetRelation::Independent,
        "artifact_collision" => TargetRelation::ArtifactCollision,
        "transport_collision" => TargetRelation::TransportCollision,
        other => panic!("no target relation {other:?}"),
    };
    let admitted = define_diagnostics::admit_relation(
        relation,
        text(input, "path"),
        text(input, "target"),
        text(input, "detail"),
    );
    issues(&admitted.err())
}

/// DEFINE-16's refusal of the listing at `path`, whose `dataset` publishes
/// `artifact` outside the directory of the document `define`, `{path,
/// dataset, artifact, define}`.
fn artifact_outside_document(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["path", "dataset", "artifact", "define"]);
    issues([&define_diagnostics::artifact_outside_document(
        text(input, "path"),
        text(input, "dataset"),
        text(input, "artifact"),
        text(input, "define"),
    )])
}

/// DEFINE-24's refusal of the generated text at `path` holding the one
/// `character`, `{path, character}`.
fn xml_whitespace(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["path", "character"]);
    let written = text(input, "character");
    let mut characters = written.chars();
    let (Some(character), None) = (characters.next(), characters.next()) else {
        panic!("a character is one character: {written:?}")
    };
    issues([&define_diagnostics::xml_whitespace(
        text(input, "path"),
        character,
    )])
}

/// DEFINE-25's refusal of the generated document at `path`, whose `value`
/// breaks the schema `facet` names, `{path, facet, value}`.
fn xml_invalid(input: &Json) -> Vec<Diagnostic> {
    let input = input.closed(&["path", "facet", "value"]);
    issues([&define_diagnostics::xml_invalid(
        text(input, "path"),
        text(input, "facet"),
        input.field("value").value(),
    )])
}

/// The findings of the 1.0 front end's own checks of the lowered `document`,
/// the rules of the surface its schema cannot state, `{document}`.
fn front_end_checks(input: &Json) -> Vec<Diagnostic> {
    let document = tree(input.closed(&["document"]).field("document")).document();
    let lowered = front_end::lower(&document, None);
    front_end::checks::findings(lowered.spec())
        .iter()
        .map(Record::record)
        .collect()
}

/// Held published rows, `{columns, rows}`: `columns` as `[name, type]`
/// pairs, and each row the list of its values, one for each column.
struct Held {
    schema: TableSchema,
    rows: Vec<Vec<Value>>,
}

impl TableAccess for Held {
    type Error = Infallible;
    fn schema(&self) -> &TableSchema {
        &self.schema
    }
    fn row_count(&self) -> usize {
        self.rows.len()
    }
    fn cell(&self, row: usize, column: usize) -> Result<ValueRef<'_>, CellError<Infallible>> {
        self.rows
            .get(row)
            .and_then(|cells| cells.get(column))
            .map(ValueRef::from)
            .ok_or(CellError::OutOfBounds { row, column })
    }
}

fn held(json: &Json) -> Held {
    let json = json.closed(&["columns", "rows"]);
    let columns = json
        .field("columns")
        .items()
        .iter()
        .map(|column| match column.items() {
            [name, kind] => Column {
                name: name.text().into(),
                kind: column_type(kind),
            },
            other => panic!("a column is a [name, type] pair: {other:?}"),
        })
        .collect::<Vec<_>>();
    let rows = json
        .field("rows")
        .items()
        .iter()
        .map(|row| {
            assert_eq!(
                row.items().len(),
                columns.len(),
                "a row holds one value for each column: {row:?}"
            );
            row.items().iter().map(value).collect()
        })
        .collect();
    Held {
        schema: TableSchema::new(columns).expect("an admitted schema"),
        rows,
    }
}

/// The terminology of the codelist sources `json`, each written as a codelist
/// file writes its source ([`codelist_source`]).
fn catalogue(json: &Json) -> ct::Catalogue {
    let sources = json.items().iter().map(codelist_source).collect();
    ct::Catalogue::admit(sources)
        .unwrap_or_else(|findings| panic!("an admitted terminology: {findings:?}"))
}
