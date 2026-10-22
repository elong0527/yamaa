//! Every use case reads only through the engine's resolver (#3147). A
//! counting resolver serves one benchmark case from memory under the logical
//! root `/`, names that exist nowhere on disk, keeps what a save publishes,
//! and records every request. Each use case succeeds and reproduces the
//! case's authored files, so it read nothing else; the record shows exactly
//! what it read and wrote, and which requests asked for input data.
#![cfg(feature = "yaml")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use yamaa::{
    CancellationToken, Engine, EngineOptions, HostLanguage, MemoryResolver, OdmOptions,
    ResolverFailure, ResolverKind, ResolverOperation, ResolverReply, ResolverRequest,
    ResourceResolver,
};

const CREATED: &str = "2026-09-11T00:00:00";

/// One request as the resolver saw it: its operation and the location it
/// names, the declaring identity's directory joined with the written path.
type Access = (&'static str, String);

struct Counting {
    files: BTreeMap<String, Arc<[u8]>>,
    published: MemoryResolver,
    log: Mutex<Vec<Access>>,
    /// The location of every request for input data, in order.
    inputs: Mutex<Vec<String>>,
}

/// The lexical location `written` names from `declaring`.
fn location(declaring: &str, written: &str) -> String {
    let mut parts = if written.starts_with('/') {
        Vec::new()
    } else {
        declaring.rsplit_once('/').map_or(Vec::new(), |(base, _)| {
            base.split('/').filter(|s| !s.is_empty()).collect()
        })
    };
    for part in written.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            name => parts.push(name),
        }
    }
    format!("/{}", parts.join("/"))
}

impl ResourceResolver for Counting {
    fn request(&self, request: ResolverRequest<'_>) -> Result<ResolverReply, ResolverFailure> {
        let named = match request.operation {
            ResolverOperation::Stage | ResolverOperation::Commit | ResolverOperation::Abort => {
                request.reference.unwrap_or_default().to_owned()
            }
            _ => location(request.declaring, request.written),
        };
        self.log
            .lock()
            .unwrap()
            .push((request.operation.name(), named.clone()));
        if request.kind == ResolverKind::Input {
            self.inputs.lock().unwrap().push(named.clone());
        }
        match request.operation {
            ResolverOperation::Inspect | ResolverOperation::Capture => {
                let bytes = self
                    .files
                    .get(&named)
                    .ok_or_else(|| ResolverFailure::from_code("missing"))?;
                let reference = format!("case:{named}");
                if request.reference.is_some_and(|held| held != reference) {
                    return Err(ResolverFailure::from_code("changed"));
                }
                let bytes =
                    (request.operation == ResolverOperation::Capture).then(|| Arc::clone(bytes));
                Ok(ResolverReply::new(reference, bytes, None))
            }
            _ => self.published.request(request),
        }
    }
}

struct Case {
    directory: PathBuf,
    resolver: Arc<Counting>,
}

impl Case {
    /// Serve a benchmark case's authored files, and `extra` under their
    /// own names, from memory. The expected directory is truth and is not
    /// served.
    fn new(name: &str, extra: &[(&str, &[u8])]) -> Self {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../benchmarks")
            .join(name);
        let mut files = BTreeMap::new();
        let mut pending = vec![(String::new(), directory.clone())];
        while let Some((prefix, path)) = pending.pop() {
            for entry in std::fs::read_dir(&path).unwrap().flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let written = format!("{prefix}/{name}");
                if entry.file_type().unwrap().is_dir() {
                    if written != "/expected" {
                        pending.push((written, entry.path()));
                    }
                } else {
                    files.insert(written, Arc::from(std::fs::read(entry.path()).unwrap()));
                }
            }
        }
        for (name, bytes) in extra {
            files.insert((*name).to_owned(), Arc::from(*bytes));
        }
        Self {
            directory,
            resolver: Arc::new(Counting {
                files,
                published: MemoryResolver::new(),
                log: Mutex::new(Vec::new()),
                inputs: Mutex::new(Vec::new()),
            }),
        }
    }

    fn engine(&self) -> Engine {
        Engine::new(
            EngineOptions::new(HostLanguage::Python, "resolver-test".into(), "0".into())
                .with_resolver(Arc::clone(&self.resolver) as Arc<dyn ResourceResolver>)
                .with_functions(Arc::new(NoFunctions)),
        )
    }

    /// An engine that claims no host, which refuses a project function call
    /// instead of making one, for cases whose specifications call some.
    fn neutral(&self) -> Engine {
        Engine::new(
            EngineOptions::neutral("resolver-test".into(), "0".into())
                .with_resolver(Arc::clone(&self.resolver) as Arc<dyn ResourceResolver>),
        )
    }

    /// Every location asked for as input data since the last call, with the
    /// record cleared.
    fn inputs(&self) -> Vec<String> {
        std::mem::take(&mut *self.resolver.inputs.lock().unwrap())
    }

    fn expected(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.directory.join("expected").join(name)).unwrap()
    }

    fn published(&self, location: &str) -> Vec<u8> {
        self.resolver
            .published
            .published(&format!("memory:{location}"))
            .unwrap_or_else(|| panic!("nothing was published at {location}"))
            .to_vec()
    }

    /// Every distinct operation and location the resolver was asked for
    /// since the last call, with the record cleared.
    fn accesses(&self) -> BTreeSet<Access> {
        self.requests().into_iter().collect()
    }

    /// Every request since the last call, in order and each as often as it
    /// was made, with the record cleared.
    fn requests(&self) -> Vec<Access> {
        std::mem::take(&mut *self.resolver.log.lock().unwrap())
            .into_iter()
            .map(|(operation, named)| {
                // A staging token names its target after its sequence.
                let named = match named.split_once(":memory:") {
                    Some(_) => named
                        .rsplit_once("memory:")
                        .map(|(_, target)| target.to_owned())
                        .unwrap(),
                    None => named.trim_start_matches("memory:").to_owned(),
                };
                (operation, named)
            })
            .collect()
    }
}

use crate::support::NoFunctions;

fn set(accesses: &[(&'static str, &str)]) -> BTreeSet<Access> {
    accesses
        .iter()
        .map(|(operation, named)| (*operation, (*named).to_owned()))
        .collect()
}

const SPEC: [(&str, &str); 2] = [("inspect", "/spec.yaml"), ("capture", "/spec.yaml")];
const PROJECT: [(&str, &str); 2] = [("inspect", "/project.yaml"), ("capture", "/project.yaml")];
const INPUT: [(&str, &str); 2] = [
    ("inspect", "/input/dm_raw.csv"),
    ("capture", "/input/dm_raw.csv"),
];
const SAVE_DM: [(&str, &str); 3] = [
    ("target", "/dm.csv"),
    ("stage", "/dm.csv"),
    ("commit", "/dm.csv"),
];

#[test]
fn a_check_reads_only_metadata_through_the_resolver() {
    let case = Case::new("sdtm-dm-metadata", &[]);
    let engine = case.engine();
    let token = CancellationToken::new();
    assert_eq!(
        engine
            .spec_check("spec.yaml", None, &token)
            .unwrap()
            .issues()
            .rows(),
        0
    );
    assert_eq!(case.accesses(), set(&SPEC));
    assert_eq!(
        engine
            .project_check("project.yaml", &token)
            .unwrap()
            .issues()
            .rows(),
        0
    );
    assert_eq!(case.accesses(), set(&[PROJECT, SPEC].concat()));
    assert_eq!(
        engine.project_spec("project.yaml", "DM").unwrap(),
        "/spec.yaml"
    );
    assert_eq!(case.accesses(), set(&[PROJECT, SPEC].concat()));
    assert_eq!(engine.project_names("project.yaml").unwrap(), ["DM"]);
    assert_eq!(case.accesses(), set(&[PROJECT, SPEC].concat()));
}

/// D45: a project's names come from the project file and its
/// specifications' metadata alone. On every project benchmark they capture no
/// input data, and they are the names of a run of every section, which reads
/// the inputs; a project that the names refuse runs nothing.
#[test]
fn project_names_read_no_input_data_and_name_what_a_run_builds() {
    let benchmarks = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../benchmarks");
    let mut projects = Vec::new();
    for benchmark in std::fs::read_dir(&benchmarks).unwrap().flatten() {
        if !benchmark.file_type().unwrap().is_dir() {
            continue;
        }
        for file in std::fs::read_dir(benchmark.path()).unwrap().flatten() {
            let file = file.file_name().to_string_lossy().into_owned();
            if file.ends_with("project.yaml") {
                let benchmark = benchmark.file_name().to_string_lossy().into_owned();
                projects.push((benchmark, file));
            }
        }
    }
    projects.sort();
    assert!(projects.len() > 30, "{projects:?}");
    let token = CancellationToken::new();
    // The runs that read input data, which shows that the record sees it.
    let mut reading = 0;
    for (benchmark, project) in &projects {
        let case = Case::new(benchmark, &[]);
        let engine = case.neutral();
        let names = engine.project_names(project);
        assert_eq!(case.inputs(), Vec::<String>::new(), "{benchmark}");
        let runs = engine.project_run(project, None, &token).unwrap();
        reading += usize::from(!case.inputs().is_empty());
        match names {
            Ok(names) => assert_eq!(names, runs.names(), "{benchmark}"),
            Err(failure) => {
                assert!(runs.names().is_empty(), "{benchmark}");
                assert!(
                    failure
                        .message
                        .starts_with(&format!("cannot read {project}: ")),
                    "{benchmark}: {}",
                    failure.message
                );
            }
        }
    }
    assert!(
        reading > projects.len() / 2,
        "{reading} of {}",
        projects.len()
    );
}

#[test]
fn a_run_reads_and_saves_only_through_the_resolver() {
    let case = Case::new("sdtm-dm-metadata", &[]);
    let token = CancellationToken::new();
    let mut run = case.engine().spec_run("spec.yaml", None, &token).unwrap();
    assert_eq!(run.issues().rows(), 0);
    assert_eq!(
        case.accesses(),
        set(&[&SPEC[..], &INPUT, &[("target", "/dm.csv")]].concat())
    );
    run.save().unwrap();
    assert_eq!(case.accesses(), set(&SAVE_DM));
    assert_eq!(case.published("/dm.csv"), case.expected("dm.csv"));
}

#[test]
fn a_project_run_reads_and_saves_only_through_the_resolver() {
    let case = Case::new("sdtm-dm-metadata", &[]);
    let token = CancellationToken::new();
    let mut runs = case
        .engine()
        .project_run("project.yaml", None, &token)
        .unwrap();
    assert_eq!(runs.issues().rows(), 0);
    assert_eq!(runs.names(), ["DM"]);
    assert_eq!(
        case.accesses(),
        set(&[&PROJECT[..], &SPEC, &INPUT, &[("target", "/dm.csv")]].concat())
    );
    runs.save().unwrap();
    assert_eq!(case.accesses(), set(&SAVE_DM));
    assert_eq!(case.published("/dm.csv"), case.expected("dm.csv"));
}

#[test]
fn a_submission_reads_and_saves_only_through_the_resolver() {
    let dm = Case::new("sdtm-dm-metadata", &[]).expected("dm.csv");
    // The saved dataset a submission reads, as a run would have published it.
    let case = Case::new("sdtm-dm-metadata", &[("/dm.csv", &dm)]);
    let token = CancellationToken::new();
    let mut submission = case
        .engine()
        .project_submission("project.yaml", Some("sdtm"), Some(CREATED), "0.4.0", &token)
        .unwrap();
    assert_eq!(submission.issues().rows(), 0);
    assert_eq!(
        case.accesses(),
        set(&[
            &PROJECT[..],
            &SPEC,
            &[
                ("capture", "/dm.csv"),
                ("target", "/define.xml"),
                ("target", "/dm.json"),
            ],
        ]
        .concat())
    );
    submission.save().unwrap();
    assert_eq!(
        case.accesses(),
        set(&[
            ("target", "/dm.json"),
            ("stage", "/dm.json"),
            ("commit", "/dm.json"),
            ("target", "/define.xml"),
            ("stage", "/define.xml"),
            ("commit", "/define.xml"),
        ])
    );
    assert_eq!(case.published("/define.xml"), case.expected("define.xml"));
    assert_eq!(case.published("/dm.json"), case.expected("dm.json"));
}

#[test]
fn resolve_reads_only_the_specification_through_the_resolver() {
    let case = Case::new("sdtm-dm-metadata", &[]);
    let resolved = case.engine().spec_resolve("spec.yaml", None).unwrap();
    assert_eq!(case.accesses(), set(&SPEC));
    let yamaa::ContextValue::Map(fields) = resolved else {
        panic!("resolved facts are an object");
    };
    assert_eq!(
        fields.get("entry"),
        Some(&yamaa::ContextValue::Text("/spec.yaml".into()))
    );
}

#[test]
fn read_odm_captures_its_source_once_and_its_table_saves_through_the_resolver() {
    const ODM: &[u8] = br#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3">
  <ClinicalData StudyOID="S" MetaDataVersionOID="M">
    <SubjectData SubjectKey="01"><StudyEventData StudyEventOID="E">
      <FormData FormOID="F"><ItemGroupData ItemGroupOID="G">
        <ItemData ItemOID="AGE" Value="42"/>
      </ItemGroupData></FormData>
    </StudyEventData></SubjectData>
  </ClinicalData>
</ODM>"#;
    let case = Case::new("sdtm-dm-metadata", &[("/odm/items.xml", ODM)]);
    let result = case.engine().read_odm("odm/items.xml", &OdmOptions::new());
    // One request each: a source read twice would log twice.
    let once = |location: &str| vec![("capture", location.to_owned())];
    if cfg!(feature = "odm") {
        let table = result.unwrap();
        assert_eq!(table.rows(), 1);
        assert_eq!(case.requests(), once("/odm/items.xml"));
        // The table saves through the same resolver, as a run's output does.
        assert_eq!(
            case.engine().save_table(&table, "odm/items.csv").unwrap(),
            ["memory:/odm/items.csv"]
        );
        assert_eq!(
            case.accesses(),
            set(&[
                ("target", "/odm/items.csv"),
                ("stage", "/odm/items.csv"),
                ("commit", "/odm/items.csv"),
            ])
        );
        assert_eq!(
            String::from_utf8(case.published("/odm/items.csv")).unwrap(),
            format!(
                "{}\n1.3,,S,M,01,,,E,,,,,,F,,,,,,,G,,,,AGE,,42,true,false,1\n",
                yamaa::ODM_COLUMNS.join(",")
            )
        );
        let Err(yamaa::Error::Rejected(report)) =
            case.engine().read_odm("odm/absent.xml", &OdmOptions::new())
        else {
            panic!("a source the resolver lacks is refused with its issue");
        };
        // The caller's own path, named as given (FILE-7).
        let [issue] = report.diagnostics.as_slice() else {
            panic!("one issue: {report:?}");
        };
        assert_eq!(issue.code, "resource_path_missing");
        assert_eq!(
            issue.context.get("path"),
            Some(&yamaa::ContextValue::Text("odm/absent.xml".into()))
        );
        assert_eq!(case.requests(), once("/odm/absent.xml"));
    } else {
        assert!(matches!(
            result,
            Err(yamaa::Error::Unavailable { feature: "odm" })
        ));
        assert_eq!(case.requests(), Vec::new());
    }
}
