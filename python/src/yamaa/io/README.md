# Verify and publish a completed table

```python
from yamaa.io import publish_artifact, write_artifact
from yamaa.verification import finalize_output, verify_columns

verify_columns(table, spec.columns, spec.keys)
ordered = finalize_output(table, spec.output, spec.keys, spec.verifications or [])
publish_artifact(
    permitted_target,
    write_artifact(ordered, spec.output, spec.keys),
)
```

- `verify_columns()` is the stage-5 hook for completed columns.
  `finalize_output()` validates output/key membership, checks keys, runs
  dataset verifications, and applies stable presentation ordering. Both
  raise `VerificationError` for data failures with stable phase, condition,
  spec path, and offending keys.
- `write_artifact()` selects the profile from `output.path` (`.csv` byte
  exact, `.parquet` read-back identical) and renders `output.columns` in
  order. Missing is always bare in `csv` and null in `parquet`.
- `publish_artifact()` accepts a caller-permitted `pathlib.Path`, writes
  complete bytes through a same-directory temporary file, and atomically
  replaces it; failures preserve the previous artifact and remove residue.

Focused tests for this component:

```sh
uv run --project python --no-sync pytest python/tests/verification \
  python/tests/io/test_csv_write.py python/tests/io/test_parquet_write.py \
  python/tests/io/test_publish.py python/tests/models/test_values.py -q
```
