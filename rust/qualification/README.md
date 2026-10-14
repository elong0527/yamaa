# Native public qualification gates

The current gates qualify the installed public `domain`/`check`/`define` routes
through the shared Rust engine. `public-conformance.json` requires 316 unchanged
benchmark cases in Python and R. Its retained lane inventory names all eighteen
Python platform/interpreter/package-form combinations and all three R source
hosts, including Windows R. `audit_native_run.py` independently requires every
route, complete public and producer comparisons, strict R checks, archive bytes
and toolchain evidence. A missing Windows R tuple cannot satisfy these gates.

The historical Python-assisted `required.json`, `known-gaps.json` and
`missing-routes.json` were retired with their reference-assisted consumers in
commit `384cb9111c2367aaf12298f29afbec3fc05831e2`, merged through #2110/#2113.
The missing R-entrypoint row described that retired route. Its removal records
route retirement; it does not relabel an unexecuted assisted tuple as a public
pass. The exact earlier files and findings remain in Git history. Current R and
Windows R obligations belong to the installed public gates above.

`supplemental.json` separately reconciles component/compile contract files and
installed Python/R probe suites. Its runner retains actual suite logs and results;
these do not enter the complete-run benchmark count. See the
[supplemental evidence contract](../SUPPLEMENTAL.md).

The public M1 gate in `tools/qualify_public_cohort.py` requires twelve
cases from actual installed Python/R public domain runs. Both installed suites
already compare their complete reports with independent truth, including source
reads, source tables, ordered diagnostics and refusal to save a failed build.
The public comparator now requires those reports rather than leaving them outside
its selected cohort. Missing or mismatched reports fail the gate. Windows R
remains required; passes apply only to the recorded source revision and package.
The [release-evidence ledger](../planning/1996-qualification.md) records the
current combined-main limitation and the branch qualification procedure.


## Complete public report comparisons

The retained M1 cohort compares actual installed public owners with unchanged
committed complete-report truth. `qualify_public_cohort.py` covers all twelve
original cases, including source and derived tables, exact scalar observations,
ordered nodes/diagnostics/verification/callback/resource ledgers and artifacts.
`qualify_producer_cohort.py` retains each actual producer owner before Save and
after two Saves, its source/artifact provenance, exact publication prefix and
saved CSV/Parquet bytes. It compares complete observations with
`producer-cohort/shared-producer-parquet/reference.json`; CSV bytes are exact
and Parquet content is compared logically with an independent reader.

The tools use a private report/comparison module under `rust/tools`. They import
no yamaa package, reference evaluator or assisted qualification inventory.
`complete-public-cohort/2` records two actual installed `shared_run` routes per
case, Python and R. Each row separately identifies
`committed-authored-report-truth` by its exact Git revision/path and retains
the original truth bytes. Stored truth is never presented as a fresh
`reference_run` or a newly installed package. The audit checks all case/host
tuples, actual owner metadata, exact-source truth bytes, complete reports and
producer held Save bytes again. Older v1 audit artifacts remain readable as
historical evidence.

Mutation controls in `test_committed_cohort.py` and `test_producer_cohort.py`
reject missing fields, changed values/types/exact float bits, reordered or
missing observations/logs/callbacks, wrong truth/source/path tuples, incomplete
producer owners and corrupted saved artifacts. The authored fixture reports and
benchmark expected outputs are unchanged. Local replay of prior evidence tests
the comparator; renewed hosted qualification remains necessary for a new head.

## Corpus-wide public conformance

`rust/tools/public_conformance.py` plans all benchmark entries from literal
`run.py` and `run.R` calls. Cases without runners use their single specification
and a host-specific environment when present. A root environment is never
passed implicitly. The Python and R runners call only the installed public
`domain` and `yamaa_domain` APIs. Python runs with `-I` and refuses reference
semantic imports. Both copy execution inputs into a fresh output directory,
exclude `expected/`, and publish there with `save()`. Python also stages the
canonical `python/uv.lock`, matching the existing benchmark test harness.

The standalone comparator reads the completed records and independent
`expected/` files. It checks public column types, row order, full int64 text,
nulls, UTF-8 bytes (including embedded NUL), binary64 bits and temporal host
values. Saved CSV bytes compare exactly. Parquet compares logical fields,
types, nulls, row order and values; metadata and compression are immaterial.
For a declared `output.decimals`, public floats compare at the exact reported
precision in the committed CSV under REQ-0747, with no tolerance. The inventory
explicitly records that the CSV does not supply unrounded float truth.
Negative contracts compare the first Rust-projected issue's phase, condition,
requirement and ordered specification paths, with the declared context subset.
Unsupported operations and infrastructure errors never count as negative passes.

Each inventory records one of `pass`, `mismatch`, `unsupported`, `error` or
`excluded` for every benchmark and counts unsupported operations. The two
historical path negatives are excluded under #1905. Four `spec_resolved.yaml`
artifacts are reference-only until cutover (#1973 item 14); Define XML/JSON are
explicitly `not_compared` by these domain runners. The fixed-time public Define
suites remain separate. These dispositions do not suppress table differences.

Each sampled PR lane and every main or full-dispatch lane uploads its inventory,
individual records and staged saved artifacts. The committed
`public-conformance.json` gates both installed hosts. Its 316 Python and 316 R
cases are the independently replayed intersection of all eighteen Python lanes
and three R hosts in full-dispatch run 38416920074 at
`2ba10418607933f92597f7c00ac363a3113a49db`. Every active case passes on that
immutable candidate: zero mismatches, unsupported operations or execution
errors, and the two approved historical path exclusions remain visible. The
Windows R symlink negative is included in R's required passing set. This corpus
evidence does not complete other migration or release gates or qualify later
main revisions. Run locally against an installed package outside the checkout:

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

Use absolute paths for the scripts, fixtures and lock when running from outside
the checkout. The R comparator host needs `py-yaml12==0.2.0` and
`pyarrow==25.0.1`; the R runner needs only base R and the installed package.

Seed or advance `public-conformance.json` only from the intersection of passes
in fresh inventories for every approved lane (18 Python lanes and three R
hosts), using separate `python` and `r` lists. Passing `--ratchet FILE` to the
comparator makes any listed benchmark's non-pass fail CI and reports new passes.
Never seed from a cached local run or edit expected truth to add a pass. A
language-family WP adds a benchmark only after its installed report passes on
every lane of both hosts, retaining the full-run evidence in its PR. Deliberately
breaking an output must demonstrate the regression gate, then be reverted in
that seeding PR. The assisted gates above stay in place until their separate
retirement decision.

`tools/seed_public_conformance.py` enforces that seeding contract. Download all
`public-conformance-*` artifacts from one successful completed full main or dispatched
`native.yml` run into a directory, with one subdirectory per artifact. Retain
the run's REST metadata separately, then run:

```sh
gh api repos/elong0527/yamaa/actions/runs/RUN_ID > .cache/conformance-run.json
gh run download RUN_ID --repo elong0527/yamaa --pattern 'public-conformance-*' \
  --dir .cache/conformance-artifacts
python rust/tools/seed_public_conformance.py --fixtures benchmarks \
  --artifacts .cache/conformance-artifacts --run .cache/conformance-run.json \
  --output .cache/public-conformance.json
```

The output must be a fresh file. The tool requires all eighteen Python and
three R inventories from that exact revision/run, checks each host's platform,
runtime and package identity, requires a successful run conclusion, and replays
every saved observation against the
independent expected files. A claimed pass whose saved output changed is refused.
Only the intersection of actual passes enters each host's list. To advance an
existing ratchet, pass `--previous rust/qualification/public-conformance.json`;
any reduction of its passing set is refused. Commit the verified output and
enable `--ratchet` in both installed comparator steps after full-run evidence
and the deliberately broken-output control pass. Missing evidence never seeds
a partial matrix.
