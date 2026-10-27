# yamaa ASA Boston short course

November 12, 2026. A Quarto slide skeleton for developing the short course.

## Files and preview

- `index.qmd` contains the course structure, proposed exercises, source
  references, and presenter notes for material still to prepare.
- `custom.css` contains this deck's styles.
- This README keeps the course premise, provisional timing, and preparation
  decisions alongside the deck.

From this folder, preview the self-contained HTML deck with:

```bash
quarto render index.qmd
quarto preview index.qmd
```

The documentation deployment workflow renders this deck with the other
presentations. The folder README is excluded from MkDocs to reserve
`slides/yamaa-asa-boston-20261112/index.html` for the deck.

## Course premise

Build an AI first clinical data workflow with yamaa in GitHub Codespaces.
Participants use a coding agent to turn clinical requirements into explicit
specifications, test those specifications against independently authored
expectations, and assess a study build for human review and deployment.

The central question is: **How do we turn an AI generated result into a
workflow that people can understand, verify, change, and operate?**

yamaa provides a concrete setting for that question. People and agents can
revise a specification together, while the engine executes its declared
rules. This makes requirements, assumptions, tests, and review decisions
visible throughout the software development life cycle (SDLC).

## Working title and audience

**Building AI first clinical data workflows with yamaa**

Assume participants are statistical programmers, statisticians, or clinical
data scientists familiar with SDTM and ADaM and able to read basic R or
Python. Start with a prepared repository so that prior agent development
experience is optional. Choose one primary programming language for the
guided exercises; offer the other as a follow-up path.

By the end, each participant should have a reviewed specification change,
an independently checked benchmark, a study integration report, and a
short explanation of what still needs human judgment before deployment.

## The two primary repositories

Use [yamaa](https://github.com/elong0527/yamaa) for the specification language,
rules, engine, focused benchmarks, and agent evaluation. Use
[Pilot 7](https://github.com/RConsortium/submissions-pilot7-synthetic-data)
for study inputs, project specifications, published ADaM references,
comparators, and the human review workspace.

The proposed guided study is CDISC Pilot 01, which has ten active ADaM
specifications. Submission Pilots 3 and 5 each supply five active ADaM
specifications and published references. These are coverage counts from
the study catalog, not a new course validation result. KN189 and KN564
supply synthetic ODM sources and workflow-readiness views; their full
derivation workflows are ongoing. Submission Pilot 6 is staged but is
outside the current three-study yamaa publication catalog.

## Proposed teaching environment

| Component | Role in the course |
| --- | --- |
| GitHub Codespaces | A reproducible development environment with the course repository, tools, data, and exercises prepared. |
| OpenCode | The agent harness that gives the model repository context, editing and execution tools, permissions, and an iterative work loop. |
| Muse Spark | A proposed model for the hands-on agent sessions. Confirm its provider, exact model ID, access, and cost before preparing the environment. |
| Muse | A requested part of the stack whose exact product and role still need clarification. |
| yamaa | The specification language and deterministic execution engine for clinical data standardization. |
| Independent grader | Checks authored expectations and reports differences; its judgments do not depend on the coding agent declaring success. |

The existing NJSUG deck supplies an OpenCode and Muse Spark example. Select
and verify versions for this course rather than treating that historical
configuration as an installation recipe.

## A single story across the course

Begin with a realistic request: derive the adverse event analysis start date
and treatment emergence flag. Let participants identify the missing decisions:
partial-date imputation, missing or invalid dates, the subject-level treatment
date, and the source document governing each rule.

The yamaa `adam-adae-partial-dates` benchmark supplies the 15th with a
treatment-date bound. CDISC Pilot 01's retained analysis-guide quotation
instead calls for the first of the month when the day is missing. This
difference connects the two repositories and motivates study-specific
context, independent expectations, and human review. The benchmark's
different policy is intentional; the exercise must not rewrite it to match
the pilot. Author a separate toy case from the agreed study rule.

Then introduce a requirement change in a prepared study workflow. Follow
it through the specification, affected datasets, metadata, tests, and review
record. The instructor should supply the surrounding study so participants
can focus on the change and its consequences within the available time.

Use independently authored toy data for interactive changes and a prepared
CDISC Pilot 01 run for the whole-study discussion. Preserve the pilot source
data and independent reference datasets. The Pilot 7 validator checks the
project, builds its active ADaM datasets, compares them, and records study
evidence. Its review dashboard connects source documents, specifications,
and outputs; reference agreement and clinical review remain separate.

## Topics and exercises

| Topic | Practical question | Participant activity and evidence |
| --- | --- | --- |
| Agent definition and components | What makes a model into an agent that can work on this task? | Inspect its objective, instructions, context, model, tools, state, permissions, feedback loop, and stopping conditions. Trace one tool call and explain why the agent took it. |
| AI first workflow and SDLC | Which decisions should people and agents make before implementation? | Convert a request into requirements, examples, a plan, and acceptance criteria. Ask the agent to draft a yamaa specification, inspect the diff, and resolve an ambiguity before running it. |
| Benchmark creation | What evidence would distinguish a correct result from a plausible one? | Author a tiny input and expected result independently. Add missing-value, duplicate, or boundary cases; compare an incomplete prompt with an explicit contract while keeping the model and budget fixed. |
| Whole study integration testing | Do individually correct derivations still form a coherent study? | Run a prepared project and check dependencies, subject identifiers, keys, row counts, cross-dataset relationships, metadata, and expected outputs. Introduce one upstream change and inspect its downstream effects. |
| Deployment considerations | What must travel with an approved workflow? | Identify the engine, specification, templates, dependencies, input snapshot, configuration, and test evidence needed to reproduce a build. Walk through a failed build, recovery, and rollback. |
| Human and AI interaction | When should the agent act, ask, or stop? | Review an ambiguous requirement, an agent plan, a diff, and a failing result. Record the human decision and the evidence used to accept or reject the change. |

Keep human interaction visible in every exercise. A useful repeated loop is:
state the goal, expose assumptions, agree on a plan, inspect changes,
evaluate evidence, and accept or revise.

## Redesigning the SDLC

Give participants a concrete set of artifacts to carry through the workflow:

1. A requirement with unresolved clinical decisions called out.
2. Independently authored examples and acceptance criteria.
3. A plan naming the agent's scope and the human decisions it needs.
4. A versioned yamaa specification and its reviewed changes.
5. Benchmark and study integration results, including failures.
6. A human review decision and a reproducible release package.

Ask where each artifact belongs in their organization's current process,
who owns it, and which steps can be automated. A requirement change should
update the relevant artifacts together. The learning objective is to design
the feedback and review process around AI participation.

## Benchmark and integration lessons

During development, participants may inspect the reference examples.
During an evaluation, keep held-out inputs, expected outputs, and grading
logic outside the agent's accessible environment. A prompt asking the agent
not to read a directory does not provide isolation.

Expected results must come from clinical rules and independent calculation.
An agent's own output cannot establish its correctness. Include at least one
changed-input check that would expose hardcoded answers. Compare correctness,
elapsed time, model cost, and human interventions; keep conclusions scoped
to the selected tasks and conditions.

A benchmark establishes behavior on a focused case. Whole-study integration
adds evidence about the complete dependency graph and delivery package.
Demonstrate a clean rebuild, an upstream change, an expected failure, and the
recovery path. For an ODM to SDTM to ADaM example, also check that define.xml
describes the delivered datasets. Agreement between R and Python can test
host consistency, but both use the shared engine and need independent
expected data as evidence of clinical correctness.

## Proposed three hour agenda

Course duration is a working assumption pending confirmation.

| Minutes | Session | Result |
| --- | --- | --- |
| 15 | Opening problem and environment check | A baseline run and a list of unstated requirements. |
| 20 | Inspect the agent | A shared model of its components and authority. |
| 35 | Build the AI first workflow | A reviewed specification and recorded clinical decisions. |
| 10 | Break | |
| 30 | Create and challenge a benchmark | An independently authored case and a failure the original case missed. |
| 30 | Integrate the study | A dependency-aware run and an explanation of an upstream change. |
| 20 | Prepare for deployment | A release outline covering versions, credentials, audit evidence, failures, and rollback. |
| 20 | Human review and discussion | An evidence-based acceptance decision and a workflow participants can adapt. |

For a half day, expand benchmark design and let pairs exchange review roles.
For a full day, add a second domain and have participants package and rerun
their workflow from a clean environment.

## Codespaces preparation

Prepare a course repository and devcontainer with pinned tools, a tested
yamaa package, one primary language, and a small synthetic dataset. Opening
the Codespace should lead to a health check and baseline run before any
model calls. Keep the exercise focused on workflow design by installing the
runtime in advance.

Verify Muse Spark and Muse access using the exact course account setup.
Plan credential delivery through Codespaces secrets, model usage limits,
concurrency for the class, and Codespaces compute availability. Supply a
saved agent session and completed exercise checkpoints for participants who
lose access during the lab.

Treat Codespaces as the learning environment. Discuss separately how an
approved workflow runs in a production environment: data boundaries,
restricted model access, scoped credentials, persistent run records,
versioned artifacts, monitoring, and recovery. An approved specification can
run through yamaa without invoking the authoring agent on every build.

## Decisions for the next iteration

- Confirm the teaching duration, audience experience, and primary language.
- Identify Muse precisely and decide whether model comparison is a learning
  objective or simply an optional extension.
- Rehearse the partial-date toy fixture and the CDISC Pilot 01 study review.
  Select course revisions and install each study's pinned environment.
- Decide whether participants bring provider accounts or use course access.
- Agree on the minimum completed artifact each participant should leave with.

## Repository sources and course preparation

- [yamaa principles](../../articles/principles.md)
- [Partial date benchmark](https://github.com/elong0527/yamaa/tree/main/benchmarks/adam-adae-partial-dates)
- [Agent evaluation and independent grading](https://github.com/elong0527/yamaa/blob/main/evaluations/harbor/README.md)
- [Earlier OpenCode and Muse Spark example](../yamaa-njsug-2026-10-09/index.qmd)
- [Pilot 7 study catalog](https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/.github/yamaa-studies.json)
- [Pilot 01 project](https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/cdiscpilot01/spec/yamaa/project.yaml)
- [Pilot 01 source review question](https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/cdiscpilot01/spec/review/questions.json)
- [Study validation policy](https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/docs/verification.md)
- [Review workspace guide](https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/docs/review.md)
- [Writing specifications](https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/docs/yaml-authoring.md)
- [Pilot 3 Define-XML reproduction](https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/submission-pilot3/program/adam/DEFINE.md)
- [Study data licensing and attribution](https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/LICENSING.md)

Presenter notes name the source and preparation work for each section.
Complete the lab instructions, independent toy expectations, dashboard and
report examples, and saved agent sessions after rehearsal. Codespaces
configuration, model provisioning, and executable exercise scaffolding
remain future course work. The skeleton does not bundle study data or
claim that the proposed labs have been run.
