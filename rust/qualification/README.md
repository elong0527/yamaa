# Native benchmark qualification gates

The benchmark gate files below cover the complete Python-assisted native route for
[#1738](https://github.com/elong0527/yamaa/issues/1738). They do not change the
reference execution manifest, expected artifacts, compiler ownership or general
native readiness.

- `required.json` names 76 fixture/host/backend/level tuples that must pass both
  independent expected truth and the existing portable reference comparator.
- `known-gaps.json` names five unqualified validation/read-timing differences
  in the historical `reference_assisted_run` route.
  Their exact findings must remain visible. A gap never counts as a pass, cannot
  satisfy a required tuple and cannot exempt reference or infrastructure errors.
  Any changed or disappearing difference requires explicit disposition.
- `missing-routes.json` preserves the broad inventory's missing R route under
  #1739. The separately qualified public cohort does not fill unexercised rows.

`supplemental.json` separately reconciles component/compile contract files and
installed Python/R probe suites. Its runner retains actual suite logs and results;
these do not enter the complete-run benchmark count. See the
[supplemental evidence contract](../SUPPLEMENTAL.md).

The initial set was selected by executing every fixture through the original
YAML, unchanged inputs and independent expected artifacts, then comparing all
portable observations with reference Python. It includes 16 positive and 60
negative cases; frontend validation failures still belong to the assisted route.
The installed wheel/source CI runs regenerate the evidence without generating
or editing these gates or any expected file. Reports identify the actual tested
revision, package versions, artifact names and workflow run. CI artifacts, not
this static list alone, prove a pass on a particular platform/revision.

To qualify another case, implement its missing behavior, obtain a complete
installed report matching independent truth and reference observations, and add
its exact route here. A shared-compiler route requires its own `shared_run` tuple;
an assisted pass cannot satisfy it. Retire a known gap only when its new evidence
has been reviewed. Keep the inventory's denominator synchronized with the
execution manifest rather than fixing it to today's 315 cases.


The separate public M1 gate in `tools/qualify_public_cohort.py` requires eleven
cases from actual installed Python/R public domain runs. It includes the original
six-case cohort and all five historical timing fixtures. Both installed suites
already compare their complete reports with independent truth, including source
reads, source tables, ordered diagnostics and refusal to save a failed build.
The public comparator now requires those reports rather than leaving them outside
its selected cohort. Missing or mismatched reports fail the gate. Windows R
remains required and unqualified until its actual installed evidence passes.

This adds required `shared_run` comparisons for a different executed route. The
historical assisted gaps, manifest, expected artifacts and broad inventory are
retained unchanged; an assisted mismatch is never relabeled as a shared pass.


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
`public-conformance.json` gates both installed hosts. Its 294 Python cases and
293 R cases are the independently replayed intersection of all eighteen Python
lanes and three R hosts in full-dispatch run 38354915028 at
`a44d8dd51e3ff27d62a9ec490b1c8e7a734cc3cf`. The eight per-record aggregate and
inclusive-range benchmarks advance the prior 269 Python and 268 R cases without
removing a pass. Every lane retains 38 unsupported cases and two historical
exclusions. Python and Linux/macOS R retain one mismatch; Windows R retains two,
including the symlink negative that passes elsewhere.
No lane reports an execution error. The host-specific intersection keeps that
Windows R mismatch visible rather than adding it to R's gate. This corpus
evidence does not complete other migration or
release gates. Run locally against an installed package outside the checkout:

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
