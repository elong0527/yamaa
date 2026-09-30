# Agent leaderboards

How well AI coding agents turn a benchmark's request and input datasets
into the requested dataset. Each agent gets a system prompt naming
its language (R or Python), the benchmark's `prompt.md` and its
input files, in a sandbox that reaches nothing
but the model API, and its output is graded cell by cell against the
benchmark's golden file. A task passes when every cell matches and
the required script (`result.R` or `result.py`), rerun from a clean
state, writes the same datasets. Results are recorded in this
repository for review and are not published.
[The evaluation](README.md) runs on
[Harbor](https://github.com/harbor-framework/harbor); how each request
is written is in
[the benchmark prompt recipe](../../automation/benchmark_prompt.md).

Each leaderboard below is defined the way
[Harbor Hub leaderboards](https://docs.harborframework.com/core-concepts/harbor-hub/leaderboards) are: a fixed set of tasks, the
metadata and metrics each row carries, the columns shown, and ordered
ranking rules. A row is one recorded run, and its metrics aggregate the
run's trials as Harbor does, so an errored trial counts as a failure.

## ADaM pilot (Python)

Three ADaM benchmarks solved in Python: age groups in ADSL, death
information on every adverse event in ADAE, and duration of response in
ADTTE. An agent gets the Python system prompt, the benchmark's prompt
and input datasets, and a task passes when its output matches every
golden cell and `/app/output/result.py` exists.

Tasks:
[adam-adae-death-python](../../benchmarks/adam-adae-death/README.md),
[adam-adsl-age-group-python](../../benchmarks/adam-adsl-age-group/README.md),
[adam-adtte-dor-python](../../benchmarks/adam-adtte-dor/README.md). A
run is ranked here when it makes at least 1 attempt on each task at the
tasks' own timeouts. Rows are ranked by pass rate (highest first), then
cell accuracy (highest first), then cost (lowest first).

| # | Agent | Version | Model | Pass rate | Cell accuracy | Errors | Tokens in | Tokens out | Cost | Date | Tasks from | Run | Trials |
|---:|---|---|---|---:|---:|---:|---:|---:|---:|---|---|---|---:|
| 1 | opencode | 1.18.33 | opencode-go/muse-spark-1.3-contributor | 100.0% | 100.0% | 0 | 242.6k | 6.6k | $0.010 | 2026-09-30 | ad99be33 | [muse-spark-1.3-dry-run-2026-09-30](results/muse-spark-1.3-dry-run-2026-09-30.json) | 3 |

Each cell below is the task result and the share of golden cells the
agent reproduced; with several attempts, the passes out of the attempts.

| Benchmark | #1 `muse-spark-1.3-contributor` |
|---|---|
| [adam-adae-death-python](../../benchmarks/adam-adae-death/README.md) | **pass**, 100.0% |
| [adam-adsl-age-group-python](../../benchmarks/adam-adsl-age-group/README.md) | **pass**, 100.0% |
| [adam-adtte-dor-python](../../benchmarks/adam-adtte-dor/README.md) | **pass**, 100.0% |

## ADaM pilot (R)

Three ADaM benchmarks solved in R: age groups in ADSL, death information
on every adverse event in ADAE, and duration of response in ADTTE. An
agent gets the R system prompt, the benchmark's prompt and input
datasets, and a task passes when its output matches every golden cell
and `/app/output/result.R` exists.

Tasks: [adam-adae-death-r](../../benchmarks/adam-adae-death/README.md),
[adam-adsl-age-group-r](../../benchmarks/adam-adsl-age-group/README.md),
[adam-adtte-dor-r](../../benchmarks/adam-adtte-dor/README.md). A run is
ranked here when it makes at least 1 attempt on each task at the tasks'
own timeouts. Rows are ranked by pass rate (highest first), then cell
accuracy (highest first), then cost (lowest first).

| # | Agent | Version | Model | Pass rate | Cell accuracy | Errors | Tokens in | Tokens out | Cost | Date | Tasks from | Run | Trials |
|---:|---|---|---|---:|---:|---:|---:|---:|---:|---|---|---|---:|
| 1 | opencode | 1.18.33 | opencode-go/muse-spark-1.3-contributor | 100.0% | 100.0% | 0 | 305.0k | 6.4k | $0.011 | 2026-09-30 | ad99be33 | [muse-spark-1.3-dry-run-2026-09-30](results/muse-spark-1.3-dry-run-2026-09-30.json) | 3 |

Each cell below is the task result and the share of golden cells the
agent reproduced; with several attempts, the passes out of the attempts.

| Benchmark | #1 `muse-spark-1.3-contributor` |
|---|---|
| [adam-adae-death-r](../../benchmarks/adam-adae-death/README.md) | **pass**, 100.0% |
| [adam-adsl-age-group-r](../../benchmarks/adam-adsl-age-group/README.md) | **pass**, 100.0% |
| [adam-adtte-dor-r](../../benchmarks/adam-adtte-dor/README.md) | **pass**, 100.0% |

## Reading the results

- **Attempts.** A leaderboard that asks for one attempt per task is a
  pilot: a single pass or failure says little about a model on its
  own. With several attempts, rows also carry Harbor's pass@k.
- **Public data.** The inputs and golden files are published in this
  repository, so a model may have seen them in training; the sandbox
  only rules out looking them up during the run.
- **Contributor models.** A model whose name ends in `-contributor`
  runs at a discount in exchange for its provider training on the
  prompts and answers, which may inflate that provider's later scores.
