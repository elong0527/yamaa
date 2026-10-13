//! One native lifecycle behind both hosts' file entry points.
use crate::{
    file_preparation::{self, FileSpecification},
    issue_rows::Issue,
    specification_report::{self, BuildResult, Identity},
};
use serde_json::Value;
use std::{
    any::Any,
    panic::{catch_unwind, AssertUnwindSafe},
};
use yamaa_engine::domain_entry::{self, Request};

pub type Failure = Vec<Issue>;

pub fn unsupported(operation: &str, path: Option<&str>) -> Failure {
    vec![
        Issue::from_core(yamaa_core::application_issue::unsupported(operation, path))
            .expect("application issue"),
    ]
}
pub fn rejected(stage: &str, code: &str) -> Failure {
    vec![
        Issue::from_core(yamaa_core::application_issue::rejected(stage, code))
            .expect("application issue"),
    ]
}
pub(crate) fn resource_failure(stage: &str, error: &crate::file_resources::Error) -> Failure {
    use crate::file_resources::Error as E;
    let code = match error {
        E::Missing => "resource_missing",
        E::Unreadable => "resource_unreadable",
        E::NotRegularFile => "resource_not_regular",
        E::InvalidRoot => "resource_root",
        E::InvalidBase => "resource_base",
        E::InvalidPath => "resource_path",
        E::OutsideRoots => "resource_outside_roots",
        E::Symlink => "resource_link",
        E::Changed => "resource_changed",
        E::InvalidLock => "invalid_packaging_lock",
        E::Limit => "resource_limit",
    };
    rejected(stage, code)
}
pub(crate) fn publication_failure(error: &crate::file_publication::Error) -> Failure {
    use crate::file_publication::Error as E;
    use yamaa_core::{diagnostic::ContextValue, value::Value};
    let code = match error {
        E::Cleanup { .. } => "cleanup_incomplete",
        E::Changed => "target_changed",
        E::Limit => "publication_limit",
        E::InvalidTarget => "invalid_target",
        E::PathMismatch => "path_mismatch",
        E::Io(_) => {
            yamaa_core::diagnostic::ConditionCode::OutputPublicationFailed
                .definition()
                .condition
        }
    };
    let mut finding = yamaa_core::application_issue::rejected("output", code);
    if let E::Cleanup {
        operation,
        staging,
        cleanup,
    } = error
    {
        for (name, value) in [
            ("operation", operation.message()),
            ("staging", staging.to_string_lossy().into_owned()),
            ("cleanup", cleanup.to_string()),
        ] {
            finding
                .context
                .insert(name.into(), ContextValue::Scalar(Value::Str(value)));
        }
    }
    vec![Issue::from_core(finding).expect("application issue")]
}
pub(crate) fn preparation_failure(message: String) -> Failure {
    let Ok(value) = serde_json::from_str::<Value>(&message) else {
        return rejected("prepare", "resource_boundary");
    };
    let outcome = &value["outcome"];
    match outcome["status"].as_str() {
        Some("invalid") => {
            let Some(rows) = outcome["diagnostics"].as_array() else {
                return rejected("prepare", "issue_projection");
            };
            let Ok(issues) = rows
                .iter()
                .cloned()
                .map(Issue::from_diagnostic)
                .collect::<Result<Vec<_>, _>>()
            else {
                return rejected("prepare", "issue_projection");
            };
            if !crate::issue_rows::within_limit(
                &issues,
                crate::specification_check::MAX_ISSUE_BYTES,
            ) {
                return rejected("prepare", "issue_limit");
            }
            issues
        }
        Some("unsupported") => {
            let Some(features) = outcome["features"].as_array() else {
                return rejected("prepare", "issue_projection");
            };
            let mut issues = Vec::new();
            for feature in features {
                let (Some(operation), Some(path)) =
                    (feature["operation"].as_str(), feature["spec_path"].as_str())
                else {
                    return rejected("prepare", "issue_projection");
                };
                issues.extend(unsupported(operation, Some(path)));
            }
            issues
        }
        Some("rejected") => rejected(
            outcome["stage"].as_str().unwrap_or("prepare"),
            outcome["code"].as_str().unwrap_or("preparation_boundary"),
        ),
        _ => rejected("prepare", "preparation_boundary"),
    }
}

pub struct Domain {
    result: BuildResult,
    output: Option<(crate::public_table::PublicTable, Vec<u8>)>,
    targets: Option<Vec<crate::file_publication::AnchoredTarget>>,
    publication_issues: Failure,
    published: Option<Value>,
    publication_attempt: PublicationAttempt,
}
#[derive(Default)]
struct PublicationAttempt {
    entered: usize,
    completed: usize,
    failure: Option<crate::file_publication::Error>,
    unwind: Option<Box<dyn Any + Send>>,
}
fn publication_at(
    mut issues: Failure,
    field: &str,
    path: &str,
    attempt: &PublicationAttempt,
) -> Failure {
    for issue in &mut issues {
        issue.spec_paths = vec![format!("output.{field}")];
        let mut context: Value =
            serde_json::from_str(&issue.context).expect("native issue context");
        context["path"] = serde_json::json!(path);
        context["entered"] = serde_json::json!(attempt.entered);
        context["completed"] = serde_json::json!(attempt.completed);
        issue.context = context.to_string();
    }
    issues
}
impl Domain {
    /// Use the same owned output and anchored save gate for project reports.
    pub fn from_project(
        result: BuildResult,
        project: &crate::file_project::FileProject,
    ) -> Result<Self, Failure> {
        Self::from_build(result, |field| {
            if field == "path" {
                project.publication_target()
            } else {
                project.publication_field_target(field)
            }
        })
    }
    fn from_build(
        mut result: BuildResult,
        mut target: impl FnMut(
            &str,
        ) -> Result<
            crate::file_publication::AnchoredTarget,
            crate::file_resources::Error,
        >,
    ) -> Result<Self, Failure> {
        let output = result
            .output()
            .map(|bytes| {
                let table = crate::public_table::PublicTable::decode(bytes)
                    .map_err(|_| rejected("build", "output_projection"))?;
                let bytes = table
                    .ipc()
                    .map_err(|_| rejected("build", "output_projection"))?;
                Ok::<_, Failure>((table, bytes))
            })
            .transpose();
        let output = match output {
            Ok(output) => output,
            Err(original) => {
                if result.verification_log().is_none() {
                    return Err(original);
                }
                result
                    .reject_host_output()
                    .map_err(|_| rejected("build", "result_projection"))?;
                None
            }
        };
        let fields = [
            ("verification_log", result.verification_log().is_some()),
            (
                "warning_log",
                result.warning_log().is_some() && result.output().is_some(),
            ),
            ("path", result.output().is_some()),
        ];
        let targets = fields
            .into_iter()
            .filter(|(_, present)| *present)
            .map(|(field, _)| target(field))
            .collect::<Result<Vec<_>, _>>();
        let (targets, publication_issues) = match targets {
            Ok(targets) => (Some(targets), Vec::new()),
            Err(error) => (None, resource_failure("output", &error)),
        };
        Ok(Self {
            result,
            output,
            targets,
            publication_issues,
            published: None,
            publication_attempt: Default::default(),
        })
    }
    pub fn output(&self) -> Option<&[u8]> {
        self.output.as_ref().map(|(_, bytes)| bytes.as_slice())
    }
    pub fn output_table(&self) -> Option<&crate::public_table::PublicTable> {
        self.output.as_ref().map(|(table, _)| table)
    }
    pub fn verification_log(&self) -> Option<&[u8]> {
        self.result.verification_log()
    }
    pub fn warning_log(&self) -> Option<&[u8]> {
        self.result.warning_log()
    }
    pub fn log_table(&self, warning: bool) -> Option<&crate::public_table::PublicTable> {
        self.result.log_table(warning)
    }
    pub fn issues(&self) -> Failure {
        self.result
            .issues()
            .iter()
            .cloned()
            .chain(self.publication_issues.iter().cloned())
            .collect()
    }
    /// Retain complete run observations without recapture. Artifacts describe
    /// only a successful latest save; a refused save returns build observations.
    pub fn observations(&self) -> Value {
        self.published
            .clone()
            .unwrap_or_else(|| self.result.observations())
    }
    /// Preflight the complete target set before writing held log/primary bytes.
    /// Failed builds may publish their diagnostic log on explicit Save and still
    /// raise the original failed-build result. Build itself never writes.
    pub fn save(&mut self) -> Result<bool, FailedBuild> {
        self.save_with(16_777_216, |publisher, path, bytes| {
            publisher.publish(path, bytes)
        })
    }
    fn save_with(
        &mut self,
        report_limit: usize,
        mut publish: impl FnMut(
            &mut crate::file_publication::Publisher,
            &str,
            &[u8],
        ) -> Result<(), crate::file_publication::Error>,
    ) -> Result<bool, FailedBuild> {
        let failed = self.result.output().is_none();
        if failed && self.result.verification_log().is_none() {
            return Err(FailedBuild);
        }
        self.published = None;
        self.publication_attempt = Default::default();
        let Some(targets) = self.targets.as_ref() else {
            return if failed { Err(FailedBuild) } else { Ok(false) };
        };
        let items = self.result.publication_items();
        // Every at-most-three prefix report is admitted and owned before any
        // target is published. Later successful calls only move held evidence.
        let mut budget = crate::report_projection::Budget::new(report_limit);
        let reports = (0..=items.len())
            .map(|completed| {
                self.result
                    .publication_observations(completed, &mut budget)
                    .map(Some)
            })
            .collect::<Result<Vec<_>, _>>();
        let mut reports = match reports {
            Ok(reports) => reports,
            Err(_) => {
                self.publication_issues = rejected("output", "publication_report_limit");
                return if failed { Err(FailedBuild) } else { Ok(false) };
            }
        };
        let fields = [
            ("verification_log", self.result.verification_log().is_some()),
            (
                "warning_log",
                self.result.warning_log().is_some() && !failed,
            ),
            ("path", !failed),
        ]
        .into_iter()
        .filter_map(|(field, present)| present.then_some(field))
        .collect::<Vec<_>>();
        let publishers = targets
            .iter()
            .zip(&items)
            .map(|(target, (path, _, _))| target.publisher(path))
            .collect::<Result<Vec<_>, _>>();
        self.publication_issues.clear();
        let mut publishers = match publishers {
            Ok(publishers) => publishers,
            Err(error) => {
                self.publication_issues = publication_failure(&error);
                self.publication_attempt.failure = Some(error);
                return if failed { Err(FailedBuild) } else { Ok(false) };
            }
        };
        // Physical alias checks are performed on these retained parent handles
        // before the first publication call (see Publisher::same_target).
        for index in 0..publishers.len() {
            for prior in &publishers[..index] {
                match publishers[index].same_target(prior) {
                    Ok(false) => (),
                    result => {
                        let error = result
                            .err()
                            .unwrap_or(crate::file_publication::Error::InvalidTarget);
                        self.publication_issues = publication_failure(&error);
                        self.publication_attempt.failure = Some(error);
                        return if failed { Err(FailedBuild) } else { Ok(false) };
                    }
                }
            }
        }
        self.published = reports[0].take();
        for (index, (publisher, (path, bytes, _))) in publishers.iter_mut().zip(items).enumerate() {
            self.publication_attempt.entered = index + 1;
            match catch_unwind(AssertUnwindSafe(|| publish(publisher, path, bytes))) {
                Ok(Ok(())) => {
                    self.publication_attempt.completed = index + 1;
                    self.published = reports[index + 1].take();
                }
                Ok(Err(error)) => {
                    self.publication_issues = publication_at(
                        publication_failure(&error),
                        fields[index],
                        path,
                        &self.publication_attempt,
                    );
                    self.publication_attempt.failure = Some(error);
                    return if failed { Err(FailedBuild) } else { Ok(false) };
                }
                Err(payload) => {
                    self.publication_attempt.unwind = Some(payload);
                    self.publication_issues = publication_at(
                        rejected("output", "publication_unwind"),
                        fields[index],
                        path,
                        &self.publication_attempt,
                    );
                    return if failed { Err(FailedBuild) } else { Ok(false) };
                }
            }
        }
        if failed {
            Err(FailedBuild)
        } else {
            Ok(true)
        }
    }
}
#[derive(Debug)]
pub struct FailedBuild;

struct Native<'a> {
    identity: Identity<'a>,
}
/// The host provides activation and original-error conversion. All route
/// preparation, static checks and dispatch stay behind the shared application.
pub trait RuntimePort {
    type Result;
    type Error;
    fn plain(&mut self, prepared: FileSpecification) -> Result<Self::Result, Self::Error>;
    fn project(
        &mut self,
        prepared: Box<crate::file_project::FileProject>,
    ) -> Result<Self::Result, Self::Error>;
    fn producer(
        &mut self,
        prepared: Box<crate::file_graph::FileGraph>,
    ) -> Result<Self::Result, Self::Error>;
}
pub enum ApplicationError<E> {
    Preparation(crate::file_workflow::SelectionError),
    Runtime(E),
}
pub struct Application<R> {
    pub host: yamaa_engine::host_values::Language,
    pub runtime: R,
}
impl<R: RuntimePort> yamaa_engine::build::Ports for Application<R> {
    type Plain = FileSpecification;
    type Project = Box<crate::file_project::FileProject>;
    type Producer = Box<crate::file_graph::FileGraph>;
    type CheckResult = Failure;
    type BuildResult = R::Result;
    type Failure = ApplicationError<R::Error>;
    fn prepare(
        &mut self,
        request: Request<'_>,
    ) -> Result<crate::file_workflow::Selected, Self::Failure> {
        crate::file_workflow::select_request(request, self.host)
            .map_err(ApplicationError::Preparation)
    }
    fn check_plain(&mut self, prepared: FileSpecification) -> Result<Failure, Self::Failure> {
        Ok(crate::specification_check::issue_rows(prepared.run())
            .unwrap_or_else(|_| rejected("check", "issue_projection")))
    }
    fn check_project(&mut self, prepared: Self::Project) -> Result<Failure, Self::Failure> {
        Ok(crate::project_check::prepared(prepared.run())
            .unwrap_or_else(|_| rejected("check", "issue_projection")))
    }
    fn check_producer(&mut self, prepared: Self::Producer) -> Result<Failure, Self::Failure> {
        Ok(crate::producer_check::prepared(&prepared)
            .unwrap_or_else(|_| rejected("check", "issue_projection")))
    }
    fn build_plain(&mut self, prepared: FileSpecification) -> Result<R::Result, Self::Failure> {
        self.runtime
            .plain(prepared)
            .map_err(ApplicationError::Runtime)
    }
    fn build_project(&mut self, prepared: Self::Project) -> Result<R::Result, Self::Failure> {
        self.runtime
            .project(prepared)
            .map_err(ApplicationError::Runtime)
    }
    fn build_producer(&mut self, prepared: Self::Producer) -> Result<R::Result, Self::Failure> {
        self.runtime
            .producer(prepared)
            .map_err(ApplicationError::Runtime)
    }
}
impl RuntimePort for Native<'_> {
    type Result = Domain;
    type Error = Failure;
    fn plain(&mut self, mut prepared: FileSpecification) -> Result<Domain, Failure> {
        let attempt = prepared.build();
        if let Some(error) = file_preparation::opaque_resource_failure(&attempt) {
            return Err(resource_failure("capture", error));
        }
        let result = specification_report::build_result(
            prepared.run(),
            &attempt,
            Identity {
                runtime: self.identity.runtime,
                runtime_version: self.identity.runtime_version,
                engine_version: self.identity.engine_version,
                example: self.identity.example,
                specification: self.identity.specification,
                base_directory: self.identity.base_directory,
            },
        )
        .map_err(|_| rejected("build", "result_projection"))?;
        Domain::from_build(result, |field| {
            if field == "path" {
                prepared.publication_target()
            } else {
                prepared.publication_field_target(field)
            }
        })
    }
    fn project(&mut self, _: Box<crate::file_project::FileProject>) -> Result<Domain, Failure> {
        Err(unsupported("environment", Some("environment")))
    }
    fn producer(&mut self, _: Box<crate::file_graph::FileGraph>) -> Result<Domain, Failure> {
        Err(unsupported("producer_host", None))
    }
}

/// Convert an already checked plain capability without recapturing metadata.
pub fn plain(prepared: FileSpecification, identity: Identity<'_>) -> Result<Domain, Failure> {
    Native { identity }.plain(prepared)
}
fn native(identity: Identity<'_>) -> Application<Native<'_>> {
    Application {
        host: if identity.runtime == "r" {
            yamaa_engine::host_values::Language::R
        } else {
            yamaa_engine::host_values::Language::Python
        },
        runtime: Native { identity },
    }
}
pub fn check(request: Request<'_>, identity: Identity<'_>) -> Failure {
    let mut application = native(identity);
    domain_entry::check(request, &mut application).unwrap_or_else(|error| match error {
        ApplicationError::Preparation(error) => {
            crate::project_application::selection_failure(&error, application.host)
        }
        ApplicationError::Runtime(issues) => issues,
    })
}
pub fn domain(request: Request<'_>, identity: Identity<'_>) -> Result<Domain, Failure> {
    let mut application = native(identity);
    domain_entry::domain(request, &mut application).map_err(|error| match error {
        ApplicationError::Preparation(error) => {
            crate::project_application::selection_failure(&error, application.host)
        }
        ApplicationError::Runtime(issues) => issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_configuration;
    use std::{
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Study(PathBuf);
    impl Drop for Study {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn built() -> (Study, Domain) {
        let root = std::env::temp_dir().join(format!(
            "yamaa-save-log-attempt-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("input")).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks/schema-verification-log");
        fs::copy(fixture.join("input/dm.csv"), root.join("input/dm.csv")).unwrap();
        fs::copy(fixture.join("spec.yaml"), root.join("spec.yaml")).unwrap();
        let entry = root.join("spec.yaml").to_str().unwrap().to_owned();
        let result = domain(
            Request {
                specification: &entry,
                environment: None,
            },
            Identity {
                runtime: "test",
                runtime_version: "test",
                engine_version: yamaa_core::VERSION,
                example: "logs",
                specification: &entry,
                base_directory: ".",
            },
        )
        .unwrap();
        fs::remove_file(root.join("input/dm.csv")).unwrap();
        fs::remove_file(root.join("spec.yaml")).unwrap();
        (Study(root), result)
    }
    #[derive(Debug)]
    struct Marker(Arc<()>);
    impl std::fmt::Display for Marker {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("original publication marker")
        }
    }
    impl std::error::Error for Marker {}
    #[test]
    fn actual_late_failures_retain_original_error_entered_prefix_and_retry() {
        for fail_at in 0..3 {
            let (study, mut result) = built();
            let marker = Arc::new(());
            let mut entered = 0;
            assert!(!result
                .save_with(16_777_216, |publisher, path, bytes| {
                    let index = entered;
                    entered += 1;
                    if index == fail_at {
                        return Err(crate::file_publication::Error::Io(std::io::Error::other(
                            Marker(Arc::clone(&marker)),
                        )));
                    }
                    publisher.publish(path, bytes)
                })
                .unwrap());
            assert_eq!(entered, fail_at + 1);
            assert_eq!(result.publication_attempt.entered, fail_at + 1);
            assert_eq!(result.publication_attempt.completed, fail_at);
            let Some(crate::file_publication::Error::Io(error)) =
                &result.publication_attempt.failure
            else {
                panic!("original IO error was lost")
            };
            assert!(Arc::ptr_eq(
                &error.get_ref().unwrap().downcast_ref::<Marker>().unwrap().0,
                &marker
            ));
            assert_eq!(
                result.observations()["artifacts"].as_array().unwrap().len(),
                fail_at
            );
            for (index, name) in ["adsl-checks.csv", "adsl-warnings.csv", "adsl.csv"]
                .iter()
                .enumerate()
            {
                assert_eq!(study.0.join(name).exists(), index < fail_at);
            }
            let issue = result.issues().pop().unwrap();
            assert_eq!(
                issue.spec_paths,
                [format!(
                    "output.{}",
                    ["verification_log", "warning_log", "path"][fail_at]
                )]
            );
            let context: Value = serde_json::from_str(&issue.context).unwrap();
            assert_eq!(context["entered"], fail_at + 1);
            assert_eq!(context["completed"], fail_at);
            assert!(result.save().unwrap());
            assert!(result.publication_attempt.failure.is_none());
            assert!(result.issues().is_empty());
            assert_eq!(
                result.observations()["artifacts"].as_array().unwrap().len(),
                3
            );
        }
    }
    #[test]
    fn native_unwind_keeps_the_failing_boundary_and_original_payload() {
        let (study, mut result) = built();
        let marker = Arc::new(());
        let mut entered = 0;
        assert!(!result
            .save_with(16_777_216, |publisher, path, bytes| {
                entered += 1;
                if entered == 2 {
                    std::panic::panic_any(Arc::clone(&marker));
                }
                publisher.publish(path, bytes)
            })
            .unwrap());
        assert!(Arc::ptr_eq(
            result
                .publication_attempt
                .unwind
                .as_ref()
                .unwrap()
                .downcast_ref::<Arc<()>>()
                .unwrap(),
            &marker
        ));
        assert_eq!(result.publication_attempt.entered, 2);
        assert_eq!(result.publication_attempt.completed, 1);
        assert_eq!(
            result.observations()["artifacts"].as_array().unwrap().len(),
            1
        );
        assert!(study.0.join("adsl-checks.csv").exists());
        assert!(!study.0.join("adsl-warnings.csv").exists());
        assert!(!study.0.join("adsl.csv").exists());
        assert!(result.save().unwrap());
        assert!(result.publication_attempt.unwind.is_none());
    }
    #[test]
    fn report_copy_refusal_precedes_every_publication_port() {
        let (study, mut result) = built();
        assert!(!result
            .save_with(1, |_, _, _| panic!("refused report entered publisher"))
            .unwrap());
        assert_eq!(result.publication_attempt.entered, 0);
        for name in ["adsl-checks.csv", "adsl-warnings.csv", "adsl.csv"] {
            assert!(!study.0.join(name).exists());
        }
        assert!(result.save().unwrap());
    }
    #[test]
    fn actual_host_view_metadata_refusal_preserves_original_code_or_verification_owner() {
        for verification in [false, true] {
            let root = std::env::temp_dir().join(format!(
                "yamaa-host-view-refusal-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).unwrap();
            let study = Study(fs::canonicalize(root).unwrap());
            fs::write(study.0.join("input.csv"), "ID\n1\n").unwrap();
            // Build a real native output whose IPC schema exceeds the public
            // decoder's 64 KiB metadata admission. The test uses the unchanged
            // shipped schema with an explicit larger normalization budget;
            // production admission limits and output bytes remain unchanged.
            let names = (0..39)
                .map(|index| format!("V{index}_{}", "a".repeat(2048)))
                .collect::<Vec<_>>();
            let columns = std::iter::once("ID")
                .chain(names.iter().map(String::as_str))
                .collect::<Vec<_>>();
            let mut raw = format!(
                "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {{SRC: input.csv}}\noutput: {{path: out.csv, columns: [{}]{} }}\ncolumns:\n",
                columns.join(", "),
                if verification { ", verification_log: checks.csv" } else { "" }
            );
            for name in columns {
                raw.push_str(&format!(
                    "  - {{name: {name}, type: int, derivation: SRC.ID}}\n"
                ));
            }
            let entry = study.0.join("spec.yaml");
            fs::write(&entry, raw).unwrap();
            let written = entry.to_str().unwrap();
            let (mut resources, entry) = file_configuration::resources(written).unwrap();
            let mut limits = crate::specification_source::Limits::default();
            limits.normalization.validation.regex.state_cells = 32_000_000;
            limits.normalization.validation.regex.work = 32_000_000;
            limits.normalization.validation.regex.subject_bytes = 32_000_000;
            let schema = crate::specification_source::CapturedSchema::admit(
                crate::shipped_schema::capture().unwrap().sources().to_vec(),
                0,
                limits,
            )
            .unwrap();
            let document =
                crate::file_preparation::prepare_document(&mut resources, &entry, Some(schema))
                    .unwrap();
            let prepared = crate::specification_run::PreparedRun::prepare(document).unwrap();
            let attempt = prepared.execute_with_port(&mut resources);
            assert!(attempt.result.as_ref().unwrap().execution.is_some());
            let result = specification_report::build_result(
                &prepared,
                &attempt,
                Identity {
                    runtime: "test",
                    runtime_version: "test",
                    engine_version: yamaa_core::VERSION,
                    example: "host-view-refusal",
                    specification: written,
                    base_directory: ".",
                },
            )
            .unwrap();
            assert!(result.issues().is_empty());
            assert!(crate::public_table::PublicTable::decode(result.output().unwrap()).is_err());
            let domain = Domain::from_build(result, |field| {
                let origin = prepared
                    .document()
                    .written_output_field_origin(field)
                    .ok_or(crate::file_resources::Error::InvalidPath)?;
                resources.publication_target_from(origin.declaring_source, origin.written)
            });
            if !verification {
                let failures = match domain {
                    Err(failures) => failures,
                    Ok(_) => panic!("rejected public view acquired a Domain owner"),
                };
                assert_eq!(failures.len(), 1);
                assert_eq!(
                    failures[0].condition,
                    crate::test_diagnostic_truth::text("implementation-rejection", "condition")
                );
                assert_eq!(
                    serde_json::from_str::<Value>(&failures[0].context).unwrap(),
                    serde_json::json!({"stage":"build","code":"output_projection"})
                );
            } else {
                let mut domain = domain.unwrap();
                assert!(domain.output().is_none());
                assert!(domain.warning_log().is_none());
                assert_eq!(domain.log_table(false).unwrap().rows(), 0);
                assert_eq!(
                    serde_json::from_str::<Value>(&domain.issues()[0].context).unwrap(),
                    serde_json::json!({"stage":"output","code":"output_projection"})
                );
                fs::write(study.0.join("out.csv"), "primary sentinel").unwrap();
                fs::remove_file(study.0.join("input.csv")).unwrap();
                fs::remove_file(study.0.join("spec.yaml")).unwrap();
                for _ in 0..2 {
                    assert!(domain.save().is_err());
                    assert_eq!(
                        fs::read(study.0.join("out.csv")).unwrap(),
                        b"primary sentinel"
                    );
                    assert_eq!(fs::read_to_string(study.0.join("checks.csv")).unwrap(),
                        "REPORT_VERSION,ARTIFACT,SPEC_PATH,VERIFICATION_ID,CHECK,TARGET,REQUIREMENT,SEVERITY,OUTCOME,CONDITION,EVALUATED_COUNT,FAILURE_COUNT,DETAILS\n");
                }
            }
        }
    }
}
