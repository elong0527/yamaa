# Installed package checks

The Python and R suites exercise installed `domain`, `check` and `define` APIs.
Benchmark directories and `tools.conformance.public_conformance` define the current active
set and explicit exclusions for both hosts. No passing observations are retained:
each native lane gates directly on every active benchmark passing.
`supplemental.json` records only underivable catalog data (contract fixtures and
their consumers, elevated suite levels, renamed suite identities); suites and
plain fixtures are discovered by glob.

## Public reports

`python -m tools.conformance.public_conformance complete-reports` compares every
complete authored report under `crates/yamaa-application/tests/fixtures/specs/`
with retained actual installed Python and R file runs. Complete-report objects
are discovered from their files; malformed report objects fail, while
array-shaped partial diagnostic fixtures belong to their direct native tests.

One inventory retains both installed owners, unchanged authored truth and all
complete observations. Comparisons cover values, types, exact float bits,
ordering, diagnostics, verification, callbacks, resource observations and saved
artifacts. CSV bytes compare exactly; Parquet compares logical fields, types,
nulls, rows and values. An exhaustive complete-report comparison rejects extra
findings; a benchmark's partial `expected/error.yaml` assertion permits other
independent findings while requiring its own fields and every declared path.

```sh
python -m tools.conformance.public_conformance complete-reports \
  --fixtures benchmarks --python-evidence /tmp/native-python/wheel \
  --r-evidence /tmp/native-r/r-source --output /tmp/complete-reports \
  --source-revision REVISION
```

The output directory must be fresh and outside the authored fixture tree.
The revision must equal the reviewed checkout and both installed owners' actual
source revision. Comparator unit fixtures are deliberately marked unqualified;
their successful mutation controls never establish installed release evidence.

`--cli-evidence` names a command-line conformance run, the output directory of
`tools.commands.cli_conformance` (see [Command-line conformance](#command-line-conformance)),
whose saved outputs then join the comparison under `cli/` in the output
directory. The run must name the same revision and Actions run as both
owners, its binary must print their core version, and its own inventory must
have passed. For each cohort case, its outcome and saved files must equal both
owners' complete reports, CSV byte for byte and Parquet logically. Its issues
are not compared there: it ran the cases written as 1.0, whose spec paths are
not the authored truth's, and its inventory compared them with
`expected/error.yaml`.

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

The Python runner's `--output-as` names another path that reaches the output
directory, such as a Windows network share or a drive mapped to it: every
call then names the staged cases through that path as written, unresolved,
while the runner stages and records them in `--output`. The `native-share`
jobs run the corpus that way (`docs/release.md`).
The comparator needs the pinned YAML and Arrow dependencies in `python/uv.lock`;
the R runner needs base R and the installed package.

The plan also generates two metamorphic variants of each entry, built by the
same runners on staged copies and judged by the comparator against the entry's
own build rather than against `expected/`: an empty child
(`{yamaa: "1.0", type: spec, parent: <entry>}`, LAYER-5) must reproduce its
parent, and for eligible positive entries (no `rows`, no `parent`, at least
two key columns reading one input field or a literal) a JSON control copy and
a copy with the key columns moved to the front in reverse key order must agree
(COLUMN-5, OUTPUT-1, OUTPUT-2). Positive variants must report no issues with
equal output and log tables; negative variants must report the same first
issue (phase, condition, rule and spec paths). Known differences are
declared in the case's `benchmark.yaml` under `conformance.metamorphic`,
linked to a tracking issue or marked permanent with a by-design reason. A
temporary difference that stops occurring fails the run as stale; an
unlisted difference fails it as a mismatch. Approved exclusions also belong
to their case's `conformance` metadata.

The comparison commands above gate on the current directories alone: every
active case must pass on that host, including a newly added directory. No
prior observations are consulted, so adding a benchmark needs no registry
edit. Expected data remains unchanged.

## Command-line conformance

`rust/tests/public_conformance_cli.py` is the command line's runner (#3060).
It runs the plan's command-line calls through a `yamaa` binary in staged
copies of the cases, without `expected/`: `yamaa run SPEC`, or
`yamaa run PROJECT --spec NAME` for a call with a project, the calls the
Python and R entries make. Each runs with `--format json`, and the record
keeps its exit status, its standard error and its JSON document, with the
`status`, the issues and the paths its save wrote, relative to the case's
root as the Python and R records keep them (#3312). For the command line, the
plan excludes each benchmark whose project supplies project functions: the
binary has no project-function implementation, so a specification that calls
one fails before it reads data (#3030 section 2.3). Each excluded case still
gets a record that names the reason. The runner reports each case as
`excluded`, `observed` or `error`. A case is observed when each of its runs
exits with status 0 or 1 and prints one JSON document; any other run, such as
one that exits 2 because the command line could not run it as asked
(#3030 D68), makes its record an error.

The comparator judges command-line records by the criteria the hosts' records
meet, with what the command line has: it holds no typed host view, and runs
each call once, with no metamorphic variant. Each run of a positive, and each
domain build of a Define benchmark, exits 0 with the status `succeeded`,
reports no issue and saves its declared report, then its output, at the
declared paths; CSV bytes compare exactly and Parquet logically. Each run of
a negative exits 1, its first issue must match `expected/error.yaml`, and it
saves no more than the report a failed run writes (PROJECT-12). An error
record fails its benchmark, and the gate requires every benchmark that the
command line runs.

The binary dispatches on the type a PATH declares in its `type:` header and
reads only 1.x files (#3030 D122, D125), the language the corpus is written
in. `tools.commands.cli_conformance` copies the corpus, plans the copies,
runs the runner over them and judges the records against the expected files
into `inventory.json`, keeping the copies, the plan and the records under its
output directory. It exits 1 unless every benchmark that the command line
runs passes:

```sh
python -m tools.commands.cli_conformance --binary rust/target/debug/yamaa --output /tmp/public-cli
```

With `--prepare-only` in place of `--binary`, it writes the copies and the
plan and stops. Each `native-cli-install` job prepares them while the
network is up, then runs the runner over them with the unpacked binary of
its archive, with the network disabled, and judges the records with
`python -m tools.conformance.public_conformance compare`
(`docs/release.md`). The cohort comparison reads the output beside its
platform's owners.
