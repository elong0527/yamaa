---
title: Leaderboard
hide:
  - actions
---

# Agent leaderboard

How well AI coding agents turn a benchmark's request and input datasets
into the requested dataset. Each agent gets only the benchmark's
`instruction.md` and its input files, in a sandbox that reaches nothing
but the model API, and its output is graded cell by cell against the
benchmark's golden file. A task passes when every cell matches.
[The evaluation](https://github.com/elong0527/yamaa/blob/main/evaluations/harbor/README.md) runs on
[Harbor](https://github.com/harbor-framework/harbor); how each request
is written is in
[the benchmark prompt recipe](https://github.com/elong0527/yamaa/blob/main/automation/benchmark_prompt.md).

## Models

| Agent | Model | Passed | Cell accuracy | Tokens in / out | Cost | Run |
|---|---|---|---|---|---|---|
| opencode 1.18.33 | `opencode-go/muse-spark-1.3-contributor` | 3 / 3 | 100.0% | 216.3k / 6.2k | $0.008 | 2026-09-29 |

## Benchmarks

Each cell is the task result and the share of golden cells the agent
reproduced; with several attempts, the passes out of the attempts.

| Benchmark | `muse-spark-1.3-contributor` |
|---|---|
| [adam-adae-death](../benchmark/adam-adae-death.html) | **pass**, 100.0% |
| [adam-adsl-age-group](../benchmark/adam-adsl-age-group.html) | **pass**, 100.0% |
| [adam-adtte-dor](../benchmark/adam-adtte-dor.html) | **pass**, 100.0% |

## Reading the results

- **Few attempts.** A pilot run makes one attempt per benchmark, so a
  single pass or failure says little about a model on its own.
- **Public data.** The inputs and golden files are published in this
  repository, so a model may have seen them in training; the sandbox
  only rules out looking them up during the run.
- **Contributor models.** A model whose name ends in `-contributor`
  runs at a discount in exchange for its provider training on the
  prompts and answers, which may inflate that provider's later scores.
