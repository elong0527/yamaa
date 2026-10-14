//! Aggregate derive bindings remain local when specifications are inherited.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use yamaa_adapters::{file_application, specification_report::Identity};
use yamaa_engine::domain_entry::Request;
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "yamaa-inherited_aggregate_bindings-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn build(&self, name: &str) -> file_application::Domain {
        let path = self.0.join(name);
        let path = path.to_str().unwrap();
        let identity = || Identity {
            runtime: "test",
            runtime_version: "test",
            engine_version: yamaa_core::VERSION,
            example: "inherited_aggregate_bindings",
            specification: path,
            base_directory: ".",
        };
        file_application::domain(
            Request {
                specification: path,
                environment: None,
            },
            identity(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
use serde_json::json;
#[test]
fn reducer_and_chained_binding_names_do_not_become_global_column_dependencies() {
    for (chained, collision) in [(false, false), (true, false), (false, true)] {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("dm.csv"), "ID\n1\n2\n").unwrap();
        fs::write(
            fixture.0.join("ex.csv"),
            "ID,EXSTDTC\n1,2025-01-05\n1,2025-01-02\n2,2025-02-10\n",
        )
        .unwrap();
        let mut bindings = vec![
            json!({"name":"STDT","type":"date","derivation":{"to_date":{"source":"EX.EXSTDTC"}}}),
        ];
        if chained {
            bindings.push(json!({"name":"DAYNUM","type":"int","derivation":{"to_epoch_day":{"source":"STDT"}}}));
        }
        let mut parent = json!({
            "schema_version":"1.0", "domain":"OUT", "keys":["ID"], "base":"DM",
            "input":{"DM":"dm.csv","EX":"ex.csv"}, "output":{"path":"result.csv","columns":["ID","RESULT"]},
            "columns":[
                {"name":"ID","type":"str","derivation":"DM.ID"},
                {"name":"RESULT","type":if chained {"int"} else {"date"},"derivation":{"aggregate":{
                    "expr":if chained {"MIN(DAYNUM)"} else {"MIN(STDT)"}, "derive":bindings
                }}}
            ]
        });
        if collision {
            parent["columns"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name":"STDT","type":"date","derivation":"RESULT"}));
            parent["output"]["columns"]
                .as_array_mut()
                .unwrap()
                .push(json!("STDT"));
        }
        fs::write(fixture.0.join("parent.yaml"), parent.to_string()).unwrap();
        fs::write(
            fixture.0.join("child.yaml"),
            "schema_version: '1.0'\nparents: parent.yaml\n",
        )
        .unwrap();
        for entry in ["parent.yaml", "child.yaml"] {
            let mut result = fixture.build(entry);
            assert!(
                result.issues().is_empty(),
                "{chained} {collision} {entry}: {:?}",
                result.issues()
            );
            assert!(result.save().unwrap());
            let expected = if chained {
                "ID,RESULT\n1,20090\n2,20129\n"
            } else if collision {
                "ID,RESULT,STDT\n1,2025-01-02,2025-01-02\n2,2025-02-10,2025-02-10\n"
            } else {
                "ID,RESULT\n1,2025-01-02\n2,2025-02-10\n"
            };
            assert_eq!(
                fs::read_to_string(fixture.0.join("result.csv")).unwrap(),
                expected
            );
        }
    }
}
