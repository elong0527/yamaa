---
title: Leaderboard
hide:
  - actions
---

# Agent leaderboards

How well AI coding agents turn a benchmark's request and input datasets
into the requested dataset. Each agent gets only the benchmark's
`prompt.md` and its input files, in a sandbox that reaches nothing
but the model API, and its output is graded cell by cell against the
benchmark's golden file. A task passes when every cell matches.
[The evaluation](https://github.com/elong0527/yamaa/blob/main/evaluations/harbor/README.md) runs on
[Harbor](https://github.com/harbor-framework/harbor); how each request
is written is in
[the benchmark prompt recipe](https://github.com/elong0527/yamaa/blob/main/automation/benchmark_prompt.md).

Each leaderboard below is defined the way
[Harbor Hub leaderboards](https://docs.harborframework.com/core-concepts/harbor-hub/leaderboards) are: a fixed set of tasks, the
metadata and metrics each row carries, the columns shown, and ordered
ranking rules. A row is one recorded run, and its metrics aggregate the
run's trials as Harbor does, so an errored trial counts as a failure.

## ADaM pilot

Three ADaM benchmarks: age groups in ADSL, death information on every
adverse event in ADAE, and duration of response in ADTTE. An agent gets
the benchmark's prompt and input datasets, and a task passes when its
output matches every golden cell.

Tasks: [adam-adae-death](../benchmark/adam-adae-death.html),
[adam-adsl-age-group](../benchmark/adam-adsl-age-group.html),
[adam-adtte-dor](../benchmark/adam-adtte-dor.html). A run is ranked here
when it makes at least 1 attempt on each task at the tasks' own
timeouts. Rows are ranked by pass rate (highest first), then cell
accuracy (highest first), then cost (lowest first).

| # | Agent | Version | Model | Pass rate | Cell accuracy | Errors | Tokens in | Tokens out | Cost | Date | Run | Trials |
|---:|---|---|---|---:|---:|---:|---:|---:|---:|---|---|---:|
| 1 | opencode | 1.18.33 | opencode-go/muse-spark-1.3-contributor | 100.0% | 100.0% | 0 | 216.3k | 6.2k | $0.008 | 2026-09-29 | [muse-spark-1.3-pilot](https://github.com/elong0527/yamaa/blob/main/evaluations/harbor/results/muse-spark-1.3-pilot.json) | 3 |

Each cell below is the task result and the share of golden cells the
agent reproduced; with several attempts, the passes out of the attempts.

| Benchmark | #1 `muse-spark-1.3-contributor` |
|---|---|
| [adam-adae-death](../benchmark/adam-adae-death.html) | **pass**, 100.0% |
| [adam-adsl-age-group](../benchmark/adam-adsl-age-group.html) | **pass**, 100.0% |
| [adam-adtte-dor](../benchmark/adam-adtte-dor.html) | **pass**, 100.0% |

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
