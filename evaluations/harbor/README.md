# Agent evaluation with Harbor

Runs yamaa benchmarks as agent evaluations: an AI coding agent gets a
benchmark's `instruction.md` and its input datasets, writes the requested
dataset, and is graded cell by cell against the benchmark's golden file.
[Harbor](https://github.com/harbor-framework/harbor) runs the agent in
Docker; this folder only writes Harbor task directories and a job file.

| File | Role |
|---|---|
| `Dockerfile` | the base image: Python and R with data packages, OpenCode's offline settings |
| `build.py` | benchmarks with an `instruction.md` -> Harbor tasks and `job.json` |
| `grade.py` | the verifier, copied into every task's `tests/` |
| `leaderboard.py` | recorded runs (`results/`) -> the site's leaderboard page |
| `results/` | recorded agent runs, one file per Harbor job |

How to write an instruction is in
[`automation/benchmark_prompt.md`](../../automation/benchmark_prompt.md).

## What a run enforces

- **The agent sees only the instruction and `/app/input/`.** Specifications,
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

## Record a run on the leaderboard

```bash
uv run --project python --no-sync python evaluations/harbor/leaderboard.py \
	add ~/.cache/yamaa-harbor/jobs/muse-spark-1.3-pilot
```

`add` writes `results/<job>.json` (benchmark, agent, model, rewards,
tokens, cost, time per trial) and rebuilds
[`docs/articles/leaderboard.md`](../../docs/articles/leaderboard.md), the
site's leaderboard page. Commit both; a test fails when the page and the
results disagree. Record only jobs of one agent and model.
