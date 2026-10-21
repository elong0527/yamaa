//! The staleness guard (PROJECT-15). A project submission composes a
//! specification that declares `output.report` only with the artifact and the
//! inputs its saved run report records. The guard compares declared
//! identities only, the facts the report records today (CHECK-24): the `run`
//! row's `ARTIFACT` and `COUNT` against the specification's `output.path` and
//! the rows its artifact holds, `submission.has_no_data` against those rows,
//! and each declared input's `input` row against the byte length and record
//! count that input has now. Nothing is timed or hashed, and the report
//! records nothing more (#3030 D151).
use crate::{
    ast,
    diagnostic::{ConditionCode, Context},
    json::Value as Json,
    run_report::{Recorded, RecordedInput, Run},
    specification::{SourceDeclaration, SourceProfile},
    submission_composition::Finding,
};

/// What a stale artifact disagrees with: its finding's `reason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The declared report file is not a run report.
    ReportInvalid,
    /// The `run` row names another `output.path`.
    ArtifactPath,
    /// The `run` row records a failed run, which saved no artifact.
    RunFailed,
    /// The artifact holds another number of rows than the `run` row records.
    RowCount,
    /// `submission.has_no_data` disagrees with whether the artifact holds rows.
    HasNoData,
    /// A declared input has no `input` row.
    InputMissing,
    /// An input's byte length differs from its row's `DETAILS.bytes`.
    InputBytes,
    /// An input's record count differs from its row's `COUNT`.
    InputRecords,
}
impl Reason {
    /// Every reason, in the order the guard compares.
    pub const ALL: [Self; 8] = [
        Self::ReportInvalid,
        Self::ArtifactPath,
        Self::RunFailed,
        Self::RowCount,
        Self::HasNoData,
        Self::InputMissing,
        Self::InputBytes,
        Self::InputRecords,
    ];
    /// The `reason` a finding records.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReportInvalid => "report_invalid",
            Self::ArtifactPath => "artifact_path",
            Self::RunFailed => "run_failed",
            Self::RowCount => "row_count",
            Self::HasNoData => "has_no_data",
            Self::InputMissing => "input_missing",
            Self::InputBytes => "input_bytes",
            Self::InputRecords => "input_records",
        }
    }
}

fn text(value: &str) -> Json {
    Json::String(value.into())
}

/// The guard of one listed specification that declares a report.
pub struct Guard<'a> {
    /// The listing's `output.report`, such as `sdtm.specs[0].output.report`,
    /// where every finding is reported.
    pub path: String,
    /// The specification's dataset: its `domain`.
    pub dataset: &'a str,
    /// The report's path as declared.
    pub report: &'a str,
}
impl Guard<'_> {
    /// One `stale_artifact` finding for `reason`, with the facts it compared:
    /// `recorded`, as the report records it, and `actual`, as composition
    /// reads it now.
    pub fn stale<const N: usize>(&self, reason: Reason, facts: [(&str, Json); N]) -> Finding {
        let mut context = Context::from([
            ("dataset".into(), text(self.dataset)),
            ("report".into(), text(self.report)),
            ("reason".into(), text(reason.as_str())),
        ]);
        context.extend(facts.into_iter().map(|(key, value)| (key.into(), value)));
        Finding {
            code: ConditionCode::StaleArtifact,
            path: self.path.clone(),
            context,
        }
    }
    /// The `run` row records a successful run that wrote `output`, the
    /// specification's `output.path` as declared; the rows it records.
    pub fn run(&self, recorded: &Recorded, output: &str) -> Result<usize, Finding> {
        if recorded.artifact != output {
            return Err(self.stale(
                Reason::ArtifactPath,
                [
                    ("recorded", text(&recorded.artifact)),
                    ("actual", text(output)),
                ],
            ));
        }
        match &recorded.run {
            Run::Succeeded { rows } => Ok(*rows),
            Run::Failed { condition } => {
                Err(self.stale(Reason::RunFailed, [("recorded", text(condition))]))
            }
        }
    }
    /// The artifact holds the `recorded` rows, and holds rows exactly when the
    /// specification does not declare `submission.has_no_data`.
    pub fn artifact(&self, recorded: usize, rows: usize, has_no_data: bool) -> Result<(), Finding> {
        if rows != recorded {
            return Err(self.stale(
                Reason::RowCount,
                [
                    ("recorded", Json::from(recorded)),
                    ("actual", Json::from(rows)),
                ],
            ));
        }
        if has_no_data != (rows == 0) {
            return Err(self.stale(
                Reason::HasNoData,
                [
                    ("has_no_data", Json::Bool(has_no_data)),
                    ("actual", Json::from(rows)),
                ],
            ));
        }
        Ok(())
    }
    /// The `input` row of the declared input `name`.
    pub fn input<'r>(
        &self,
        recorded: &'r Recorded,
        name: &str,
    ) -> Result<&'r RecordedInput, Finding> {
        recorded
            .inputs
            .iter()
            .find(|row| row.name == name)
            .ok_or_else(|| self.stale(Reason::InputMissing, [("input", text(name))]))
    }
    /// The input holds the byte length its row records; a row that records
    /// none, for a host table, matches no file.
    pub fn input_bytes(&self, row: &RecordedInput, bytes: usize) -> Result<(), Finding> {
        if row.bytes == Some(bytes) {
            return Ok(());
        }
        Err(self.stale(
            Reason::InputBytes,
            [
                ("input", text(&row.name)),
                ("recorded", row.bytes.map_or(Json::Null, Json::from)),
                ("actual", Json::from(bytes)),
            ],
        ))
    }
    /// The input decodes to the records its row records; `records` is `None`
    /// when its bytes no longer decode under its profile.
    pub fn input_records(
        &self,
        row: &RecordedInput,
        records: Option<usize>,
    ) -> Result<(), Finding> {
        if row.records.is_some() && row.records == records {
            return Ok(());
        }
        Err(self.stale(
            Reason::InputRecords,
            [
                ("input", text(&row.name)),
                ("recorded", row.records.map_or(Json::Null, Json::from)),
                ("actual", records.map_or(Json::Null, Json::from)),
            ],
        ))
    }
}

/// The declaration that counts a declared input's records as a run decodes
/// them (CHECK-24): its path, its profile, and the encoding and byte-order
/// mark its bytes need, without its declared field types, which never change
/// how many records its profile decodes. `None` when the path selects no
/// profile.
pub fn counted(input: &ast::Input) -> Option<SourceDeclaration> {
    let path = input.source.path.text.as_str();
    Some(SourceDeclaration {
        name: input.name.name.clone(),
        path: path.into(),
        types: Vec::new(),
        profile: SourceProfile::from_path(path)?,
        encoding: input.source.encoding.as_ref().map(|text| text.text.clone()),
        byte_order_mark: input.source.byte_order_mark,
        empty_string_present: input.source.empty_string == ast::EmptyString::Present,
        ordinal: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> Guard<'static> {
        Guard {
            path: "sdtm.specs[0].output.report".into(),
            dataset: "DM",
            report: "dm-report.csv",
        }
    }
    fn recorded(run: Run) -> Recorded {
        Recorded {
            artifact: "dm.csv".into(),
            run,
            inputs: vec![RecordedInput {
                name: "RAW".into(),
                records: Some(3),
                bytes: Some(120),
            }],
        }
    }
    /// The reason and compared facts of a finding, after asserting the
    /// fields every finding records.
    fn facts(finding: Finding) -> (String, Context) {
        assert_eq!(finding.code, ConditionCode::StaleArtifact);
        assert_eq!(finding.path, "sdtm.specs[0].output.report");
        let mut context = finding.context;
        assert_eq!(context.remove("dataset"), Some(text("DM")));
        assert_eq!(context.remove("report"), Some(text("dm-report.csv")));
        let Some(Json::String(reason)) = context.remove("reason") else {
            panic!("every finding records its reason")
        };
        (reason, context)
    }

    #[test]
    fn a_matching_report_passes_every_comparison() {
        let guard = guard();
        let recorded = recorded(Run::Succeeded { rows: 2 });
        assert_eq!(guard.run(&recorded, "dm.csv"), Ok(2));
        assert_eq!(guard.artifact(2, 2, false), Ok(()));
        assert_eq!(guard.artifact(0, 0, true), Ok(()));
        let row = guard.input(&recorded, "RAW").unwrap();
        assert_eq!(guard.input_bytes(row, 120), Ok(()));
        assert_eq!(guard.input_records(row, Some(3)), Ok(()));
    }

    #[test]
    fn each_mismatch_names_its_reason_and_the_facts_it_compared() {
        let guard = guard();
        let succeeded = recorded(Run::Succeeded { rows: 2 });
        // The condition a failed run records, from the registry.
        let condition = ConditionCode::OutputDuplicateKeyFailed
            .definition()
            .condition;
        let failed = recorded(Run::Failed {
            condition: condition.into(),
        });
        let row = &succeeded.inputs[0];
        let never_read = RecordedInput {
            name: "RAW".into(),
            records: None,
            bytes: None,
        };
        let cases = [
            (
                guard.stale(Reason::ReportInvalid, []),
                "report_invalid",
                vec![],
            ),
            (
                guard.run(&succeeded, "dm_v2.csv").unwrap_err(),
                "artifact_path",
                vec![("recorded", text("dm.csv")), ("actual", text("dm_v2.csv"))],
            ),
            (
                guard.run(&failed, "dm.csv").unwrap_err(),
                "run_failed",
                vec![("recorded", text(condition))],
            ),
            (
                guard.artifact(2, 3, false).unwrap_err(),
                "row_count",
                vec![("recorded", Json::from(2)), ("actual", Json::from(3))],
            ),
            (
                guard.artifact(2, 2, true).unwrap_err(),
                "has_no_data",
                vec![("has_no_data", Json::Bool(true)), ("actual", Json::from(2))],
            ),
            (
                guard.artifact(0, 0, false).unwrap_err(),
                "has_no_data",
                vec![
                    ("has_no_data", Json::Bool(false)),
                    ("actual", Json::from(0)),
                ],
            ),
            (
                guard.input(&succeeded, "AE").unwrap_err(),
                "input_missing",
                vec![("input", text("AE"))],
            ),
            (
                guard.input_bytes(row, 121).unwrap_err(),
                "input_bytes",
                vec![
                    ("input", text("RAW")),
                    ("recorded", Json::from(120)),
                    ("actual", Json::from(121)),
                ],
            ),
            (
                guard.input_bytes(&never_read, 120).unwrap_err(),
                "input_bytes",
                vec![
                    ("input", text("RAW")),
                    ("recorded", Json::Null),
                    ("actual", Json::from(120)),
                ],
            ),
            (
                guard.input_records(row, Some(4)).unwrap_err(),
                "input_records",
                vec![
                    ("input", text("RAW")),
                    ("recorded", Json::from(3)),
                    ("actual", Json::from(4)),
                ],
            ),
            (
                guard.input_records(row, None).unwrap_err(),
                "input_records",
                vec![
                    ("input", text("RAW")),
                    ("recorded", Json::from(3)),
                    ("actual", Json::Null),
                ],
            ),
            (
                guard.input_records(&never_read, None).unwrap_err(),
                "input_records",
                vec![
                    ("input", text("RAW")),
                    ("recorded", Json::Null),
                    ("actual", Json::Null),
                ],
            ),
        ];
        for (finding, reason, compared) in cases {
            let (actual, context) = facts(finding);
            assert_eq!(actual, reason);
            assert_eq!(
                context,
                compared
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value))
                    .collect::<Context>(),
                "{reason}"
            );
        }
        let names = Reason::ALL.map(Reason::as_str);
        assert_eq!(
            names,
            [
                "report_invalid",
                "artifact_path",
                "run_failed",
                "row_count",
                "has_no_data",
                "input_missing",
                "input_bytes",
                "input_records"
            ]
        );
    }
}
