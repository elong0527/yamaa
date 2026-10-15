# Installed package checks

The Python and R suites exercise installed `domain`, `check` and `define` APIs.
Benchmark directories and `tools.conformance.public_conformance` define the current active
set and explicit exclusions for both hosts. No passing observations are retained:
each native lane gates directly on every active benchmark passing.
`supplemental.json` records only underivable catalog data (contract fixtures and
their consumers, elevated suite levels, renamed suite identities); suites and
plain fixtures are discovered by glob.

## Public reports

`tools.conformance.qualify_public_cohort` compares complete domain reports with authored
truth. `tools.conformance.qualify_producer_cohort` compares retained producer results before
and after repeated Save calls with `producer-cohort/` fixtures. Comparisons cover
values, types, exact float bits, ordering, diagnostics, verification, callbacks,
resource observations and saved artifacts. CSV bytes compare exactly; Parquet
compares logical fields, types, nulls, rows and values.

## Benchmark conformance

`tools.conformance.public_conformance` discovers benchmark specifications and explicit
runner arguments. Its shared plan accounts for every authored case, with two
reviewed exclusions. Runnable entries describe host metadata;
qualification requires each installed host's actual passing inventory.
The installed Python and R runners stage execution inputs
without `expected/`, build domains and save outputs. The comparator reads
independent expected files and rejects changed values, types, order, diagnostics
or saved bytes. Per-case exclusions and artifact comparison scope appear in
the resulting inventory.

After `make setup`, run from the repository root against the installed package:

```sh
python -I -m tools.conformance.public_conformance plan --fixtures benchmarks --output /tmp/public-plan
python -I python/tests/installed/public_conformance.py --plan /tmp/public-plan --fixtures benchmarks \
  --python-lock python/uv.lock --output /tmp/public-python --source-revision REVISION
Rscript --vanilla rust/tests/public_conformance.R /tmp/public-plan benchmarks /tmp/public-r REVISION
python -I -m tools.conformance.public_conformance compare --fixtures benchmarks \
  --observed /tmp/public-python --output /tmp/public-python/inventory.json
python -I -m tools.conformance.public_conformance compare --fixtures benchmarks \
  --observed /tmp/public-r --output /tmp/public-r/inventory.json
```

Use absolute runner, fixture and lock paths when running outside the checkout.
The importable comparison tools are installed by `make setup`.
The comparator needs the pinned YAML and Arrow dependencies in `python/uv.lock`;
the R runner needs base R and the installed package.

The plan also generates two metamorphic variants of each entry, built by the
same runners on staged copies and judged by the comparator against the entry's
own build rather than against `expected/`: an empty child
(`{schema_version: "1.0", parents: <entry>}`, REQ-0627) must reproduce its
parent, and for eligible positive entries (no `rows`, no `parents`, at least
two key columns reading one input field or a literal) a JSON control copy and
a copy with the key columns moved to the front in reverse key order must agree
(REQ-0060, REQ-0062, REQ-0220). Positive variants must report no issues with
equal output and log tables; negative variants must report the same first
issue (phase, condition, requirement and spec paths). Known differences are
declared in the case's `benchmark.yaml` under `conformance.metamorphic`,
linked to a tracking issue or marked permanent with a by-design reason. A
temporary difference that stops occurring fails the run as stale; an
unlisted difference fails it as a mismatch. Approved exclusions also belong
to their case's `conformance` metadata.

The comparison commands above gate on the current directories alone: every
active case must pass on that host, including a newly added directory. No
prior observations are consulted, so adding a benchmark needs no registry
edit. Expected data remains unchanged.
