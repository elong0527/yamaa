# Verify a completed table and publish its artifact

`yamaa.verification` asserts over a completed table; `yamaa.artifacts` turns
one into the bytes R020 fixes and replaces a permitted target with them.

```python
from yamaa.artifacts import ArtifactTarget, build_artifact, publish_artifact
from yamaa.verification import check_column, check_dataset, check_keys

failures = [
    failure
    for column in specification.columns
    for failure in check_column(table, column, specification.keys)
]
failures = failures or list(check_keys(table, specification.keys))
failures = failures or list(
    check_dataset(table, specification.verifications or [], specification.keys)
)
if failures:
    raise RuntimeError(failures)

artifact = build_artifact(table, specification.output, specification.keys)
publish_artifact(ArtifactTarget(run_directory / "adsl.csv"), artifact)
```

`verify_completed_table(table, columns, keys, verifications)` runs the three
stages in R005 order and raises `VerificationError` at the first that fails;
the three checks above are the same work with the failures in hand.

## What each step owns

- **`check_column`** runs one column's verifications over its completed
  values, which is R005 stage 5 rather than a final sweep, so an executor
  calls it as each column finishes its lifecycle. **`check_keys`** validates
  output identity once every column is complete, and **`check_dataset`** runs
  last. Each returns the failures it found, in the committed error shape:
  phase, condition, stable specification path, requirement, failure count,
  and representative offending keys.
- A declaration R009 requires the validation phase to reject -- a reversed
  `range`, a repeated verification id, a grouped `row_count` with no id, a
  predicate that cannot be evaluated -- raises `DeclarationError` instead. A
  verification whose own declaration is invalid asserts nothing, so it is
  refused rather than reported as a data failure.
- **`build_artifact`** applies R005's column selection and row order and
  refuses an output declaration or a stored value R020 cannot write. The
  artifact it returns is writable by construction.
- **`render_artifact`** produces the bytes: `.csv` exactly, down to quoting,
  the distinction between a missing value and a collected empty string, and
  the one display rounding `output.decimals` asks for; `.parquet` under the
  R020-20 mapping, uncompressed, with no key-value metadata of its own.
- **`publish_artifact`** writes those bytes into a temporary file beside the
  target, flushes them to the filesystem, and replaces the target in one
  step. A failure leaves the previous artifact untouched and removes the
  temporary file.

`ArtifactTarget` is the caller's explicit permission to write one file.
R021 reaches only the files a run reads and R020 owns the artifact's bytes
rather than where a run may put them, so no target is admitted on a
specification's word alone.

## Focused tests

```bash
uv run --project python --no-sync pytest python/tests/verification python/tests/artifacts
```
