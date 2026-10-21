"""The installed public API works without the test extra's packages: the
package has no runtime dependency, and its tables convert in Rust."""

import subprocess
import sys
import textwrap


def test_runtime_without_optional_python_readers(tmp_path):
    script = textwrap.dedent(
        '''
        import importlib.abc
        import json
        import re
        import sys
        from importlib.metadata import requires, version
        from pathlib import Path

        blocked = {
            "yaml",
            "yaml12",
            "polars",
            "pyarrow",
            "pandas",
            "lxml",
            "pydantic",
            "packaging",
        }

        class OptionalReaders:
            def find_spec(self, fullname, path=None, target=None):
                if fullname.split(".")[0] in blocked:
                    raise ModuleNotFoundError(
                        "optional dependency is unavailable: " + fullname,
                        name=fullname,
                    )
                return None

        sys.meta_path.insert(0, OptionalReaders())

        import yamaa
        from yamaa import _locked_functions

        # A runtime requirement is one that no extra selects: none remains.
        assert {
            re.match(r"[A-Za-z0-9._-]+", value).group().lower()
            for value in requires("yamaa")
            if "extra" not in value.partition(";")[2]
        } == set()

        root = Path(sys.argv[1])
        source = "ID,VALUE\\na,-9223372036854775808\\nb,9223372036854775807\\nc,\\n"
        (root / "input.csv").write_text(source)
        specification = """schema_version: '1.0'
        domain: TEST
        keys: [ID]
        input: {SRC: input.csv}
        base: SRC
        columns:
          - {name: ID, type: str, derivation: SRC.ID}
          - {name: VALUE, type: int, derivation: SRC.VALUE}
        output: {path: output.parquet, columns: [ID, VALUE]}
        """
        path = root / "spec.yaml"
        path.write_text(specification)
        spec = yamaa.spec(path)
        assert spec.check().issues.num_rows == 0
        run = spec.run()
        assert run.issues.num_rows == 0
        assert isinstance(run.output, yamaa.Table)
        expected = [
            {"ID": "a", "VALUE": -9223372036854775808},
            {"ID": "b", "VALUE": 9223372036854775807},
            {"ID": "c", "VALUE": None},
        ]
        assert run.output.to_pylist() == expected
        run.save()
        assert (root / "output.parquet").is_file()

        path = root / "roundtrip.yaml"
        path.write_text(
            specification.replace("SRC: input.csv", "SRC: output.parquet")
            .replace("path: output.parquet", "path: roundtrip.csv")
        )
        roundtrip = yamaa.spec(path).run()
        assert roundtrip.issues.num_rows == 0
        assert roundtrip.output.to_pylist() == expected
        roundtrip.save()
        assert (root / "roundtrip.csv").read_text() == source

        # The engine verifies a function's uv lock: the host imports no
        # packaging library to report the installed versions.
        (root / "dedent.yaml").write_text(
            'yamaa: "1.0"\\ntype: function\\nname: dedent\\n'
            "description: Text without its common leading white space.\\n"
            "params: [{name: text, type: str}]\\nreturns: str\\n"
            "implementations: {python: textwrap.dedent}\\ntests:\\n"
            "  - {id: normal, covers: [normal, boundary], args: {text: x}, result: x}\\n"
            "  - {id: missing, covers: ['short-circuit-missing:text'],"
            " args: {text: null}, result: null}\\n"
        )
        (root / "uv.lock").write_text(
            'version = 1\\n[[package]]\\nname = "yamaa"\\n'
            f'version = "{version("yamaa")}"\\n'
            f"resolution-markers = [\\"sys_platform == '{sys.platform}'\\"]\\n"
        )
        (root / "project.yaml").write_text(
            'yamaa: "1.0"\\ntype: project\\nfunctions: [dedent.yaml]\\n'
            "environments:\\n  python: {lock: uv.lock}\\n"
            "adam:\\n  specs: [called.yaml]\\n"
        )
        (root / "called.yaml").write_text(
            specification.replace("derivation: SRC.ID}", "derivation: SRC.ID}\\n"
            "  - {name: TEXT, type: str, derivation: "
            "{function: {name: dedent, args: {text: SRC.ID}}}}", 1)
            .replace("columns: [ID, VALUE]", "columns: [ID, TEXT]")
            .replace("path: output.parquet", "path: called.csv")
        )
        called = yamaa.project(root / "project.yaml").spec("TEST").run()
        assert called.issues.num_rows == 0, called.issues
        assert [row["TEXT"] for row in called.output.to_pylist()] == ["a", "b", "c"]
        activation = json.loads(called._native.observations())["activation"]
        assert activation["lock"] == "verified"
        assert not any(name.split(".")[0] in blocked for name in sys.modules)
        '''
    )
    result = subprocess.run(
        [sys.executable, "-I", "-c", script, str(tmp_path)],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, result.stdout + result.stderr
