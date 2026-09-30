# Agent evaluation with Harbor

Runs yamaa benchmarks as agent evaluations: an AI coding agent gets a
benchmark's `prompt.md` and its input datasets, writes the requested
dataset, and is graded cell by cell against the benchmark's golden file.
[Harbor](https://github.com/harbor-framework/harbor) runs the agent in
Docker; this folder only writes Harbor task directories and a job file.

| File | Role |
|---|---|
| `Dockerfile` | the base image: Python and R with data packages, OpenCode's offline settings |
| `system-r.md`, `system-python.md` | the shared system prompt per language: use only that language and write `result.R`/`result.py` |
| `build.py` | benchmarks with a `prompt.md` -> Harbor tasks and `job.json`, one task per benchmark per language |
| `grade.py` | the verifier, copied into every task's `tests/` |
| `leaderboard.py` | Harbor jobs -> `results/`; `results/` -> `leaderboard.md` |
| `leaderboard.md` | the leaderboard page, for review in the repository; not published |
| `leaderboards/` | leaderboard definitions, one file per leaderboard (one per language) |
| `results/` | recorded agent runs, one file per Harbor job |

How to write a prompt is in
[`automation/benchmark_prompt.md`](../../automation/benchmark_prompt.md).

## What a run enforces

- **One language per task.** Each task's `instruction.md` is the shared
  system prompt for its language (`system-r.md` or `system-python.md`)
  followed by the benchmark's `prompt.md`, which itself never names a
  language. The R track requires `/app/output/result.R`, the Python track
  `/app/output/result.py`, each rerunnable to reproduce the datasets. A
  shell call to the other language's interpreter in the trajectory
  (`python3` on the R track, `Rscript` on the Python track), or a bridge
  package in the script (`reticulate`, `rpy2`), zeroes the trial.
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
  match too. Column and row order are reported, not graded. Cells are read
  by column type: numbers compare as numbers within a relative 1e-9 (`1.0`
  equals `1`), dates also accept a midnight datetime, text is compared
  exactly (spaces included), and an empty cell, `NA`, or `.` is no value.
  `reward.json` also carries `cell_accuracy`, `row_accuracy`, `reproduced`,
  and `language_violations`, and `verifier/diff-<file>.csv` lists the
  differing cells.

## Setup

A Docker engine whose kernel supports nftables `fib` (Harbor's allowlist
needs it): Linux, or OrbStack on macOS. Docker Desktop may lack it.

```bash
uv sync --project python --group harbor
docker build -t yamaa-harbor-env:0.2 evaluations/harbor
```

## Build and check the tasks

`build.py` writes the tasks and `job.json` to `~/.cache/yamaa-harbor/`
(`$XDG_CACHE_HOME/yamaa-harbor` when set, or `--out`), and Harbor writes its
job directories under `jobs/` there. Both stay outside the repository,
whose validators read every file in the tree. Each build first removes the
tasks of the previous one.

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

The pilot for development is the three ADaM benchmarks on the
leaderboards, in both languages:

```bash
uv run --project python --no-sync python evaluations/harbor/build.py \
	--benchmarks adam-adsl-age-group adam-adae-death adam-adtte-dor \
	--model opencode-go/muse-spark-1.3-contributor \
	--n-concurrent 3
```

Harbor's `oracle` agent runs a script that writes the golden files byte for
byte, so it also passes the rerun, and must score 1 on every task; its
`nop` agent writes nothing and must score 0:

```bash
uv run --project python --no-sync harbor run \
	-p ~/.cache/yamaa-harbor/tasks \
	-a oracle \
	-o ~/.cache/yamaa-harbor/jobs \
	--job-name oracle \
	-y
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

```bash
export OPENCODE_API_KEY=...
uv run --project python --no-sync python evaluations/harbor/build.py \
	--model opencode-go/muse-spark-1.3-contributor \
	--n-attempts 1 \
	--job-name muse-pilot
uv run --project python --no-sync harbor run \
	-c ~/.cache/yamaa-harbor/job.json \
	-y
```

One job can cover both tracks; name one track with `--language` and a
matching `--job-name` (for example `--language r --job-name muse-pilot-r`)
to record R and Python runs separately.

Each trial directory under `~/.cache/yamaa-harbor/jobs/<job>/` holds the
agent's files (`artifacts/app/`) and trajectory (`agent/trajectory.json`),
the grade (`verifier/grade.json`, `verifier/reward.json`, and a
`diff-<file>.csv` when cells differ), and Harbor's `result.json` with
tokens and cost. `harbor view ~/.cache/yamaa-harbor/jobs` browses them.

## Leaderboards

Each leaderboard follows the
[Harbor Hub model](https://docs.harborframework.com/core-concepts/harbor-hub/leaderboards):
a curated, ranked table whose definition fixes the columns and the ranking
rules, and whose rows are recorded runs that point back to their trials.
`leaderboards/<name>.yaml` holds:

- `tasks` and `attempts`: the benchmarks a run must cover and the fewest
  attempts on each. A run qualifies only at the tasks' own timeouts, so
  every row answers the same question.
- `formats`: how the site prints number columns.
- `harbor`: the Hub definition itself, in the shape
  `harbor hub leaderboard create --config` takes: the `metadata_schema`
  and `metrics_schema` of a row, the `columns`, and the ordered `rank_by`
  rules. Accessors read one flat key, `metadata.<key>` or `metrics.<key>`.

A leaderboard's tasks are its fixed question, as a Hub leaderboard is
pinned to dataset versions: adding a task leaves earlier runs without it,
and `render` then refuses them. Start a new leaderboard instead. There is
one leaderboard per language (`adam-pilot-r`, `adam-pilot-python`), so R
and Python are ranked independently; one job can appear on both boards.

A row's metadata comes from the job (agent, version, model, date,
attempts, job and Harbor version, and the yamaa commit of its tasks) and its metrics from its trials on the
leaderboard's tasks, aggregated as Harbor aggregates them:

| Metric | Meaning |
|---|---|
| `reward` | mean reward, the pass rate; an errored trial counts as 0 (Harbor's mean) |
| `cell_accuracy`, `row_accuracy` | mean share of golden cells and rows reproduced |
| `pass_at_<k>` | Harbor's unbiased pass@k averaged over tasks, for k = 2, 4, 5, 8, 10, ... up to the fewest attempts on a task |
| `n_trials`, `n_errors` | trials aggregated, and trials that ended in an exception |
| `input_tokens`, `output_tokens`, `cost_usd` | totals, when every trial reported them |
| `agent_seconds` | mean agent time per trial |

Harbor skips pass@k when a verifier writes several rewards, as `grade.py`
does, so `leaderboard.py` computes it from `reward`. Rows are ordered with
Harbor Hub's own comparison: each `rank_by` rule in turn, nulls last
unless the rule says `nulls: first`. With the `harbor` group installed,
tests check the pass@k estimator, the ordering and the exported configs
against Harbor's own code; without it they skip.

To compare models, ask for several attempts per task: one attempt is a
pilot, and pass@k needs at least two. Terminal-Bench 2.0, the reference
Harbor benchmark, takes leaderboard runs with `--n-attempts 5`.

### Record a run

```bash
uv run --project python --no-sync python evaluations/harbor/leaderboard.py \
	add ~/.cache/yamaa-harbor/jobs/muse-spark-1.3-pilot
```

`add` writes `results/<job>.json` (job and trial ids, agent, model,
attempts, the yamaa commit the tasks were built from, and each trial's
rewards, tokens, cost and time), puts the run on every leaderboard it
qualifies for (or the ones named with `--leaderboard`), and rebuilds
[`leaderboard.md`](leaderboard.md). It refuses a job that mixes agents or
models, or that qualifies for no leaderboard. `--hide` records a run
without showing it, as Harbor's row `status: hide` does. Commit the result
and the page; a test fails when they disagree.

Results are recorded in the repository for review and are not published:
`leaderboard.md` is not part of the documentation site, and nothing is
uploaded to Harbor Hub. Build a run's tasks from a committed tree, so its
"Tasks from" commit names the prompts it answered (a `+dirty` suffix
means the tree had uncommitted changes).

A run recorded before the per-language split, such as the original
`muse-spark-1.3-pilot`, names a retired leaderboard and is not carried
forward; re-run one job per track instead.
