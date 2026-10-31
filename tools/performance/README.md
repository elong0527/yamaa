# Performance comparison

A scaled comparison of yamaa against polars and dplyr, run by hand and never
in CI. It measures; it asserts nothing about speed. Post its output on the
issue it is evidence for, never in a committed file.

## Run it

Build the command line with the release profile, then run the comparison
from the repository root:

```sh
(cd rust && cargo build --locked --release -p yamaa-cli)
uv run --project python --no-sync python -m tools.performance
```

That runs the bds and copy cases at 18k, 180k, 1.8M and 7.2M LB rows. The
function case runs at 1M rows only with `--python`, an interpreter with a
release build of yamaa installed; `make setup` installs a dev build, so make
one apart, for example:

```sh
uv venv build/release-venv --python 3.12
uv pip install --python build/release-venv/bin/python ./python
uv run --project python --no-sync python -m tools.performance \
  --python build/release-venv/bin/python
```

To measure one case at one size against another build of the command line,
yamaa alone:

```sh
uv run --project python --no-sync python -m tools.performance \
  --case copy --size 1.8M --engine yamaa --cli /path/to/rust/target/release/yamaa
```

`--help` lists every option. `--case`, `--size` and `--engine` repeat;
yamaa always runs, as the reference the other engines are checked against.
Inputs and outputs go under `build/performance/` (`--work`); the generated
inputs are kept there and reused, since a size always generates the same
tables.

## What it measures

Each run is one process; its time includes interpreter start-up.

- **Wall seconds**: the median of 3 runs, or of 1 at 7.2M rows and over
  (`--repeat`).
- **Peak RSS**: the process's own peak resident memory, from `wait4`.
  `launch.py` starts each run from a small process, because Linux carries a
  process's peak across `exec`, and the runner's own peak grows with the
  outputs it compares.
- **Output rows and file size**, and yamaa's phase table from `--verbose`
  (the median run's).

Before an engine's times count, its first output is read back and checked
equal to yamaa's: the same columns, and, sorted by the case's keys, the same
values, every float bit for bit. An engine that differs, fails or cannot run
shows its reason in place of its numbers.

## Cases

The inputs come from `generate.py`: an ADSL and LB pair at a subject count
(6 lab tests x 12 visits per subject, about 4% of results missing), and a
vitals table for the function case. A size counts input rows; the LB cases
generate one subject per 72 rows, so 18k, 180k, 1.8M and 7.2M are 250,
2,500, 25,000 and 100,000 subjects.

| Case | yamaa | Compared with |
|---|---|---|
| `bds` | `cases/bds/spec.yaml`, `benchmarks/adam-adlb-bds` with Parquet inputs and output and no verifications | `polars_cases.py bds` on all cores (`polars`, or `--polars-threads`) and on one (`polars-1`), and `dplyr_bds.R` when R has dplyr and arrow |
| `copy` | `cases/copy/spec.yaml`, LB copied from Parquet to Parquet on its keys | `polars_cases.py copy` |
| `function` | `cases/function/expression.yaml`, BMI as an expression, through `host_run.py` | `function.yaml`: the same BMI from a Python project function (`yamaa-function`) |

The polars and dplyr programs write the derivation the way a user of each
would. A change to a case's spec changes them with it.
