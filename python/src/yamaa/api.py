"""Public entry point: derive a spec into its output CSV text."""

from .engine import Engine


def derive(spec_path, project_root=None):
    """Run the derivation for the spec at spec_path.

    project_root: directory containing environment.yaml for specs that
    call project functions (Stage 2). Returns the rendered output CSV
    text. Raises YamaaError on any validation, derivation, or output
    failure.
    """
    return Engine(spec_path, project_root=project_root).run()
