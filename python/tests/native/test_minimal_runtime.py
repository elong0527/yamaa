"""The installed public API works without the test extra's readers."""

import subprocess
import sys
import textwrap


def test_runtime_without_optional_python_readers(tmp_path):
    script = textwrap.dedent(
        '''
        import importlib.abc
        import sys
        from importlib.metadata import requires
        from pathlib import Path

        from packaging.requirements import Requirement

        blocked = {"yaml", "yaml12", "pyarrow", "lxml", "pydantic"}

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

        dependencies = [Requirement(value) for value in requires("yamaa")]
        assert {
            value.name for value in dependencies
            if value.marker is None or value.marker.evaluate({"extra": ""})
        } == {"polars", "packaging"}

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
        assert spec.check().issues.is_empty()
        run = spec.run()
        assert run.issues.is_empty()
        expected = [
            {"ID": "a", "VALUE": -9223372036854775808},
            {"ID": "b", "VALUE": 9223372036854775807},
            {"ID": "c", "VALUE": None},
        ]
        assert run.output.to_dicts() == expected
        run.save()
        assert (root / "output.parquet").is_file()

        path = root / "roundtrip.yaml"
        path.write_text(
            specification.replace("SRC: input.csv", "SRC: output.parquet")
            .replace("path: output.parquet", "path: roundtrip.csv")
        )
        roundtrip = yamaa.spec(path).run()
        assert roundtrip.issues.is_empty()
        assert roundtrip.output.to_dicts() == expected
        roundtrip.save()
        assert (root / "roundtrip.csv").read_text() == source
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
