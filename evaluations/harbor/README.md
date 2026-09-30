# Agent evaluation with Harbor

Runs yamaa benchmarks as agent evaluations: an AI coding agent gets a
benchmark's `prompt.md` and its input datasets, writes the requested
dataset, and is graded cell by cell against the benchmark's golden file.
[Harbor](https://github.com/harbor-framework/harbor) runs the agent in
Docker; this folder only writes Harbor task directories and a job file.

| File | Role |
|---|---|
| `Dockerfile` | the base image: Python and R with data packages, OpenCode's offline settings |
| `build.py` | benchmarks with a `prompt.md` -> Harbor tasks and `job.json` |
| `grade.py` | the verifier, copied into every task's `tests/` |
| `leaderboard.py` | Harbor jobs -> `results/`; `results/` -> the site's leaderboard page and Harbor Hub configs |
| `leaderboards/` | leaderboard definitions, one file per leaderboard |
| `results/` | recorded agent runs, one file per Harbor job |

How to write a prompt is in
[`automation/benchmark_prompt.md`](../../automation/benchmark_prompt.md).

## What a run enforces

- **The agent sees only the prompt and `/app/input/`.** Specifications,
  READMEs, and golden files never enter its container; the golden files
  live in `tests/`, which Harbor builds into a separate verifier image.
- **Closed book.** While the agent works, only the model API host is
  reachable. During setup Harbor may also reach GitHub, `nodejs.org`, and
  the npm registry to install OpenCode. OpenCode's `webfetch` and
  `websearch` tools are denied, and the grader zeroes any trial whose
  trajectory calls them. The verifier has no network.
- **Grading.** Reward 1 when every requested dataset has the golden's
  columns, keys, and cell values; column and row order are reported, not
  graded. Cells are read by column type: numbers compare as numbers
  (`1.0` equals `1`), dates also accept a midnight datetime, and an empty
  cell, `NA`, or `.` is no value. `reward.json` also carries
  `cell_accuracy` and `row_accuracy`, and `verifier/diff-<file>.csv` lists
  the differing cells.

## Setup

A Docker engine whose kernel supports nftables `fib` (Harbor's allowlist
needs it): Linux, or OrbStack on macOS. Docker Desktop may lack it.

```bash
uv sync --project python --group harbor
docker build -t yamaa-harbor-env:0.1 evaluations/harbor
```

## Build and check the tasks

`build.py` writes the tasks and `job.json` to `~/.cache/yamaa-harbor/`
(`$XDG_CACHE_HOME/yamaa-harbor` when set, or `--out`), and Harbor writes its
job directories under `jobs/` there. Both stay outside the repository,
whose validators read every file in the tree.

```bash
uv run --project python --no-sync python evaluations/harbor/build.py \
	--model opencode-go/muse-spark-1.3-contributor
```

Harbor's `oracle` agent copies the golden files and must score 1 on every
task; its `nop` agent writes nothing and must score 0:

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
and `render` then refuses them. Start a new leaderboard instead.

A row's metadata comes from the job (agent, version, model, date,
attempts, job and Harbor version) and its metrics from its trials on the
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
attempts, and each trial's rewards, tokens, cost and time), puts the run
on every leaderboard it qualifies for (or the ones named with
`--leaderboard`), and rebuilds
[`docs/articles/leaderboard.md`](../../docs/articles/leaderboard.md), the
site's leaderboard page. It refuses a job that mixes agents or models, or
that qualifies for no leaderboard. `--hide` records a run without showing
it, as Harbor's row `status: hide` does. Commit the result and the page; a
test fails when they disagree.

### Publish to Harbor Hub

A Hub leaderboard belongs to a dataset package you own on the Hub, and
its rows point at uploaded trials. Once the tasks are published as that
package, upload each recorded job, export, and create:

```bash
uv run --project python --no-sync harbor upload ~/.cache/yamaa-harbor/jobs/<job>
uv run --project python --no-sync python evaluations/harbor/leaderboard.py \
	export adam-pilot --package <org>/<dataset>
uv run --project python --no-sync harbor hub leaderboard create \
	--config ~/.cache/yamaa-harbor/hub/adam-pilot.leaderboard.yaml
uv run --project python --no-sync harbor hub leaderboard row create \
	<org>/<dataset>/adam-pilot --config ~/.cache/yamaa-harbor/hub/adam-pilot.rows.yaml
```

A run recorded before trial ids were kept, such as `muse-spark-1.3-pilot`,
exports a row without trial associations.
