# Principles

Two AI-agent sessions read the same prompt to generate SDTM / ADaM datasets.
Both are sure they understand it. Each writes the code from scratch. The two output datasets disagree.

This disagreement is usually not a misreading.
The agents simply follow different paths through the gaps in the prompt.

YAMAA moves agreement to the planning stage. People and AI agents build the
specification together. Questions get answered while they are still
cheap to answer. A YAMAA specification is built to have exactly one execution
across different programming languages. It covers clinical data work from
Electronic Data Capture (EDC) extraction through Study Data Tabulation Model
(SDTM) and Analysis Data Model (ADaM) to define.xml.

## The core principle

> **A YAMAA specification has exactly one execution. Where it would have two,
> YAMAA fails instead of choosing.**

Think of execution as a function:

*`output_dataset = derive(input_datasets, spec)`*

Datasets go in and out. *derive* is the derivation engine.

The whole design of YAMAA is a contract between people and AI agents. The
input datasets are given. The contract covers two things:

- **spec**: the closed vocabulary. It says what you can write, and nothing
  more. There is no free text field for derivations. There is no "you know
  what I mean."
- **derive**: the derivation semantics. It says what each written item means.
  Numbered rules fix the meaning, so the R engine and the Python engine
  cannot read the same spec in two ways.

## Built for AI agents

A person who finds something unclear can walk over and ask a colleague. A live
AI coding session can ask for clarification. But an AI agent working on its own has
only the documents in front of it. There is no colleague to ask.

So for people, one execution prevents arguments. For AI, it does a bigger
job. The YAMAA language forces every question to be answered during planning,
because the YAMAA specification should be the one place where people agree on what to build.

This is also why the vocabulary is closed. The typical AI failure is confident
invention: text that reads well but runs wrong, and then needs heavy review.
Talking with AI can cost as much as talking with another person, or more. A closed
vocabulary limits what the AI can write. Every line is either valid or caught
by validation. Working with an AI agent to complete CDISC data standardization
becomes an iterative way to review generated datasets and improve the specification with minimal code changes.

## Principles

These four principles follow from the core principle of one execution:

- **Inheritance**: e.g. organization, compound, and study layers combine in a fixed
  order into one final specification. A company standard is shared,
  not copied into files that then change separately.
- **Language neutral**: The same specification with the same inputs generates the
  same output dataset in R and in Python.
- **Explicit**: Nothing reaches the output unless the specification put it
  there. Each declared column is derived in exactly one place.
- **Extension**: A specification holds no code from R or Python. It has one
  extension point: a named contract. A project function is declared by its
  contract where it is used, and written once, in one language for the
  project.

Following the principles, the goal is to move most of the AI agents' work into
building the YAMAA specification with people, where unclear points are cheap
to fix.

## Cost and benefit

Using YAMAA means accepting a contract between people and AI agents. The cost
is real, and it is paid early. Every unclear point above is answered while the
specification is being written, by the people who can answer it, at the
planning stage. The goal: no open questions left for the coding stage.

That is the trade. Unclear points are fixed once, early, by people. The
alternative is to fix them many times, late, by machines that guess in
different ways.

## Principles to working model

These principles are not rules by themselves. Each one is enforced by the
[Rules](../reference/rules.md) and demonstrated in the
[benchmark](../benchmark/index.md) with minimal, visible benchmarks.
