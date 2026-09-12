# Verify and publish a completed table

```python
from yamaa.artifacts import publish_artifact, write_artifact
from yamaa.verification import verify_columns, verify_dataset

verify_columns(table, spec.columns, spec.keys)
verify_dataset(table, spec.keys, spec.verifications or [])
publish_artifact(spec.output.path, write_artifact(table, spec.output, spec.keys))
```

- `verify_columns()` runs each column's checks after its lifecycle (R005
  stage 5); `verify_dataset()` validates keys then dataset checks (R009
  runs last). Both return the table and raise `VerificationError` with
  stable phase, condition, spec path, and offending keys.
- `write_artifact()` selects the profile from `output.path` (`.csv` byte
  exact, `.parquet` read-back identical) and renders `output.columns` in
  order. Missing is always bare in `csv` and null in `parquet`.
- `publish_artifact()` writes complete bytes through a same-directory
  temporary file and atomically replaces the target; failures preserve
  the previous artifact and remove the residue.

Focused tests for this component:

```sh
export PATH="$HOME/.local/bin:$PATH"
uv run --project python --no-sync pytest python/tests/verification python/tests/artifacts -q
```
