---
title: Principles
---

# Principles

> **YAMAA docs:** [Principles](principles.md) | [Why](why-yamaa.md) | [Excel to YAMAA](excel-to-yamaa.md) | [Schema concepts](schema-concepts.md) | [Examples walkthrough](yaml-examples-walkthrough.md)

> **Read this if** you want the shortest account of what YAMAA is for, before
> the argument in [Why YAMAA](why-yamaa.md) or the syntax in
> [Schema concepts](schema-concepts.md).

---

Two programmers read the same Excel specification, both are certain they
understood it, and their programs disagree. The disagreement is almost never a
misreading -- it is that the specification never reached the point they
disagreed on. Everything below follows from taking that seriously.

## The principle

> **A YAMAA specification has exactly one execution. Where it would have two,
> YAMAA fails instead of choosing.**

The second sentence is what makes the first one true. Any format can claim to
be reproducible; what distinguishes YAMAA is that when a specification runs
out, it stops rather than filling the gap with a default that happens to be
defensible.

## What follows from it

**1. Where a specification would admit two answers, YAMAA raises an error
instead of choosing one.**
A join that matches twice and a value the codelist does not contain both stop
the run until the specification says which answer it wants. (R001, R003, R008)

**2. Nothing reaches the output unless a specification put it there.**
No variable is picked up because it happens to share a name, every declared
column is derived in exactly one place, and a deliberate blank is written as
`literal: null`. (R002, R005)

**3. Sameness is defined down to the bytes.**
The same specification over the same inputs produces the same file, on any
machine, whether the R or the Python implementation ran it. (R019, R020, R021,
R026)

**4. The derivation vocabulary is closed, so an unsupported derivation fails
validation instead of looking plausible.**
There is no free-text derivation column to fill with something that reads well
and runs differently, which is also what makes a generated specification
reviewable. (R004, R007, R011)

**5. A specification carries no host-language code, and its one extension point
is a named contract.**
A project function is declared by contract where it is used and implemented
once, separately, in the project's single runtime language. (R018)

**6. Every question about meaning has exactly one rule that owns it, and that
rule can be cited by number.**
A disagreement in review ends with a citation such as R013-7 rather than with
whoever has been doing this longest.

**7. What YAMAA refuses to do is specified as precisely as what it does.**
More than half the examples are negative, and each one fixes the exact error,
so a refusal is a documented behavior rather than something the first person to
attempt it discovers.

**8. A standard is inherited as layers, not copied as a template.**
Organization, compound and study layers compose in a fixed order into one
resolved specification, so a corporate convention is versioned and distributed
rather than copied and left to drift. (R017)

## What this costs you

YAMAA has no `ROUND`. SAS `round()` is half-up and R `round()` defaults to
banker's rounding, so a specification that says "rounded to 1 decimal" has
already chosen one of them without saying which -- YAMAA declines to choose,
and rounding stays a reporting decision. Most of what YAMAA leaves out is left
out for that reason;
[section 3 of Why YAMAA](why-yamaa.md#3-what-an-excel-spec-has-that-yamaa-deliberately-does-not)
lists the rest with the rule that records each decision.

The cost is real, and it is paid early. Every question above is settled while
the specification is being written, by the people who can settle it, instead of
during validation or after a database lock.

## Where these live

The principles are not themselves normative. Each one is enforced by rules in
[`yaml/rules/`](https://github.com/elong0527/yamaa/tree/main/yaml/rules), where
the numbered requirements are authoritative and this page is not.
