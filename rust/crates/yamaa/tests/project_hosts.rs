//! `ProjectRequest::host_languages`: the languages a run of a project could
//! call its project functions in, read with no study data. The command
//! line settles a standalone run's functions on the one language it names.
#![cfg(feature = "yaml")]

use yamaa::{Engine, EngineOptions, HostLanguage};

struct Study(crate::support::Scratch);

impl Study {
    fn new() -> Self {
        let root = crate::support::Scratch::new("project-hosts");
        std::fs::create_dir_all(root.join("functions")).unwrap();
        std::fs::write(
            root.join("spec.yaml"),
            "yamaa: \"1.0\"\ntype: spec\ndomain: TEST\nkeys: [ID]\ninput: {RAW: raw.csv}\noutput: {path: test.csv, columns: [ID]}\ncolumns:\n  - {name: ID, type: str, label: Identifier, derivation: RAW.ID}\n",
        )
        .unwrap();
        Self(root)
    }

    fn write(&self, name: &str, contents: &str) {
        std::fs::write(self.0.join(name), contents).unwrap();
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_str().unwrap().replace('\\', "/")
    }
}

/// A function file implemented in `languages`.
fn function_file(languages: &str) -> String {
    format!(
        "yamaa: \"1.0\"\ntype: function\nname: shout\ndescription: The value in capitals.\nparams: [{{name: x, type: str}}]\nreturns: str\nimplementations: {{{languages}}}\ntests:\n  - {{id: normal, covers: [normal], args: {{x: a}}, result: A}}\n  - {{id: empty, covers: [boundary], args: {{x: ''}}, result: ''}}\n  - {{id: missing, covers: ['short-circuit-missing:x'], args: {{x: null}}, result: null}}\n"
    )
}

/// The neutral engine reads the project as the command line does.
fn engine() -> Engine {
    Engine::new(EngineOptions::neutral(
        "project-hosts-test".into(),
        "0".into(),
    ))
}

/// The languages follow the `environments` entries when the project
/// declares any, and the function files' implementations otherwise; a
/// project with neither names none.
#[test]
fn the_environments_name_the_languages_and_the_implementations_otherwise() {
    let study = Study::new();
    let cases: [(&str, &str, &str, Vec<HostLanguage>); 5] = [
        (
            "both environments",
            "environments:\n  python: {lock: python/uv.lock}\n  r: {lock: r/renv.lock}\n",
            "python: pkg.shout",
            vec![HostLanguage::Python, HostLanguage::R],
        ),
        (
            "one environment, the other implementation",
            "environments:\n  r: {lock: r/renv.lock}\n",
            "python: pkg.shout",
            vec![HostLanguage::R],
        ),
        (
            "no environment, one implementation",
            "",
            "r: pkg::shout",
            vec![HostLanguage::R],
        ),
        (
            "no environment, both implementations",
            "",
            "python: pkg.shout, r: pkg::shout",
            vec![HostLanguage::Python, HostLanguage::R],
        ),
        ("no functions", "", "", Vec::new()),
    ];
    for (label, environments, implementations, expected) in cases {
        let functions = if implementations.is_empty() {
            String::new()
        } else {
            study.write("functions/shout.yaml", &function_file(implementations));
            "functions: [functions/shout.yaml]\n".to_owned()
        };
        study.write(
            "project.yaml",
            &format!("yamaa: \"1.0\"\ntype: project\n{functions}{environments}adam: {{specs: [spec.yaml]}}\n"),
        );
        let languages = engine()
            .project(&study.path("project.yaml"))
            .host_languages()
            .unwrap_or_else(|failure| panic!("{label}: {failure:?}"));
        assert_eq!(languages, expected, "{label}");
    }
}

/// A project that cannot be read refuses the call as `cannot read`, with
/// the issues a check reports: here a listed function file that does not
/// exist.
#[test]
fn a_project_that_cannot_be_read_refuses_the_call_with_its_issues() {
    let study = Study::new();
    study.write(
        "project.yaml",
        "yamaa: \"1.0\"\ntype: project\nfunctions: [functions/absent.yaml]\nadam: {specs: [spec.yaml]}\n",
    );
    let project = study.path("project.yaml");
    let failure = engine()
        .project(&project)
        .host_languages()
        .expect_err("a missing function file refuses the read");
    assert!(
        failure
            .message
            .starts_with(&format!("cannot read {project}: ")),
        "{}",
        failure.message
    );
    assert_eq!(
        failure
            .diagnostics
            .iter()
            .map(|issue| (issue.code.as_str(), issue.spec_path.clone()))
            .collect::<Vec<_>>(),
        [("resource_path_missing", vec!["functions[0]".to_owned()])]
    );
}
