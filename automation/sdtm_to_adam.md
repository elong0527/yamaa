# Assess an SDTM-to-ADaM study with the yamaa R package

Help a statistical programmer assess yamaa on an organizational study,
starting with installation of the **yamaa R package 0.5.0**. The goal is to
generate clear, maintainable yamaa specifications that follow current best
practice and fully reproduce the intended SDTM-to-ADaM derivation logic.
Use R for the assessment and choose an appropriate study-specific workflow.

## Confirm readiness

Install and verify the released R package using its
[installation guidance](https://github.com/elong0527/yamaa/blob/yamaa-v0.5.0/R/yamaa/README.md#installation).
Record the environment and assess the installation experience.

Confirm the availability and relevance of SDTM inputs, reference ADaM outputs,
existing specifications, SAS or R programs and their dependencies, and
supporting study documentation. Identify missing resources, incompatible
versions and conflicting evidence. Agree the target datasets and preserve
all targets in the assessment, including those that are blocked or untested.

## Generate and assess the specifications

Use current yamaa best practices supported by R package 0.5.0. Build
reviewable specifications from the available study evidence and use R to
execute them. Preserve original source evidence and reference datasets.

Investigate disagreements among specifications, code and input/output data.
Document their impact, the proposed interpretation and anything requiring
study-owner review. Distinguish source problems from specification mistakes
and possible yamaa limitations.

## Validate the results independently

Compare every target dataset and cell with its reference, including structure,
metadata, record identity and any required order. Report exact matches
separately from differences accepted under a stated numeric tolerance.

Review derivation rules and relevant edge cases as well as observed outputs.
A complete data match alone does not prove complete logic equivalence.
Keep unresolved source conflicts, untested rules and missing evidence visible
in the conclusion; do not claim 100% equivalence while these affect it.

## Report outcomes and effort

Provide an overall summary and per-dataset results covering:

- Fully matching datasets out of all targets, with mismatched, blocked and
  untested datasets identified. A full match includes data and required
  metadata, with zero validation issues.
- Exactly matching cells out of all reference cells, with missing or extra
  data and tolerance-only differences reported separately. Keep blocked
  targets in the totals where their reference size is known; identify any
  incomplete denominator.
- End-to-end time, including installation, investigation and rework, with
  execution and comparison times distinguished where measurable.
- Token usage and money spent for the whole job, including unsuccessful
  attempts. Distinguish measured usage, billed costs and estimates; mark
  unavailable information explicitly rather than reporting it as zero.

Explain resource readiness, source discrepancies, derivation coverage,
remaining limitations and the evidence supporting each conclusion.

## Prepare useful product feedback

Identify evidence-based improvements to the R user interface, specification
schema, engine and supporting documentation. Distinguish confirmed defects,
missing capabilities and usability suggestions.

Prepare GitHub-ready feedback with enough public or independently synthetic
evidence to explain and reproduce each finding. Exclude confidential study
data, proprietary logic, identifiers, internal paths and other internal
details. Keep organizational evidence separate from feedback intended for
public sharing.

## Deliver for human review

Organize generated specifications, R programs, datasets, reports and logs
according to the
[Pilot 7 folder structure](https://github.com/RConsortium/submissions-pilot7-synthetic-data#folder-structure).
Provide a reproducible R workflow and enough evidence for a second programmer
to review the logic, results and unresolved findings.

Iterate until the work is ready for human review. State clearly whether the
assessment is complete, partial or blocked, and what remains before the study
can claim full derivation equivalence.
