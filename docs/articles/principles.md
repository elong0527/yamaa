# Principles

Two AI-agent sessions read the same prompt to generate SDTM / ADaM datasets.
Both are sure they understand it. Each writes the code from scratch. The two output datasets disagree.

This disagreement is usually not a misreading.
The agents simply follow different paths through the gaps in the prompt.

yamaa moves agreement to the planning stage. People and AI agents build the
specification together. Questions get answered while they are still
cheap to answer. yamaa is a domain-specific language whose specifications
are built to have exactly one execution across different programming
languages. It covers clinical data work from Electronic Data Capture (EDC)
extraction through Study Data Tabulation Model (SDTM) and Analysis Data Model
(ADaM) to define.xml.

## The core principle

> **A yamaa specification has exactly one execution. Where it would have two,
> yamaa fails instead of choosing.**

Think of execution as a function:

*`output_dataset = derive(input_datasets, spec)`*

Datasets go in and out. *derive* is the derivation engine.

The whole design of yamaa is a contract between people and AI agents. The
input datasets are given. The contract covers two things:

- **spec**: the closed vocabulary. It says what you can write, and nothing
  more. There is no free text field for derivations. There is no "you know
  what I mean."
- **derive**: the derivation semantics. It says what each written item means.
  Numbered rules fix the meaning, so R, Python and the command line cannot
  read the same spec in two ways.

## Built for AI agents

A person who finds something unclear can walk over and ask a colleague. A live
AI coding session can ask for clarification. But an AI agent working on its own has
only the documents in front of it. There is no colleague to ask.

So for people, one execution prevents arguments. For AI, it does a bigger
job. The yamaa language forces every question to be answered during planning,
because the yamaa specification should be the one place where people agree on what to build.

This is also why the vocabulary is closed. The typical AI failure is confident
invention: text that reads well but runs wrong, and then needs heavy review.
Talking with AI can cost as much as talking with another person, or more. A closed
vocabulary limits what the AI can write. Every line is either valid or caught
by validation. The files are plain text, and each names its language version
and its type. `lint` keeps one layout with the comments intact, and `check`
names each problem's condition, rule and spec path without reading data, so
an agent can work from the findings. Working with an AI agent to complete
CDISC data standardization becomes an iterative way to review generated
datasets and improve the specification with minimal code changes.

## Principles

These four principles follow from the core principle of one execution:

- **Inheritance**: Each specification names at most one parent, and the chain
  combines in a fixed order, from its root ancestor to the entry, into one final
  specification; each layer adds the fragments it lists under `include` just
  before itself, and a cycle or a file reached twice fails instead of
  resolving. Every layer is a `spec` file, and a layer removes an inherited
  value only by naming it in `clear`. A company standard is shared, not copied
  into files that then change separately.
- **Language neutral**: The same specification with the same inputs generates the
  same output dataset, to the byte, in R, in Python and from the command line,
  on every supported operating system. No host has its own semantics, global
  options or environment variables. A specification that calls a project
  function fails before reading data when the running host has no
  implementation of it, rather than approximating the result; the command line
  runs no host language, so it refuses such a specification.
- **Explicit**: Nothing reaches the output unless the specification put it
  there. Each declared column is derived in exactly one place. Nothing is
  inferred either: every file names its language version and type, a join
  names its keys, and text in an expression is quoted, so a bare word there is
  always a reference, never text. What a specification needs from its project,
  it names: checked or run without its project, a specification that binds a
  codelist or calls a project function fails before reading data.
- **Extension**: A specification holds no code from R or Python. It has one
  extension point: a named contract. A project function is declared once, in a
  function file, with its parameters, return type and tests, and the callable
  that implements it in each host language. A specification calls it by its
  logical name, and the host that runs the study runs the same tests before it
  reads any data.

Following the principles, the goal is to move most of the AI agents' work into
building the yamaa specification with people, where unclear points are cheap
to fix.

## Cost and benefit

Using yamaa means accepting a contract between people and AI agents. The cost
is real, and it is paid early. Every unclear point above is answered while the
specification is being written, by the people who can answer it, at the
planning stage. The goal: no open questions left for the coding stage.

That is the trade. Unclear points are fixed once, early, by people. The
alternative is to fix them many times, late, by machines that guess in
different ways.

## Principles to working model

These principles are not rules by themselves. Each one is enforced by the
[Rules](rules.md) and demonstrated in the
[benchmark](../benchmark/index.md) with minimal, visible benchmarks.
