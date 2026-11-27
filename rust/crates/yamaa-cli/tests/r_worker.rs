//! The standalone run's R worker as the binary talks to it, played by a
//! stand-in worker: a shell script answering the protocol of
//! `src/worker.rs`, run as `--interpreter`. It binds every callable and
//! answers every call with the value it was given, so these tests reach the
//! binary's own side of a call without an R installation: what it refuses
//! before it sends anything. The R package's suites cover the real worker.
//! The stand-in is a POSIX shell script, so the family runs on Unix.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use crate::support::{yamaa_in, Scratch};

/// The stand-in worker: it logs each request beside itself and answers
/// `info` with this engine's version, `bind` with handle 1, `invoke` with
/// its last argument's value, the identity of a one-parameter function, and
/// anything else with a bare `ok`.
fn stand_in(study: &Scratch) -> String {
    study.write(
        "worker.sh",
        format!(
            r#"#!/bin/sh
log="$(dirname "$0")/requests.log"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  case "$line" in
    info) printf 'ok\t%s\n' '{version}' ;;
    bind*) printf 'ok\t1\n' ;;
    invoke*) printf 'ok\t%s\n' "${{line##*=}}" ;;
    *) printf 'ok\n' ;;
  esac
done
"#,
            version = yamaa::VERSION
        ),
    );
    let script = study.join("worker.sh");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    study.spelled("worker.sh")
}

/// An int beyond 2^53 in magnitude has no exact R value (FUNC-9), so the
/// binary refuses the call before it sends anything, with the failure the
/// R package's own port raises, which R's function_values.R checks there:
/// `function_call_failed` under FUNC-8 with the registry's message, located
/// at the `function` key of column Y (line 9, column 39 of the spec), whose
/// context names the function, its callable, its authored source and entry
/// (FUNC-10: the spec) and the row's keys, and the reason of the facade's
/// `HostLanguage::unheld`, which the facade classifies as a rejection's
/// reason. The other row is still called.
#[test]
fn an_int_beyond_2_53_fails_the_r_call_before_the_worker_receives_it() {
    let study = Scratch::new("r-worker");
    let worker = stand_in(&study);
    study.write(
        "project.yaml",
        "yamaa: '1.0'\ntype: project\nfunctions: [functions/int_value.yaml]\nadam: {specs: [spec.yaml]}\n",
    );
    study.write(
        "functions/int_value.yaml",
        "yamaa: '1.0'\ntype: function\nname: int_value\n\
         description: Return the int argument unchanged.\n\
         params: [{name: x, type: int}]\nreturns: int\n\
         implementations: {r: base::identity}\ntests:\n\
         \x20 - {id: normal, covers: [normal], args: {x: 7}, result: 7}\n\
         \x20 - {id: boundary, covers: [boundary], args: {x: 9007199254740992}, result: 9007199254740992}\n\
         \x20 - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n",
    );
    study.write(
        "spec.yaml",
        "yamaa: '1.0'\ntype: spec\nmodel: ADaM\ndomain: TEST\nkeys: [ID]\n\
         input: {SRC: {path: input.csv, types: {ID: int, X: int}}}\ncolumns:\n\
         \x20 - {name: ID, type: int, derivation: SRC.ID}\n\
         \x20 - {name: Y, type: int, derivation: {function: {name: int_value, args: {x: SRC.X}}}}\n\
         output: {path: output.csv, columns: [ID, Y]}\n",
    );
    study.write("input.csv", "ID,X\n1,7\n2,9007199254740993\n");
    let output = yamaa_in(
        &study,
        &[
            "--format",
            "json",
            "run",
            &study.spelled("project.yaml"),
            "--host",
            "r",
            "--interpreter",
            &worker,
        ],
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let document: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("{error}: {stdout}"));
    let spec = study.spelled("spec.yaml");
    assert_eq!(
        document["issues"],
        serde_json::json!([{
            "severity": "error",
            "phase": "derivation",
            "condition": "function_call_failed",
            "rule": "FUNC-8",
            "message": "the call to project function base::identity failed",
            "file": spec,
            "line": 9,
            "column": 39,
            "spec_paths": ["columns.Y.derivation.function"],
            "context": {
                "function": "int_value",
                "call": "base::identity",
                "entry": spec,
                "source": spec,
                "keys": [{"ID": 2}],
                "reason": "argument x: integer 9007199254740993 is beyond 2^53 and cannot be held exactly by an R double",
            },
            "source_span": null,
            "operand_route": null,
        }]),
        "the refused call's one issue: {stdout}"
    );
    let requests = std::fs::read_to_string(study.join("requests.log")).unwrap();
    assert!(
        requests.contains("invoke\t1\tx=i:7\n") && !requests.contains("9007199254740993"),
        "the other row is called and no request carries the unheld int: {requests}"
    );
}
