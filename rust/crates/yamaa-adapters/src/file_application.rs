//! One native lifecycle behind both hosts' file entry points.
use crate::{
    file_configuration,
    file_preparation::{self, FileSpecification},
    issue_rows::Issue,
    specification_report::{self, BuildResult, Identity},
};
use serde_json::Value;
use yamaa_engine::domain_entry::{self, Ports, Request};

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
fn resource_failure(stage: &str, error: &crate::file_resources::Error) -> Failure {
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
        E::Limit => "resource_limit",
    };
    rejected(stage, code)
}
fn publication_failure(error: &crate::file_publication::Error) -> Failure {
    use crate::file_publication::Error as E;
    use yamaa_core::{diagnostic::ContextValue, value::Value};
    let code = match error {
        E::Cleanup { .. } => "cleanup_incomplete",
        E::Changed => "target_changed",
        E::Limit => "publication_limit",
        E::InvalidTarget => "invalid_target",
        E::PathMismatch => "path_mismatch",
        E::Io(_) => "publication_failed",
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
fn preparation_failure(message: String) -> Failure {
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
    declared: String,
    target: Option<String>,
    publication_issues: Failure,
}
impl Domain {
    pub fn output(&self) -> Option<&[u8]> {
        self.output.as_ref().map(|(_, bytes)| bytes.as_slice())
    }
    pub fn output_table(&self) -> Option<&crate::public_table::PublicTable> {
        self.output.as_ref().map(|(table, _)| table)
    }
    pub fn issues(&self) -> Failure {
        self.result
            .issues()
            .iter()
            .cloned()
            .chain(self.publication_issues.iter().cloned())
            .collect()
    }
    /// Failed builds never construct a publisher. Operational save failures
    /// remain issues; successful retry clears only the preceding save refusal.
    pub fn save(&mut self) -> Result<bool, FailedBuild> {
        if self.result.output().is_none() {
            return Err(FailedBuild);
        }
        let Some(target) = self.target.as_ref() else {
            return Ok(false);
        };
        let published = crate::file_publication::Publisher::new(&self.declared, target).and_then(
            |mut publisher| {
                self.result
                    .save(&mut publisher)
                    .map(|_| ())
                    .map_err(|error| match error {
                        yamaa_engine::specification_output::SaveError::Publish(error) => error,
                        yamaa_engine::specification_output::SaveError::FailedBuild => {
                            unreachable!("accepted output target")
                        }
                    })
            },
        );
        self.publication_issues.clear();
        match published {
            Ok(()) => Ok(true),
            Err(error) => {
                self.publication_issues = publication_failure(&error);
                Ok(false)
            }
        }
    }
}
#[derive(Debug)]
pub struct FailedBuild;

struct Native<'a> {
    identity: Identity<'a>,
}
impl Ports for Native<'_> {
    type Environment = ();
    type Prepared = FileSpecification;
    type CheckResult = Failure;
    type BuildResult = Domain;
    type Failure = Failure;
    fn environment(&mut self, path: Option<&str>) -> Result<(), Failure> {
        if path.is_some() {
            Err(unsupported("environment", Some("environment")))
        } else {
            Ok(())
        }
    }
    fn prepare(&mut self, path: &str) -> Result<FileSpecification, Failure> {
        let (resources, entry) =
            file_configuration::resources(path).map_err(|error| match error {
                file_configuration::Error::Configuration => {
                    rejected("prepare", "project_configuration")
                }
                file_configuration::Error::Decode(_) => {
                    rejected("prepare", "project_configuration_yaml")
                }
                file_configuration::Error::Path(error) => resource_failure("prepare", &error),
            })?;
        FileSpecification::prepare(resources, &entry).map_err(|error| match error {
            file_preparation::Error::Resource(error) => resource_failure("prepare", &error),
            error => preparation_failure(error.into_message()),
        })
    }
    fn check(&mut self, prepared: FileSpecification, _: &()) -> Result<Failure, Failure> {
        crate::specification_check::issue_rows(prepared.run())
            .map_err(|_| rejected("check", "issue_projection"))
    }
    fn activate(&mut self, _: &FileSpecification, _: &()) -> Result<(), Failure> {
        // The closed compiler rejects all function/environment vocabulary before
        // this checkpoint. No activation or reference fallback is admitted yet.
        Ok(())
    }
    fn build(&mut self, mut prepared: FileSpecification, _: &()) -> Result<Domain, Failure> {
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
        let (target, publication_issues) = if result.output().is_some() {
            match prepared.publication_target() {
                Ok(target) => (Some(target), Vec::new()),
                Err(error) => (None, resource_failure("output", &error)),
            }
        } else {
            (None, Vec::new())
        };
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
            .transpose()?;
        Ok(Domain {
            result,
            output,
            declared: prepared.run().compiled().output_path().into(),
            target,
            publication_issues,
        })
    }
}
pub fn check(request: Request<'_>, identity: Identity<'_>) -> Failure {
    domain_entry::check(request, &mut Native { identity }).unwrap_or_else(|issues| issues)
}
pub fn domain(request: Request<'_>, identity: Identity<'_>) -> Result<Domain, Failure> {
    domain_entry::domain(request, &mut Native { identity })
}
