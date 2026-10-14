# Installed package checks

The Python and R suites exercise installed `domain`, `check` and `define` APIs.
`public-conformance.json` declares the required benchmark set for each host.
`supplemental.json` declares component contracts and installed probe suites.
`audit_native_run.py` checks package identity, toolchain observations, complete
reports, saved artifacts and R package checks for every selected platform.

## Public reports

`tools/qualify_public_cohort.py` compares complete domain reports with authored
truth. `tools/qualify_producer_cohort.py` compares retained producer results before
and after repeated Save calls with `producer-cohort/` fixtures. Comparisons cover
values, types, exact float bits, ordering, diagnostics, verification, callbacks,
resource observations and saved artifacts. CSV bytes compare exactly; Parquet
compares logical fields, types, nulls, rows and values.

## Benchmark conformance

`tools/public_conformance.py` discovers benchmark specifications and explicit
runner arguments. The installed Python and R runners stage execution inputs
without `expected/`, build domains and save outputs. The comparator reads
independent expected files and rejects changed values, types, order, diagnostics
or saved bytes. Per-case exclusions and artifact comparison scope appear in
the resulting inventory.

Run from the repository root against an installed package:

```sh
python -I rust/tools/public_conformance.py plan --fixtures benchmarks --output /tmp/public-plan
python -I rust/tests/public_conformance.py --plan /tmp/public-plan --fixtures benchmarks \
  --python-lock python/uv.lock --output /tmp/public-python --source-revision REVISION
Rscript --vanilla rust/tests/public_conformance.R /tmp/public-plan benchmarks /tmp/public-r REVISION
python -I rust/tools/public_conformance.py compare --fixtures benchmarks \
  --observed /tmp/public-python --output /tmp/public-python/inventory.json
python -I rust/tools/public_conformance.py compare --fixtures benchmarks \
  --observed /tmp/public-r --output /tmp/public-r/inventory.json
```

Use absolute script, fixture and lock paths when running outside the checkout.
The comparator needs the pinned YAML and Arrow dependencies in `python/uv.lock`;
the R runner needs base R and the installed package.

`tools/seed_public_conformance.py` derives required sets from successful full
matrix runs. It replays saved observations against expected files, requires
every platform/interpreter/package tuple from the same revision and refuses
reductions when given `--previous`. The comparator's `--ratchet` option enforces
the declared required set. Expected data remains unchanged.
