"""Mandatory real-R execution of the repository validator's exact issue transport."""

import importlib.util
import json
import shutil
from pathlib import Path
from tempfile import TemporaryDirectory

loader = importlib.util.spec_from_file_location(
    "r_check_transport", Path(__file__).with_name("_r_check_transport.py")
)
transport = importlib.util.module_from_spec(loader)
loader.loader.exec_module(transport)
metadata = transport.installation(required=True)
assert metadata["version"] == "0.3.0", metadata
assert metadata["core"] == "0.1.0", metadata
assert metadata["function_package_version"] == "1.0.0", metadata
package = Path(metadata["package_path"]).resolve()
assert package.is_dir() and (package / "DESCRIPTION").is_file()
fixture_root = Path(metadata["fixture_path"])
positive = (
    "schema-functions",
    "schema-non-finite",
    "adam-adsl-bmi",
    "adam-advs-percentiles",
)
with TemporaryDirectory(prefix="yamaa-real-r-validator-") as temporary:
    work = Path(temporary)
    assert not work.resolve().is_relative_to(package)
    for name in (*positive, "negative-function-contract"):
        case = work / name
        shutil.copytree(fixture_root / name, case)
        environment = case / "r/environment.yaml"
        lock = case / "r/renv.lock"
        assert lock.is_file(), f"mandatory real declared lock absent: {lock}"
        locked = json.loads(lock.read_text(encoding="utf-8"))
        assert locked["Packages"]["yamaabenchmarks"]["Version"] == "1.0.0"
        # The exact public Check program receives original declarations, with no
        # study source or callable invocation/publication authority.
        shutil.rmtree(case / "input")
        specification = case / "spécification.yaml"
        (case / "spec.yaml").rename(specification)
        rows = transport.rows(specification, environment, required=True)
        if name in positive:
            assert rows == [], (name, rows)
        else:
            assert len(rows) == 1, rows
            phase, condition, requirement, paths, context = rows[0]
            assert (phase, condition, requirement, paths) == (
                "validation",
                "invalid_function_argument",
                "REQ-0700",
                ["columns.RESULT.derivation.function.args"],
            )
            assert json.loads(context) == {
                "argument": "x",
                "cause": "missing_required_argument",
                "function": "project_value",
            }
        assert not list(case.glob("*.csv")), "Check published a study artifact"
        print(name + " repository validator real-R issue transport passed")

    # UTF-8 context and a single path array must survive the exact R program.
    unicode_specification = work / "unicode.yaml"
    unicode_specification.write_text(
        json.dumps(
            {
                "schema_version": "1.0",
                "domain": "UTF8",
                "keys": ["ID"],
                "input": {"SOURCE": {"path": "absent.csv"}},
                "columns": [
                    {
                        "name": "ID",
                        "type": "str",
                        "label": "ID",
                        "derivation": "SOURCE.ID",
                    },
                    {
                        "name": "TEXT",
                        "type": "str",
                        "label": "Text",
                        "derivation": {"literal": "x"},
                        "verifications": [{"matches": {"pattern": "é("}}],
                    },
                ],
                "output": {"path": "never.csv", "columns": ["ID", "TEXT"]},
            }
        ),
        encoding="ascii",
    )
    [(phase, condition, requirement, paths, context)] = transport.rows(
        unicode_specification,
        work / "schema-functions/r/environment.yaml",
        required=True,
    )
    assert (phase, condition, requirement, paths) == (
        "validation",
        "invalid_regex",
        "REQ-0827",
        ["columns.TEXT.verifications[0].matches.pattern"],
    )
    assert json.loads(context) == {"pattern": "é("}
    assert not (work / "never.csv").exists()
    print("repository validator real-R UTF-8 context and array paths passed")
