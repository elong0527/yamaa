# yamaa-asa-boston-2026-11-12 slides

"AI-First Workflow", subtitled "Starting from CDISC Data Standardization" --
workshop slides for the 2026 Boston Pharmaceutical Statistics Symposium
(2026-11-12).

[View the rendered slides](https://elong0527.github.io/yamaa/slides/yamaa-asa-boston-2026-11-12/index.html)

## Build

The HTML is built by the Deploy docs GitHub Action and published with the
docs site. To preview locally: `quarto render index.qmd` from this folder.
The output is one self-contained HTML file (`embed-resources: true`).

## Outline

The deck has eight sections:

- **Welcome:** the workshop outline, disclaimer, training objectives,
  preparation, and resources. It assumes familiarity with clinical
  development and CDISC standards, plus R programming experience. The
  preconfigured GitHub Codespace provides R, Python, OpenCode, Claude Code,
  and Codex.
- **AI Agents:** examples using arena.ai and the personal agent Muse,
  followed by explanations of AI agents, large language models, and
  harnesses.
- **AI-first workflow:** a mental game on building an AI-native biometrics
  group, the levels of AI adoption, AI-first, and AI-native work, a
  brainstorm on designing an AI-first workflow for CDISC data standardization
  (where to start building the agent, with its answer), and the design
  principles: trustworthy assistance, plain text first, deterministic first,
  and one source of truth.
- **Agentic CDISC data standardization:** the problem of one specification
  producing two results, its root cause, the evidence that prompt
  completeness decides success, and the workflow's goal and principles. It
  introduces yamaa's planning-stage collaboration, YAML specifications, core
  principle, four components, inheritance, R Consortium submission pilots
  and Pilot 7, and Pilot 5 performance. It closes with a workflow summary.
- **AI evaluation:** what a benchmark is, the yamaa benchmarks, how Harbor
  runs and grades an agent on a benchmark, and an invitation to the yamaa
  organization on Harbor Hub.
- **Case study: creating a benchmark for ADaM derivation:** the anatomy of
  the mean arterial pressure benchmark, its three prompt tiers, and rules for
  a fair benchmark.
- **Case study: review and finalize a benchmark:** how people review and
  finalize a yamaa SDTM, SEND, or ADaM benchmark through its review page, with the current round
  status, and how an AI-reported R Consortium Pilot 7 finding is reviewed and
  finalized.
- **Case study: assessing an agent with the benchmark:** per-task results
  for Muse Spark 1.3 from its Harbor Hub jobs, and an exercise on reading a
  job.
