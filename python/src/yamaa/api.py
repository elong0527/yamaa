"""Public entry points: derive a spec into its output CSV text, or into
every file the run publishes."""

from . import logs as _logs
from .csv_io import write_parquet_bytes
from .engine import Engine
from .errors import YamaaError


def derive(spec_path, project_root=None):
    """Run the derivation for the spec at spec_path.

    project_root: directory containing environment.yaml for specs that
    call project functions (Stage 2). Returns the rendered output CSV
    text. Raises YamaaError on any validation, derivation, or output
    failure.
    """
    return Engine(spec_path, project_root=project_root).run()


def derive_artifacts(spec_path, project_root=None):
    """Run the derivation and return every file it publishes.

    Returns {declared path: file} in the order a publisher replaces them
    (REQ-1181): `output.verification_log`, then `output.warning_log`, then
    the primary artifact at `output.path`. Each path's extension selects its
    profile: a csv file is its text, a parquet file its bytes. Raises
    YamaaError as derive does; a failed run that reached execution
    publishes its declared verification log alone (REQ-1177), which the
    error carries as {path: file} in `artifacts`.
    """
    engine = Engine(spec_path, project_root=project_root)
    try:
        text = engine.run()
    except YamaaError as err:
        err.artifacts = _logs.sidecars(engine, succeeded=False)
        raise
    path = engine.output["path"]
    if path.lower().endswith(".parquet"):
        cols, rows, types = engine._artifact
        return {**engine.sidecars, path: write_parquet_bytes(cols, rows, types)}
    return {**engine.sidecars, path: text}
