# Agent evaluation with Harbor

Runs yamaa benchmarks as agent evaluations: an AI coding agent gets a
benchmark's prompt and its input datasets, writes the requested dataset,
and is graded cell by cell against the benchmark's golden file.
[Harbor](https://github.com/harbor-framework/harbor) runs the agent in
Docker; this folder only writes Harbor task directories and a job file.

| File | Role |
|---|---|
| `Dockerfile` | the base image: Python and R with data packages, OpenCode's offline settings |
| `system-r.md`, `system-python.md` | the shared system prompt per language: use only that language and write `result.R`/`result.py` |
| `prompts/` | the prompts, `<benchmark>/full.md`, `conventions.md`, and `brief.md`, one file per tier; what each tier means is in [`prompts/README.md`](prompts/README.md) |
| `brief.py` | writes each `brief.md` from its `full.md` |
| `build.py` | benchmarks with a prompt -> Harbor tasks per prompt tier and language with their Harbor Hub READMEs, one dataset README per tier and language, and one job file per tier, language, and model variant |
| `hub.py` | a job's preserved tasks -> private Hub task and dataset revisions, with verified job associations |
| `grade.py` | the verifier, copied into every task's `tests/` |
| `solutions/` | reference solutions, `<benchmark>/result.R` and `result.py`, written from the full prompt alone; the oracle runs them. `result.sas` is the same in SAS, checked in CI only (see [SAS reference solutions](#sas-reference-solutions)) |
| `leaderboard.py` | Harbor job directories -> Harbor Hub leaderboard and row configs |
| `leaderboards/` | leaderboard definitions, one file per leaderboard (one per prompt tier and language) |

Results are not kept in this repository: runs are uploaded to Harbor Hub and
reviewed on its leaderboards.

Tasks are built from the full prompt unless `--prompt` names the
conventions or brief tier (see [Prompt tiers](#prompt-tiers)). The Harbor
Evaluation workflow
(`.github/workflows/harbor-evaluation.yml`) reruns every reference
solution, in R, Python, and SAS, against its benchmark's current data, and
checks that every prompt still asks for the datasets and columns the
expected data holds, so a change to a benchmark that breaks either fails
its pull request.

How to write a prompt is in
[`automation/benchmark_prompt.md`](../../automation/benchmark_prompt.md).

## What a run enforces

- **One language per task.** Each task's `instruction.md` is the shared
  system prompt for its language (`system-r.md` or `system-python.md`)
  followed by the benchmark's prompt of the task's tier, which itself
  never names a language. The R track requires `/app/output/result.R`,
  the Python track `/app/output/result.py`, each rerunnable to reproduce
  the datasets. A
  shell call that runs the other language zeroes the trial: `python`,
  `pip`, or `uv` on the R track, `R` or `Rscript` on the Python track, as a
  command (also behind `env`, `sudo`, `timeout`, `bash -c`, or `$(...)`).
  Naming one is not a call: in a `grep` pattern, a quoted string, a
  comment, a heredoc body, or the tool call's description. So is a bridge
  in the script: `library(reticulate)` or `system("python ...")` in R,
  `import rpy2` or `subprocess.run(["Rscript", ...])` in Python.
- **The agent sees only the prompt and `/app/input/`.** Specifications,
  READMEs, golden files, and input schemas (`*.schema.yaml`) never enter
  its container; the golden files live in `tests/`, which Harbor builds
  into a separate verifier image.
- **Closed book.** While the agent works, only the model API host is
  reachable. During setup Harbor may also reach GitHub, `nodejs.org`, and
  the npm registry to install OpenCode. OpenCode's `webfetch` and
  `websearch` tools are denied, and the grader zeroes any trial whose
  trajectory calls them. The verifier has no network.
- **Grading.** Reward 1 when every requested dataset has the golden's
  columns, keys, and cell values, and the script, rerun by the verifier
  from a clean state (its datasets deleted first), writes datasets that
  match too. Then the held-out rerun: with every third subject (by
  `USUBJID`) dropped from the inputs, the script must write the golden
  restricted to the subjects kept, so a script that writes its rows
  literally fails. Every task ships its reference solution (`solutions/`,
  copied into the verifier's `tests/reference/`), and the check applies
  when that reference, rerun the same way, writes exactly the restricted
  golden; it is skipped with a note when the reference's data differs (a
  derivation across subjects, or inputs without `USUBJID`), and a
  reference that crashes is a verifier error, never an exemption. Then,
  when an output carries `USUBJID`, the changed-input challenge: every
  subject identifier in the inputs is renamed and supported ages,
  measurements and complete dates are changed. The reference computes the
  answer from those inputs, and the script must write it. The grade records
  the shared seed and changed columns. This checks supported counterfactuals;
  it does not prove generalization to every possible input. Column and row
  order are reported, not graded. Cells are read by column type: integers
  compare exactly (`1.0` equals `1`), other numbers within a relative 1e-9,
  dates also accept a midnight datetime, text is compared exactly (spaces
  included), and an empty cell, `NA`, or `.` is no value. An output that is a symlink
  is rejected.
  `reward.json` also carries `cell_accuracy`, `row_accuracy`, `reproduced`,
  `language_violations`, `held_out_checked`, `held_out` (when checked),
  `challenge_checked`, and `challenge_passed`, and
  `verifier/diff-<file>.csv` lists the differing cells.
  Partial accuracy is the minimum across the submitted datasets and every
  checked rerun. Failed execution or a language/web policy violation gets
  zero partial credit; an executable, partly correct derivation keeps its
  credit. Extra rows and columns reduce accuracy, and a row missing any
  required column is not fully correct. Each output in `grade.json` also
  reports `column_accuracy` for comparisons of individual variables.
- **Sandboxed reruns.** The verifier runs the agent's script, and the
  reference, as the image's unprivileged `nobody` user (`sandbox.py`):
  `/tests`, with the golden and the reference, is readable by root only,
  every rerun restores the original inputs from `tests/input/` and starts
  from an empty output folder, and every process the script left is
  killed. A model's job also requires the agent's trajectory
  (`YAMAA_REQUIRE_TRAJECTORY`), so the language and web-tool audit always
  has a record to read. `run.py` copies each task into the job and saves
  `evaluation.json` before execution, so setup failures retain provenance
  and later builds cannot change what the job runs. The verifier's saved
  `task.toml` must agree with that snapshot.

## SAS reference solutions

Every benchmark solved in R and Python is also solved in SAS, in
`solutions/<benchmark>/result.sas`, written the same way: from the full
prompt and the inputs alone. SAS has no Harbor track yet; no system
prompt, task, or leaderboard uses these scripts. The Harbor Evaluation
workflow runs each one with [OpenSAS](https://github.com/kirha-ai/opensas),
an open-source interpreter for the SAS 9.4 language, at the release its
`OPENSAS_VERSION` pins, and grades the datasets it writes as it grades the
other two. `adam-adsl-randomization` has none: its DM input is Parquet,
which neither SAS 9.4 nor OpenSAS reads. To run them locally, put that
release on `PATH` as `sas`; without it the SAS cases skip.

```bash
uv run --project python --no-sync pytest python/tests/test_harbor_evaluation.py \
	-k "sas"
```

OpenSAS runs some valid SAS 9.4 differently, and accepts some code SAS 9.4
would not. Write each script so that it is correct under both:

- Read every input with a DATA step `INFILE ... dsd firstobs=2 truncover`
  that declares each column's length or informat, with identifiers as text,
  rather than PROC IMPORT. OpenSAS keeps `0001` as text where SAS 9.4 reads
  the number 1, and a guessed type can change when the verifier's reruns
  drop subjects.
- Declare every character variable's length before its first assignment.
  OpenSAS sizes it from the data, which hides a truncation SAS 9.4 makes.
- `trim()` a variable before matching it with a PRX pattern; SAS 9.4 sees
  its trailing blanks.
- Join on a range with `where lo <= x <= hi`; OpenSAS ignores
  `on x between lo and hi`. In a join, qualify a column both tables hold
  when ORDER BY names it.
- End with one PROC SQL `select` of the output columns, in the prompt's
  order and case, with each date as `put(DATE, yymmdd10.)` text that is
  empty when the date is missing, then PROC EXPORT that table with no data
  set options. OpenSAS's PROC EXPORT writes dates as day counts and ignores
  KEEP=, DROP=, WHERE=, and RENAME=.

OpenSAS is a project by KIRHA, licensed under the Apache License 2.0. The
workflow downloads its release binary and runs it; this repository holds no
OpenSAS code or files, and the `result.sas` scripts are original work under
this repository's MIT license. SAS is a registered trademark of SAS
Institute Inc.; neither yamaa nor OpenSAS is affiliated with, endorsed by,
or sponsored by SAS Institute Inc.

## Setup

A Docker engine whose kernel supports nftables `fib` (Harbor's allowlist
needs it): Linux, or OrbStack on macOS. Docker Desktop may lack it.

```bash
uv sync --project python --group harbor
docker build -t yamaa-harbor-env:0.5 evaluations/harbor
```

## Build and check the tasks

`build.py` writes to `~/.cache/yamaa-harbor/` (`$XDG_CACHE_HOME/yamaa-harbor`
when set, or `--out`): the tasks under `tasks/`, one dataset README per
language under `datasets/<language>/`, and the job files under `configs/`.
Harbor writes its job directories under `jobs/` there. All of it stays
outside the repository, whose validators read every file in the tree. Each
build first removes the tasks, datasets, and job files of the previous one.

Each language is its own Harbor Hub dataset, `yamaa/yamaa-sdtm-adam-r` and
`yamaa/yamaa-sdtm-adam-python` (`--dataset-prefix` changes the part before
`-<language>`), so R and Python are published, run, and ranked apart. Each
task gets a `README.md`, its page on Harbor Hub: the benchmark's own
README, the inputs, the outputs and keys, and the grading and network
rules. It sits at the task root, which never enters the agent's container.
`datasets/<language>/README.md` is the dataset's page, listing its tasks.

```bash
uv run --project python --no-sync python evaluations/harbor/build.py \
	--model opencode-go/muse-spark-1.3-contributor
```

One build covers both tracks (`--language r python`, the default); pass
`--language r` or `--language python` to build a single track. Each
benchmark becomes `<benchmark>-r` and `<benchmark>-python` tasks.
A benchmark is skipped, with its reason on stderr, when the grader
cannot check it: its golden is a warning or violation log, a golden cell
holds a token the grader reads as no value (`NA`, `.`), a column's type is
not one the grader knows, or no specification writes a golden dataset.
Naming one with `--benchmarks` fails fast instead. Domains built together
(`spec_dm.yaml` and `spec_suppdm.yaml`) are one task that grades both
datasets.

### Prompt tiers

`--prompt` picks the prompt tiers to build: `full` (the default),
`conventions`, and `brief`; [`prompts/README.md`](prompts/README.md) says
what each keeps. Every tier is its own Hub dataset per language,
`yamaa/yamaa-sdtm-adam-<tier>-<language>`, with tasks
`<benchmark>-<tier>-<language>`, its page in `datasets/<tier>-<language>/`,
and job files `<job-name>-<tier>-<language>[-<variant>].json`; the full
prompt keeps the names above. Inputs, expected data, the grader, and the
reference solution are the same in every tier, so the oracle scores 1 on
every task and `nop` 0. On a shorter prompt that shows the task builds and
grades, not that the prompt can be solved, and each tier task's page says
so.

```bash
uv run --project python --no-sync python evaluations/harbor/build.py \
	--prompt conventions brief \
	--model opencode-go/muse-spark-1.3-contributor \
	--variant low \
	--out ~/.cache/yamaa-harbor-tiers
```

To compare tiers, run the same model and variant, with the same attempts,
on all three, and read its rows on the three boards of one language.

For development, a pilot of three ADaM benchmarks in both languages is a
quick check before a full run:

```bash
uv run --project python --no-sync python evaluations/harbor/build.py \
	--benchmarks adam-adsl-age-group adam-adae-death adam-adtte-dor \
	--model opencode-go/muse-spark-1.3-contributor \
	--n-concurrent 3 \
	--job-name muse-spark-1.3-pilot
```

Harbor's `oracle` agent runs each task's `solution/result.R` or
`result.py` and must score 1 on every task, rerun included; its `nop`
agent writes nothing and must score 0. That script is the benchmark's
reference solution from `solutions/`, written from the full prompt and
the inputs alone, which also shows the prompt can be solved. Every task
needs one, since the held-out and changed-input reruns compute their
answers with it, and a benchmark without one is not built. To run the
oracle:

```bash
uv run --project python --no-sync harbor run \
	-p ~/.cache/yamaa-harbor/tasks \
	-a oracle \
	-o ~/.cache/yamaa-harbor/jobs \
	--job-name oracle \
	-y
```

`smoke.py` does both checks offline in one step: it builds the tasks,
probes the verifier sandbox (`probe.py`: the script cannot read the golden
files, change the inputs or the grader's logs, or leave a process
behind), then randomly samples ten buildable tasks without replacement.
Both `oracle` and `nop` run the same sample: ten trials each, twenty total.
`oracle` must score 1 with every required challenge checked, and `nop` must
score 0. If fewer than ten tasks are available, it checks all of them.
`sample.json` saves the selected tasks and random seed; pass that seed with
`--seed` to repeat the sample from the same source and prompt tiers.
The Harbor Oracle workflow (`.github/workflows/harbor-oracle.yml`) samples
across all three prompt tiers and both languages on pull requests, Mondays,
and manual runs. Before a model run, use the same sampled preflight:

```bash
uv run --project python --no-sync python evaluations/harbor/smoke.py \
	--prompt full conventions brief \
	--n-concurrent 6 \
	--out ~/.cache/yamaa-harbor-smoke
```

## Run an agent

The provider is chosen per run: `--model <provider>/<model>` picks the
model API host and the key variable, and the key is read from your shell
when the job starts, never written to a file.

| Provider prefix | Host | Key |
|---|---|---|
| `opencode-go`, `opencode` | `opencode.ai` | `OPENCODE_API_KEY` |
| `anthropic` | `api.anthropic.com` | `ANTHROPIC_API_KEY` |
| `openai` | `api.openai.com` | `OPENAI_API_KEY` |
| `xai` | `api.x.ai` | `XAI_API_KEY` |

Any other provider, or a gateway, takes `--api-host` and `--key-env`.
Each job file allows the agent 900 s to install (`override_setup_timeout_sec`),
since many trials downloading at once slow it; the agent's own time limit
is the task's, which leaderboard runs must keep.

The build writes one job file per language and model variant,
`configs/<job-name>-<language>-<variant>.json`, so each job is one row on
one language's leaderboard. `--variant` takes the model's OpenCode
variants, which set its reasoning effort; for
`opencode-go/muse-spark-1.3-contributor` they are `minimal`, `low`,
`medium`, `high`, and `xhigh` (`opencode models opencode-go --verbose`
lists them). Without `--variant` there is one job per language at the
model's default. `--job-name` is the prefix, by default the model's name.

```bash
export OPENCODE_API_KEY=...
for job in ~/.cache/yamaa-harbor/configs/*.json; do
	uv run --project python --no-sync python evaluations/harbor/run.py -c "$job"
done
```

Use this wrapper for ranked runs. It refuses an existing job directory;
choose a new job name for a new complete run. Automatic retries are disabled.
Do not select successful attempts from an interrupted or failed job.
Keep the job's `evaluation.json` and `task-snapshots/` with its results until
the Hub confirms the upload.
At trial start the wrapper also saves the task metadata and manifest under
`verifier/`, which Harbor includes in its upload/download archives. It restores
the manifest after verification resets that directory, retaining the verifier's
own task snapshot for comparison. Successful and failed setup trials therefore
keep their evidence after a Hub download.

After each complete model job, the wrapper uploads every trial and its
artifacts privately to the `yamaa` organization on Harbor Hub, including
failed attempts. Authenticate once with `harbor auth login` as a member of
`yamaa`. The wrapper first publishes the job's exact task snapshots and a
dataset revision for each language and prompt tier. Generated task configs
carry their dataset source, including a custom `--dataset-prefix`.
Oracle and nop preflight jobs stay local. After the Hub confirms
the finalized job archive, every trial archive, every dataset association,
and the leaderboard row's scores and trial links,
the wrapper deletes the local trial artifacts, task snapshots, logs, and upload
caches. It keeps
only `config.json`, `result.json`, and `hub-upload.json` as completion records
for batch status and the Hub link. Unrelated files in the job directory are
left alone. Upload or verification failures retain the local artifacts.
`hub-upload.json` records upload, verification, and cleanup status, along with
the dataset and leaderboard names, immutable revisions, IDs, and links.
It retains the calculated leaderboard submission so a failed publication or
partial cleanup can be retried without rerunning the model. Before associating a
job, the wrapper checks that every uploaded attempt matches the published
dataset's exact task versions. Harbor Hub's job config uses those pinned
dataset references; `local_task_snapshots` retains the original execution
paths there without selecting the same tasks twice for replay. Trial source
labels also name their dataset. After checking the complete grading evidence,
the wrapper submits one row per job to the board for its task release, language
and prompt tier. Every board ranks by task pass rate (`reward`), shows that score
as a percentage and displays successful attempts / total attempts. Cell accuracy
is a secondary metric. Each row links to its job and every trial, including
failed attempts. Repeated uploads reuse the existing row.
Publishing, association or leaderboard verification failures keep the
local artifacts and can be retried with the same command as an upload failure.
An upload failure is
reported without stopping subsequent benchmark jobs; retry the upload,
without rerunning the model, with:

```bash
uv run --project python --no-sync python evaluations/harbor/run.py \
	--upload-only ~/.cache/yamaa-harbor/jobs/<job>
```

Each trial directory under `~/.cache/yamaa-harbor/jobs/<job>/` holds the
agent's files (`artifacts/app/`, with its `output/result.R` or
`output/result.py`) and trajectory (`agent/trajectory.json`),
the grade (`verifier/grade.json`, `verifier/reward.json`, and a
`diff-<file>.csv` when cells differ), and Harbor's `result.json` with
tokens and cost until upload is confirmed. To browse or export results after
cleanup, restore the job with `harbor job download <job-id>`.

## Leaderboards

Each leaderboard follows the
[Harbor Hub model](https://docs.harborframework.com/core-concepts/harbor-hub/leaderboards):
a curated, ranked table whose definition fixes the columns and the ranking
rules, and whose rows are uploaded runs that point back to their trials.
`leaderboards/<name>.yaml` holds:

- `package`: the Harbor Hub dataset the board belongs to, the dataset of
  its language and prompt tier.
- `grading_protocol`: the verifier's checks a run must have been graded
  with (`GRADING_PROTOCOL` in `build.py`, recorded in every task).
- `image_reference`: the runtime reference every ranked row must use.
- `tasks` and `attempts`: the benchmarks a run must cover and the fewest
  attempts on each (every buildable benchmark, one attempt), with the
  same number of attempts on every task.
- `harbor`: the Hub definition itself, in the shape
  `harbor hub leaderboard create --config` takes: the `metadata_schema`
  and `metrics_schema` of a row, the `columns`, and the ordered `rank_by`
  rules. Accessors read one flat key, `metadata.<key>` or `metrics.<key>`.

A leaderboard's tasks are its fixed question, as a Hub leaderboard is
pinned to dataset versions: adding a task leaves earlier runs without it,
and `export` then refuses them. Start a new leaderboard instead. There is
one leaderboard per dataset, so one per language and prompt tier
(`yamaa/yamaa-sdtm-adam-r/sdtm-adam-v3-r`,
`yamaa/yamaa-sdtm-adam-brief-r/sdtm-adam-v3-brief-r`, ...), and R, Python,
and each tier are ranked independently. All boards rank by complete task pass
rate before cell accuracy. Shorter prompts can leave sponsor choices unstated,
so their strict pass rates include the model's ability to infer those choices.
The boards are `v3` because
grading protocol 3 changes derivation inputs, grades regenerated partial
answers, and requires complete job evidence. Older scores stay on their
existing boards. Export appends the full source commit to the Hub board name
and pins that commit and image reference in its metadata schema, so separately
published runs cannot mix releases. Compared jobs must share the same commit.

A row is one job: one agent, model, and variant, so the variants of a model
sit side by side. Its metadata comes from the job (agent, version, model,
variant, date, attempts, job and Harbor version, and the yamaa commit,
grading protocol, and image of its tasks, read from the `task.toml` each
trial's verifier saved), the effective agent configuration with credentials
redacted, and its metrics from its trials on the
leaderboard's tasks:

| Metric | Meaning |
|---|---|
| `reward` | the pass rate: each task's mean reward, averaged over tasks; an errored trial counts as 0 |
| `trial_reward` | the mean reward over trials (Harbor's mean) |
| `reward_ci_low`, `reward_ci_high` | a 95% bootstrap interval of `reward` over tasks (2,000 resamples); it shows how much the score depends on which tasks are in, not the noise within a task |
| `cell_accuracy`, `row_accuracy` | mean minimum accuracy across submitted and regenerated datasets, with penalties for extra records/columns and zero for failed execution or policy violations |
| `pass_at_<k>` | Harbor's unbiased pass@k averaged over tasks, for k = 2, 4, 5, 8, 10, ... up to the fewest attempts on a task |
| `n_trials`, `n_errors` | trials aggregated, and trials that ended in an exception |
| `input_tokens`, `output_tokens`, `cost_usd` | totals, when every trial reported them |
| `cost_per_trial_usd` | `cost_usd` per trial |
| `agent_seconds` | mean agent time per trial |

Harbor skips pass@k when a verifier writes several rewards, as `grade.py`
does, so `leaderboard.py` computes it from `reward`. Harbor Hub orders the
rows by the board's `rank_by` rules. With the `harbor` group installed,
tests check the pass@k estimator and the exported configs against
Harbor's own code; without it they skip.

To compare models, ask for several attempts per task: one attempt is a
pilot, and pass@k needs at least two. Terminal-Bench 2.0, the reference
Harbor benchmark, takes leaderboard runs with `--n-attempts 5`.

Choose the attempt count before running any compared model and keep it equal.
Compare prompt tiers in paired runs of the same tasks, model, variant and
configuration. Inspect `column_accuracy` in the original, replay and challenge
outputs separately: copied input values, derived values, and withheld sponsor
labels answer different questions. A brief score includes guesses about
unstated sponsor choices and is not a standalone measure of CDISC correctness.
Report differences by task/domain as well as an overall mean; related tasks
are not independent evidence of generalization.

### Publish to Harbor Hub

Everything is private to the `yamaa` organization on Harbor Hub: the tasks,
the six datasets, the uploaded jobs, and the leaderboards. Log in once with
`harbor auth login` (GitHub OAuth) as a member of `yamaa`. Build the tasks
from a committed tree, so each row's "Tasks from" commit names the prompts
it answered (a `+dirty` suffix means uncommitted changes), and publish
exactly the task directories the job ran, so the uploaded trials match the
published tasks. Publish the tasks before the datasets, whose manifests
point at them; `dataset init` keeps the README the build wrote. Publish one
tier from a build of that tier alone, in its own `--out`: `tasks/*-r` also
matches `*-brief-r` and `*-conventions-r`. For the R dataset of the full
prompt (repeat with `python`; for a tier, use its dataset name and its
`datasets/<tier>-r` folder):

```bash
H="uv run --project python --no-sync harbor"
D=~/.cache/yamaa-harbor/datasets/r
$H publish ~/.cache/yamaa-harbor/tasks/*-r \
	--private
$H dataset init yamaa/yamaa-sdtm-adam-r \
	-o $D \
	--description "yamaa SDTM and ADaM benchmarks, R track"
$H add ~/.cache/yamaa-harbor/tasks/*-r \
	--to $D
$H publish $D \
	--private
$H upload ~/.cache/yamaa-harbor/jobs/<job> \
	--org yamaa \
	--private
```

A republish of changed tasks makes a new revision of each task and of the
dataset. The board links dataset revisions by id, and
`harbor hub leaderboard update <board> --dv-id <id> ...` replaces that
list, so pass every revision its rows ran on.

The runner publishes the board and its row automatically before cleanup.
For manual publication or older jobs, restore cleaned jobs with
`harbor job download <job-id>`, then write each leaderboard's configs from
the job directories and create the board and its rows:

```bash
uv run --project python --no-sync python evaluations/harbor/leaderboard.py \
	export sdtm-adam-v3-r \
	~/.cache/yamaa-harbor/jobs/muse-spark-1.3-contributor-r-low
$H hub leaderboard create \
	--config ~/.cache/yamaa-harbor/hub/sdtm-adam-v3-r.leaderboard.yaml
```

`export` takes the board's dataset from its `package`; `--package`
overrides it.
Then run the `row create` command printed by `export`. It uses the exported
name, including its full source commit. Reuse that board only for that release
and runtime reference;
do not update it to admit incompatible revisions.

An uploaded job keeps each trial's artifacts, so the agent's script stays
on the Hub under the trial's `artifacts/app/output/`, and
`harbor job download <job-id>` brings a whole job back, scripts included.

`export` refuses a job that mixes agents, models, or variants, misses a
board task or attempts, has unequal attempts across tasks, or changed the
tasks' timeouts, resources, mounts, network access, skills, or verifier.
It requires a finished job with all declared attempts and checks uniform
effective agent settings within each row.
It also refuses a trial without the grading evidence `grade.json` keeps
(a checked trajectory, and the challenge where one is required), and a
job whose tasks do not share one committed (not `+dirty`) yamaa commit,
the board's grading protocol, and one image. `--hide` exports its rows
hidden. Repeat for `sdtm-adam-v3-python` with the Python jobs. The published
tasks build from the local `yamaa-harbor-env:0.5` image, so they run where
that image is built.

Build the runtime once for a comparison and reuse that artifact reference.
The catalog comes from the pinned OpenCode binary, with model fetching off;
there is no live catalog download during the image build. Package managers
still resolve system and Node packages during builds, so rebuilding a local
tag is not a reproducibility guarantee. For shared evaluations, publish the
image once under an immutable versioned reference and set `image_reference`
on the board to that reference before running it (`build.py --image`).
