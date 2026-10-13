"""The retained IO helpers must not import any reference semantic family."""

import subprocess
import sys
from pathlib import Path


def test_public_helpers_work_without_reference_imports(odm13_path, tmp_path):
    package = Path(__file__).resolve().parents[2] / "src"
    program = """
import importlib.abc
import sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
class RefuseReference(importlib.abc.MetaPathFinder):
    def find_spec(self, fullname, path=None, target=None):
        forbidden = ('yamaa._reference_domain', 'yamaa.application', 'yamaa.adapters',
            'yamaa.expressions', 'yamaa.io', 'yamaa.models', 'yamaa.planning',
            'yamaa.runtime', 'yamaa.schema', 'yamaa.specification', 'yamaa.submission',
            'yamaa.verification', 'yamaa.odm.bindings', 'yamaa.odm.context', 'yamaa.odm.items')
        if any(fullname == name or fullname.startswith(name + '.') for name in forbidden):
            raise AssertionError('reference import reached: ' + fullname)
sys.meta_path.insert(0, RefuseReference())
from yamaa import read_odm, write_odm_parquet
frame = read_odm(sys.argv[2])
assert frame['SourceOrdinal'].to_list() == [1, 2, 3]
assert frame['Value'].to_list() == ['2026-01-02', '', None]
target = Path(sys.argv[3])
written = write_odm_parquet(sys.argv[2], target, batch_size=1)
assert written.row_count == 3 and written.output_path == target
assert target.is_file()
"""
    subprocess.run(
        [
            sys.executable,
            "-I",
            "-c",
            program,
            str(package),
            str(odm13_path),
            str(tmp_path / "retained.parquet"),
        ],
        check=True,
    )
