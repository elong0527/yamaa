//! The command line's leg of the cross-host parity truth (#2846, #3623):
//! each case of `rust/tests/host-fixtures/cases.json` that holds complete
//! issue or run-report rows runs through the `yamaa` binary with
//! `--format json`, and what it prints is compared with the rows that the
//! installed Python and R suites compare their own results with. All three
//! equal one truth, so they equal each other.
//!
//! - **Issues.** Each case of `diagnostic-parity` and `project-parity`, and
//!   the invalid files of `parquet-containment`, writes its files and makes
//!   its call, or each of its calls: `check` or `run` of `spec.yaml`, given as
//!   that relative path, or `check`, `run` or `submission` of a project. Every
//!   issue the document holds is compared with the case's rows in report
//!   order, all twelve columns of D28 (`public-contract.issue_columns`), and
//!   each issue object has exactly those keys. A context is compared as the
//!   canonical JSON text that the binary printed, so no reader chooses a
//!   number type.
//! - **Run reports.** The command line sees a run's report in the file its
//!   save writes (OUTPUT-10). `public-contract.value_level_report` declares
//!   one, whose value-level rows are compared with the case's, all fourteen
//!   columns (`public-contract.report_columns`). The failed run of
//!   `public-contract`, assembled as both hosts assemble it, saves only its
//!   report, whose bytes are compared with `failed_report_csv`.
//! - **Fragments.** Each check case of `include-fragments` prints every issue
//!   row that both hosts return for it, and `resolve` prints the layers, the
//!   `parent` and `include` resources, the resolved columns and the origins
//!   that both hosts read from the resolved facts. A fragment checked with
//!   `--parent` prints the rows that its child, which names that parent and
//!   includes the fragment, prints, and a parent given for a file that is no
//!   fragment is refused with the rows and error text both hosts raise.
//! - **Locations.** Each case of `issue-locations` makes its call, and its
//!   first issue has the condition, the first spec path, and the file, line
//!   and column that both hosts compare (P-ISSUE-1).
//! - **Lint.** The `lint` family's partial restatement, a layer and the
//!   parent it reads, is linted with and without `--fix`; each finding the
//!   document prints is compared with the case's rows, all six lint columns,
//!   and the layer stays as written.
//! - **Conditions.** `yamaa conditions` prints the catalogue as text and as
//!   JSON, and both name every condition of the `conditions` family in its
//!   order, with each authored row's cells (#3700).
//!
//! A case whose rows or files differ by host gives them by host, under
//! `python`, `r` and `cli`, and the command line reads its own: a case that
//! gives them by host gives the command line's too. The command line runs
//! no project function and checks no lock (D110, D123), so a case that calls
//! one gives the command line's `unsupported_operation`. A project run prints
//! any project finding, then every member's issues in run order, and counts
//! each member's, so a case that names a member compares that member's
//! issues.
//!
//! `{path}` stands for the path the case gave, `{root}` for its directory as
//! the engine spells it and `{version}` for the engine's version; in a
//! context they are JSON text. A row's `file` names a file of the case's
//! directory, which a location names by its absolute path (P-ISSUE-1).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde_json::value::RawValue;
use serde_json::Value;

use crate::support::{yamaa_in, Scratch};

/// The shared truth, read once for every test of this family.
static TRUTH: LazyLock<Value> = LazyLock::new(|| {
    let truth: Value =
        serde_json::from_str(include_str!("../../../tests/host-fixtures/cases.json")).unwrap();
    assert_eq!(truth["version"], "1", "the installed-host fixture version");
    truth
});

/// One JSON document the binary printed, each member as the exact text it
/// printed.
type Document = BTreeMap<String, Box<RawValue>>;

/// One issue or report row: each column's name and value, in column order.
type Row = Vec<(&'static str, Value)>;

fn family(name: &str) -> &'static Value {
    let family = &TRUTH["families"][name];
    assert!(family.is_object(), "cases.json has no family {name}");
    family
}

/// The names that a family lists under `key`, in order.
fn names(family: &'static Value, key: &str) -> Vec<&'static str> {
    family[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect()
}

/// The twelve issue columns of D28, in order.
fn issue_columns() -> Vec<&'static str> {
    names(family("public-contract"), "issue_columns")
}

/// The fourteen run-report columns, in order (CHECK-23).
fn report_columns() -> Vec<&'static str> {
    names(family("public-contract"), "report_columns")
}

/// The text that `value` holds under `key`.
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} is text: {value}"))
}

/// A fixture's bytes: UTF-8 text, or `{"hex": ...}` for bytes that no text
/// holds.
fn bytes(value: &Value) -> Vec<u8> {
    match value {
        Value::String(text) => text.as_bytes().to_vec(),
        Value::Object(_) => {
            let hex = text(value, "hex");
            (0..hex.len())
                .step_by(2)
                .map(|start| u8::from_str_radix(&hex[start..start + 2], 16).unwrap())
                .collect()
        }
        other => panic!("a byte fixture is text or hexadecimal text: {other}"),
    }
}

/// The value a case gives every host, or the command line's where it gives
/// one by host.
fn for_command_line<'a>(value: &'a Value, what: &str, case: &str) -> &'a Value {
    match value.as_object() {
        Some(hosts) => hosts.get("cli").unwrap_or_else(|| {
            panic!("{case} gives its {what} by host but not the command line's")
        }),
        None => value,
    }
}

/// The one JSON document that the binary prints for `arguments` in
/// `directory`: it exits with status 0 or 1 and prints one line, and nothing
/// on standard error.
fn document(directory: &Scratch, arguments: &[&str], case: &str) -> Document {
    let mut all = vec!["--format", "json"];
    all.extend_from_slice(arguments);
    let output = yamaa_in(directory, &all);
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 output");
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 output");
    assert!(
        matches!(output.status.code(), Some(0 | 1)) && stderr.is_empty(),
        "{case}: yamaa {arguments:?} exited with {:?}: {stderr}",
        output.status.code()
    );
    let line = stdout
        .strip_suffix('\n')
        .filter(|line| !line.contains('\n'))
        .unwrap_or_else(|| panic!("{case}: yamaa printed no single line: {stdout}"));
    serde_json::from_str(line).unwrap_or_else(|error| panic!("{case}: {error}: {line}"))
}

/// The text that a document printed for its member `key`.
fn raw<'a>(document: &'a Document, key: &str, case: &str) -> &'a str {
    document
        .get(key)
        .unwrap_or_else(|| panic!("{case}: the document has no {key}"))
        .get()
}

/// The paths that a run's save wrote, as the document prints them.
fn paths(document: &Document, case: &str) -> Vec<String> {
    serde_json::from_str(raw(document, "paths", case)).unwrap()
}

/// The issues of a document, each as its twelve columns in order, with its
/// context as the JSON text that the binary printed.
fn printed_rows(document: &Document, case: &str) -> Vec<Row> {
    let columns = issue_columns();
    let mut keys = columns.clone();
    keys.sort_unstable();
    let issues: Vec<BTreeMap<String, Box<RawValue>>> =
        serde_json::from_str(raw(document, "issues", case)).unwrap();
    issues
        .iter()
        .map(|issue| {
            // Each issue has exactly the twelve keys, which the map holds
            // sorted, as `keys` holds the columns.
            assert_eq!(
                issue.keys().map(String::as_str).collect::<Vec<_>>(),
                keys,
                "{case}: an issue's keys"
            );
            columns
                .iter()
                .map(|&column| {
                    let raw = issue[column].get();
                    let value = if column == "context" {
                        Value::String(raw.to_owned())
                    } else {
                        serde_json::from_str(raw).unwrap()
                    };
                    (column, value)
                })
                .collect()
        })
        .collect()
}

/// The issues of `name` among the rows a project run printed: any project
/// finding first, then each member's issues in run order, as many as the
/// document's `members` count for each.
fn member_rows(document: &Document, rows: Vec<Row>, name: &str, case: &str) -> Vec<Row> {
    let members: Vec<Value> = serde_json::from_str(raw(document, "members", case)).unwrap();
    let counts: Vec<(&str, usize)> = members
        .iter()
        .map(|run| {
            let count = run["issue_count"].as_u64().unwrap();
            (text(run, "name"), usize::try_from(count).unwrap())
        })
        .collect();
    let index = counts
        .iter()
        .position(|(member, _)| *member == name)
        .unwrap_or_else(|| panic!("{case}: the run has no member {name}: {counts:?}"));
    let counted = counts.iter().map(|(_, count)| count).sum::<usize>();
    let findings = rows.len().checked_sub(counted).unwrap_or_else(|| {
        panic!(
            "{case}: the members count {counted} of {} issues",
            rows.len()
        )
    });
    let start = findings
        + counts[..index]
            .iter()
            .map(|(_, count)| count)
            .sum::<usize>();
    rows.into_iter().skip(start).take(counts[index].1).collect()
}

/// A case's rows, each as its twelve columns in order, with each stand-in
/// filled: as text in a message and as JSON text in a context. A row's `file`
/// becomes the absolute path of that file of `directory`.
fn truth_rows(rows: &Value, directory: &Scratch, stand_ins: &[(&str, String)]) -> Vec<Row> {
    let columns = issue_columns();
    let fill = |text: &str, json: bool| {
        stand_ins
            .iter()
            .fold(text.to_owned(), |text, (stand_in, value)| {
                let value = if json {
                    let quoted = serde_json::to_string(value).unwrap();
                    quoted[1..quoted.len() - 1].to_owned()
                } else {
                    value.clone()
                };
                text.replace(stand_in, &value)
            })
    };
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let mut keys = row
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>();
            keys.sort_unstable();
            let mut expected = columns.clone();
            expected.sort_unstable();
            assert_eq!(
                keys, expected,
                "a truth row holds the twelve columns: {row}"
            );
            columns
                .iter()
                .map(|&column| {
                    let value = match (column, &row[column]) {
                        ("file", Value::String(file)) => Value::String(directory.spelled(file)),
                        ("message", Value::String(message)) => Value::String(fill(message, false)),
                        ("context", Value::String(context)) => Value::String(fill(context, true)),
                        (_, value) => value.clone(),
                    };
                    (column, value)
                })
                .collect()
        })
        .collect()
}

/// The records of CSV text as the csv profile writes it (CSV-1 to CSV-4): a
/// field is quoted when it holds a quote or a comma or is empty, a quote in
/// it is doubled, a missing value is an empty bare field, read as `None`,
/// and each record ends with a line feed.
fn csv_records(text: &str) -> Vec<Vec<Option<String>>> {
    let (mut records, mut record, mut field) = (Vec::new(), Vec::new(), String::new());
    let (mut quoted, mut within) = (false, false);
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match (within, character) {
            (true, '"') if characters.peek() == Some(&'"') => {
                characters.next();
                field.push('"');
            }
            (true, '"') => within = false,
            (true, other) => field.push(other),
            (false, '"') => (quoted, within) = (true, true),
            (false, ',' | '\n') => {
                let value = std::mem::take(&mut field);
                record.push((quoted || !value.is_empty()).then_some(value));
                quoted = false;
                if character == '\n' {
                    records.push(std::mem::take(&mut record));
                }
            }
            (false, other) => field.push(other),
        }
    }
    assert!(
        record.is_empty() && field.is_empty() && !within,
        "the CSV text ends with a line feed: {text}"
    );
    records
}

/// Each case of `diagnostic-parity`, the check or run of one specification,
/// prints every issue row that both hosts return for it.
#[test]
fn each_diagnostic_parity_case_prints_the_issue_rows_both_hosts_return() {
    for case in family("diagnostic-parity")["cases"].as_array().unwrap() {
        let name = text(case, "name");
        let directory = Scratch::new("parity");
        // A case whose specification is absent writes no file.
        if let Some(specification) = case["specification_yaml"].as_str() {
            directory.write("spec.yaml", specification);
            directory.write("input.csv", bytes(&case["input_csv"]));
        }
        let document = document(&directory, &[text(case, "call"), "spec.yaml"], name);
        // `{entry}` stands for the specification's absolute path as the
        // engine resolves it.
        assert_eq!(
            printed_rows(&document, name),
            truth_rows(
                &case["issues"],
                &directory,
                &[
                    ("{path}", "spec.yaml".to_owned()),
                    ("{entry}", directory.spelled("spec.yaml")),
                ]
            ),
            "{name}"
        );
    }
}

/// Each case of `project-parity` prints every issue row that both hosts
/// return for the same call: a project check, a project run, or one
/// specification or one submission of a project. A case that makes several
/// calls on its files lists each with its rows under `calls`. A member's rows
/// are its issues in the run; the command line runs no project function, so
/// the cases that call one give its own rows.
#[test]
fn each_project_parity_case_prints_the_issue_rows_both_hosts_return() {
    for case in family("project-parity")["cases"].as_array().unwrap() {
        let name = text(case, "name");
        let directory = Scratch::new("parity");
        let mut files = vec![&case["files"]];
        if let Some(hosts) = case.get("host_files") {
            files.push(for_command_line(hosts, "files", name));
        }
        for (file, contents) in files
            .into_iter()
            .flat_map(|files| files.as_object().unwrap())
        {
            directory.write(file, contents.as_str().unwrap());
        }
        let calls = match case.get("calls") {
            Some(calls) => calls
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| (&entry["call"], &entry["issues"]))
                .collect(),
            None => vec![(&case["call"], &case["issues"])],
        };
        for (call, issues) in calls {
            let path = text(call, "path");
            let operation = text(call, "operation");
            let arguments = match operation {
                "project_check" => vec!["check", path],
                "project_run" => match call["section"].as_str() {
                    Some(section) => vec!["run", path, "--section", section],
                    None => vec!["run", path],
                },
                "spec_run" => vec!["run", path, "--spec", text(call, "name")],
                "submission" => vec![
                    "submission",
                    path,
                    "--section",
                    text(call, "section"),
                    "--created",
                    text(call, "created"),
                ],
                other => {
                    panic!("{name}: the command line has no call for the operation {other}")
                }
            };
            let document = document(&directory, &arguments, name);
            let mut printed = printed_rows(&document, name);
            if let Some(member) = call["member"].as_str() {
                printed = member_rows(&document, printed, member, name);
            }
            let stand_ins = [
                ("{root}", directory.spelled("")),
                ("{version}", yamaa::VERSION.to_owned()),
            ];
            assert_eq!(
                printed,
                truth_rows(
                    for_command_line(issues, "issues", name),
                    &directory,
                    &stand_ins
                ),
                "{name}: {operation}"
            );
        }
    }
}

/// `include-fragments`: each check case of the study prints every issue row
/// that both hosts return for it, each fragment checked on a given parent the
/// rows of the child that includes it under that parent, each refused parent
/// its rows and error text (#3694), and `resolve` prints the facts both hosts
/// compare: the layers in contribution order, the `parent` and `include`
/// resources, the resolved columns, the layer each named value comes from and
/// the fields the resolved specification never holds (LAYER-1 to LAYER-4,
/// LAYER-14).
#[test]
fn the_include_fragments_study_prints_the_rows_and_facts_both_hosts_return() {
    let truth = family("include-fragments");
    let directory = Scratch::new("parity-include");
    for (file, contents) in truth["files"].as_object().unwrap() {
        directory.write(file, contents.as_str().unwrap());
    }
    let stand_ins = [("{root}", directory.spelled(""))];
    for case in truth["checks"].as_array().unwrap() {
        let name = text(case, "name");
        let document = document(&directory, &["check", text(case, "path")], name);
        assert_eq!(
            printed_rows(&document, name),
            truth_rows(&case["issues"], &directory, &stand_ins),
            "{name}"
        );
    }
    // A fragment checked on a given parent prints the rows that a child
    // naming that parent and including the fragment prints (#3694).
    for case in truth["parent_checks"].as_array().unwrap() {
        let name = text(case, "name");
        let rows = truth_rows(&case["issues"], &directory, &stand_ins);
        for arguments in [
            [
                "check",
                text(case, "fragment"),
                "--parent",
                text(case, "parent"),
            ]
            .as_slice(),
            &["check", text(case, "child")],
        ] {
            let document = document(&directory, arguments, name);
            assert_eq!(
                printed_rows(&document, name),
                rows,
                "{name}: yamaa {arguments:?}"
            );
        }
    }
    // A parent given for a file that is no fragment is refused: the error text
    // and the issue row both hosts raise.
    for case in truth["parent_refusals"].as_array().unwrap() {
        let name = text(case, "name");
        let arguments = [
            "check",
            text(case, "path"),
            "--parent",
            text(case, "parent"),
        ];
        // `{parent}` stands for the parent the case gave.
        let refused = [stand_ins[0].clone(), ("{parent}", arguments[3].to_owned())];
        let document = document(&directory, &arguments, name);
        assert_eq!(
            printed_rows(&document, name),
            truth_rows(&case["issues"], &directory, &refused),
            "{name}"
        );
        let output = yamaa_in(&directory, &arguments);
        assert_eq!(
            (
                output.status.code(),
                String::from_utf8(output.stdout).unwrap()
            ),
            (
                Some(1),
                format!(
                    "{}\n",
                    text(case, "message")
                        .replace("{path}", arguments[1])
                        .replace("{parent}", arguments[3])
                )
            ),
            "{name}: the refusal's text"
        );
    }
    let resolve = &truth["resolve"];
    let output = yamaa_in(
        &directory,
        &["--format", "json", "resolve", text(resolve, "path")],
    );
    assert_eq!(output.status.code(), Some(0), "resolve succeeds");
    let facts: Value = serde_json::from_slice(&output.stdout).unwrap();
    let entry = &facts["entries"][0];
    let located = |name: &Value| Value::String(directory.spelled(name.as_str().unwrap()));
    assert_eq!(
        entry["layers"],
        Value::Array(
            resolve["layers"]
                .as_array()
                .unwrap()
                .iter()
                .map(located)
                .collect()
        ),
        "the layers in contribution order"
    );
    let layered = entry["resources"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|resource| {
            matches!(
                resource["role"].as_str(),
                Some("parent_specification" | "included_specification")
            )
        })
        .map(|resource| {
            serde_json::json!({
                "spec_path": resource["spec_path"],
                "role": resource["role"],
                "declaring_source": resource["declaring_source"],
                "written": resource["written"],
                "location": resource["location"]["path"],
            })
        })
        .collect::<Vec<_>>();
    let expected = resolve["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|resource| {
            let mut resource = resource.clone();
            resource["declaring_source"] = located(&resource["declaring_source"]);
            resource["location"] = located(&resource["location"]);
            resource
        })
        .collect::<Vec<_>>();
    assert_eq!(layered, expected, "the parent and include resources");
    let specification = &entry["specification"];
    assert_eq!(
        specification["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|column| column["name"].clone())
            .collect::<Vec<_>>(),
        resolve["columns"].as_array().unwrap().clone(),
        "the resolved columns"
    );
    for (pointer, name) in resolve["declaring_sources"].as_object().unwrap() {
        assert_eq!(
            entry["origins"][pointer]["declaring_source"],
            located(name),
            "the layer {pointer} comes from"
        );
    }
    for field in resolve["absent_fields"].as_array().unwrap() {
        let field = field.as_str().unwrap();
        assert!(
            specification.get(field).is_none(),
            "the resolved specification holds no {field}"
        );
    }
}

/// `issue-locations`: each case writes its files and makes its call, and the
/// first issue the document prints has the condition, the first spec path,
/// and the file, line and column that both hosts compare (P-ISSUE-1). The
/// hosts' selection of a project's specification by name is the command
/// line's check of the project with `--spec`.
#[test]
fn each_issue_locations_case_prints_its_first_issue_where_both_hosts_locate_it() {
    for case in family("issue-locations")["cases"].as_array().unwrap() {
        let name = text(case, "name");
        let directory = Scratch::new("parity-locations");
        for (file, contents) in case["files"].as_object().unwrap() {
            directory.write(file, contents.as_str().unwrap());
        }
        let call = &case["call"];
        let path = text(call, "path");
        let arguments = match text(call, "operation") {
            "check" | "project_check" => vec!["check", path],
            "run" => vec!["run", path],
            "select" => vec!["check", path, "--spec", text(call, "name")],
            "submission" => vec![
                "submission",
                path,
                "--section",
                text(call, "section"),
                "--created",
                text(call, "created"),
            ],
            other => panic!("{name}: the command line has no call for the operation {other}"),
        };
        let rows = printed_rows(&document(&directory, &arguments, name), name);
        let first = rows
            .first()
            .unwrap_or_else(|| panic!("{name}: the document holds no issue"));
        let column = |key: &str| {
            first
                .iter()
                .find(|(column, _)| *column == key)
                .map(|(_, value)| value.clone())
                .unwrap()
        };
        let issue = &case["issue"];
        let file = match issue["file"].as_str() {
            Some(file) => Value::String(directory.spelled(file)),
            None => Value::Null,
        };
        assert_eq!(
            (
                column("condition"),
                column("spec_paths")
                    .as_array()
                    .and_then(|paths| paths.first())
                    .cloned()
                    .unwrap_or(Value::Null),
                column("file"),
                column("line"),
                column("column"),
            ),
            (
                issue["condition"].clone(),
                issue["spec_path"].clone(),
                file,
                issue["line"].clone(),
                issue["column"].clone(),
            ),
            "{name}"
        );
    }
}

/// `parquet-containment`: each invalid file, one whose INT96 column statistic
/// holds 13 bytes rather than 12 and one whose two row groups name one page
/// whose header holds fields parquet 55 could not skip, is refused as the one
/// issue row that both hosts return, and nothing reaches standard error; the
/// readable file builds, with no issue, and saves the CSV that both hosts save.
#[test]
fn an_invalid_parquet_file_prints_the_issue_row_both_hosts_return() {
    let truth = family("parquet-containment");
    let write = |directory: &Scratch, file: &str| {
        directory.write("spec.yaml", text(truth, "specification_yaml"));
        directory.write("input.parquet", bytes(&truth[file]));
    };
    let document_of =
        |directory: &Scratch, case: &str| document(directory, &["run", "spec.yaml"], case);
    for file in [
        "long_int96_statistic_parquet",
        "shared_presented_page_parquet",
    ] {
        let invalid = Scratch::new("parity");
        write(&invalid, file);
        assert_eq!(
            printed_rows(&document_of(&invalid, file), file),
            truth_rows(&truth["issues"], &invalid, &[]),
            "{file}"
        );
    }
    let readable = Scratch::new("parity");
    write(&readable, "readable_parquet");
    assert_eq!(
        printed_rows(&document_of(&readable, "readable"), "readable"),
        Vec::<Row>::new(),
        "the readable file"
    );
    assert_eq!(
        std::fs::read_to_string(readable.join("output.csv")).unwrap(),
        text(truth, "readable_csv"),
        "the readable file's output"
    );
}

/// `public-contract.value_level_report`: the run succeeds with no issue and
/// saves the report its specification declares, then its output; the
/// report's value-level rows are the rows both hosts' reports hold, all
/// fourteen columns in order.
#[test]
fn a_run_saves_the_value_level_report_rows_both_hosts_report() {
    let case = &family("public-contract")["value_level_report"];
    let directory = Scratch::new("parity");
    directory.write("spec.yaml", text(case, "specification_yaml"));
    directory.write("input.csv", text(case, "input_csv"));
    let document = document(&directory, &["run", "spec.yaml"], "value_level_report");
    assert_eq!(
        printed_rows(&document, "value_level_report"),
        Vec::<Row>::new(),
        "the value-level run's issues"
    );
    assert_eq!(
        paths(&document, "value_level_report"),
        [
            directory.spelled("report.csv"),
            directory.spelled("output.csv")
        ],
        "the files the value-level run saved"
    );
    let columns = report_columns();
    let row = |cells: Vec<Option<String>>| -> Row {
        assert_eq!(
            cells.len(),
            columns.len(),
            "a report row's cells: {cells:?}"
        );
        let values = cells
            .into_iter()
            .map(|cell| cell.map_or(Value::Null, Value::String));
        columns.iter().copied().zip(values).collect()
    };
    let records = csv_records(&std::fs::read_to_string(directory.join("report.csv")).unwrap());
    let header = records[0].iter().map(|name| name.as_deref().unwrap());
    assert_eq!(header.collect::<Vec<_>>(), columns, "the report's columns");
    let name = columns.iter().position(|column| *column == "NAME").unwrap();
    let saved = records[1..]
        .iter()
        .filter(|record| record[name].as_deref() == Some("value_level"))
        .map(|record| row(record.clone()))
        .collect::<Vec<_>>();
    // A truth cell is text, a count, or null for a missing value.
    let cell = |value: &Value| match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        other => Some(other.to_string()),
    };
    let authored = case["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|check| row(columns.iter().map(|&column| cell(&check[column])).collect()))
        .collect::<Vec<_>>();
    assert_eq!(saved, authored, "the value-level report rows");
}

/// The failed run of `public-contract`, whose specification both hosts
/// assemble from the family's pieces: it fails verification and saves only
/// its report (OUTPUT-10), whose bytes are those both hosts save, every row
/// and all fourteen columns in order.
#[test]
fn a_failed_run_saves_the_report_bytes_both_hosts_save() {
    let contract = family("public-contract");
    let piece = |key| text(contract, key);
    // Each edit replaces the one place its text is written, as the hosts'
    // edits do; a piece that matched nowhere, or twice, would leave another
    // specification.
    let edited = |text: String, old: &str, new: &str| {
        assert_eq!(text.matches(old).count(), 1, "{old:?} once in {text:?}");
        text.replacen(old, new, 1)
    };
    let mut specification = piece("specification_yaml").to_owned();
    for (old, new) in [
        (
            piece("integer_column_yaml"),
            piece("fatal_integer_column_yaml"),
        ),
        ("output: {", piece("unreached_row_count_yaml")),
        ("columns: [ID, VALUE]}", piece("report_output_yaml")),
    ] {
        specification = edited(specification, old, new);
    }
    let directory = Scratch::new("parity");
    directory.write("spec.yaml", specification);
    directory.write("input.csv", piece("failed_input_csv"));
    let document = document(&directory, &["run", "spec.yaml"], "failed_report_csv");
    assert_eq!(
        (
            raw(&document, "status", "failed_report_csv"),
            paths(&document, "failed_report_csv")
        ),
        ("\"failed\"", vec![directory.spelled("checks.csv")]),
        "a failed run saves only its report"
    );
    assert_eq!(
        std::fs::read_to_string(directory.join("checks.csv")).unwrap(),
        piece("failed_report_csv"),
        "the failed run's report"
    );
}

/// The `lint` family's layer cases: a layer that restates its parent's row
/// template without one derivation, and one that writes values exactly as
/// it inherits them, a copied column's label among them, print the rows
/// both hosts return, all six lint columns, and `--fix` writes nothing and
/// leaves the rows, since either meaning may be the author's.
#[test]
fn a_layer_prints_the_lint_rows_both_hosts_return() {
    let lint = family("lint");
    let columns = names(lint, "columns");
    let integers = names(lint, "integer_columns");
    let mut keys = columns.clone();
    keys.sort_unstable();
    for name in ["partial_restatement", "restated_inherited_value"] {
        let case = &lint[name];
        let directory = Scratch::new("parity");
        for (file, contents) in case["files"].as_object().unwrap() {
            directory.write(file, contents.as_str().unwrap());
        }
        let file = text(case, "file_name");
        let authored = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                columns
                    .iter()
                    .map(|&column| {
                        let value = if column == "file" {
                            Value::String(directory.spelled(file))
                        } else if integers.contains(&column) {
                            Value::from(text(row, column).parse::<i64>().unwrap())
                        } else {
                            row[column].clone()
                        };
                        (column, value)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for arguments in [["lint", file].as_slice(), &["lint", "--fix", file]] {
            let document = document(&directory, arguments, name);
            let findings: Vec<BTreeMap<String, Value>> =
                serde_json::from_str(raw(&document, "findings", name)).unwrap();
            let printed = findings
                .iter()
                .map(|finding| {
                    assert_eq!(
                        finding.keys().map(String::as_str).collect::<Vec<_>>(),
                        keys,
                        "{name}: a finding's keys"
                    );
                    columns
                        .iter()
                        .map(|&column| (column, finding[column].clone()))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            assert_eq!(
                (printed, paths(&document, name)),
                (authored.clone(), Vec::<String>::new()),
                "{name}: yamaa {arguments:?}"
            );
            assert_eq!(
                std::fs::read_to_string(directory.join(file)).unwrap(),
                text(&case["files"], file),
                "{name}: yamaa {arguments:?} leaves the layer as written"
            );
        }
    }
}

/// The condition catalogue (#3700): `yamaa conditions` prints a line of the
/// `conditions` family's columns, then one line per condition the family
/// names, in its order, each cell separated by a tab, a list joined with
/// `, ` and a missing cell empty; with `--format json` it prints the status
/// and each condition as an object with exactly those columns, its lists as
/// lists and a missing cell null. Each authored row is the line and the
/// object of its condition, as the hosts' tables hold it, and both forms
/// exit 0 with nothing on standard error.
#[test]
fn the_conditions_command_prints_the_catalogue_both_hosts_return() {
    let truth = family("conditions");
    let columns = names(truth, "columns");
    let conditions = names(truth, "names");
    let rows = truth["rows"].as_array().unwrap();
    let directory = Scratch::new("parity");
    let printed = |arguments: &[&str]| {
        let output = yamaa_in(&directory, arguments);
        let text = |bytes: Vec<u8>| String::from_utf8(bytes).expect("UTF-8 output");
        (
            output.status.code(),
            text(output.stdout),
            text(output.stderr),
        )
    };

    let (status, stdout, stderr) = printed(&["conditions"]);
    assert_eq!((status, stderr.as_str()), (Some(0), ""), "yamaa conditions");
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(
        (
            lines[0].to_owned(),
            lines[1..]
                .iter()
                .map(|line| line.split('\t').next().unwrap())
                .collect::<Vec<_>>()
        ),
        (columns.join("\t"), conditions.clone()),
        "the printed columns and conditions"
    );
    for row in rows {
        let cells = row.as_array().unwrap();
        let line = cells
            .iter()
            .map(|cell| match cell {
                Value::Array(items) => items
                    .iter()
                    .map(|item| item.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(", "),
                Value::Null => String::new(),
                other => other.as_str().unwrap().to_owned(),
            })
            .collect::<Vec<_>>()
            .join("\t");
        let name = cells[0].as_str().unwrap();
        let at = conditions.iter().position(|other| *other == name).unwrap();
        assert_eq!(lines[at + 1], line, "the line of {name}");
    }

    let (status, stdout, stderr) = printed(&["--format", "json", "conditions"]);
    assert_eq!(
        (status, stderr.as_str()),
        (Some(0), ""),
        "yamaa --format json conditions"
    );
    let document: BTreeMap<String, Value> =
        serde_json::from_str(stdout.strip_suffix('\n').unwrap()).unwrap();
    assert_eq!(
        (
            document.keys().map(String::as_str).collect::<Vec<_>>(),
            &document["command"],
            &document["status"]
        ),
        (
            vec!["command", "conditions", "status"],
            &Value::from("conditions"),
            &Value::from("succeeded")
        ),
        "the document"
    );
    let objects: Vec<BTreeMap<String, Value>> =
        serde_json::from_value(document["conditions"].clone()).unwrap();
    let mut keys = columns.clone();
    keys.sort_unstable();
    for object in &objects {
        assert_eq!(
            object.keys().map(String::as_str).collect::<Vec<_>>(),
            keys,
            "the keys of {}",
            object["condition"]
        );
    }
    assert_eq!(
        objects
            .iter()
            .map(|object| object["condition"].as_str().unwrap())
            .collect::<Vec<_>>(),
        conditions,
        "the conditions of the document"
    );
    for row in rows {
        let cells = row.as_array().unwrap();
        let name = cells[0].as_str().unwrap();
        let object = objects
            .iter()
            .find(|object| object["condition"] == cells[0])
            .unwrap();
        assert_eq!(
            columns
                .iter()
                .map(|&column| object[column].clone())
                .collect::<Vec<_>>(),
            *cells,
            "the object of {name}"
        );
    }
}
