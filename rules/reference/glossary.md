# Glossary

This glossary pins one meaning per shared clinical term used across the
contracts. Each definition states what the term means and names the IG
source it follows. Contracts use these meanings.

## subject

A subject is an individual who participates in a clinical trial, either
as recipient of the investigational product(s) or as a control. In
regular-expression operations, "subject" names the input text being
matched, not the trial participant.
(CDISC Clinical Research Glossary v5.0 [ICH]; operations/text.md)

## record

A record is one row of a dataset. In yamaa contracts, "record" names an
input record: one row of a declared source dataset.
(ADaMIG 1.3, section 1.5.1)

## row

A row is a constructed output record: one output record produced by a
row template or another output-building step. The input-side term is
"record"; the output-side term is "row".
(execution/rows.md)

## observation

An observation is an assessment of a subject's condition collected
during a study: one discrete piece of collected information, for
example a measure used to assess an outcome. In SDTM, observations
group into domains. ADaMIG uses "observation" as a synonym for record.
(CDISC Glossary [SDTM]; SDTMIG 3.4 Appendix B; ADaMIG 1.3, section 1.5.1)

## variable

A variable is an attribute, phenomenon, characteristic, or event that
can take different qualitative or quantitative values. In a CDISC
dataset, a variable is one column of the dataset.
(CDISC Glossary; ADaMIG 1.3, section 1.5.1)

## column

A column is a declared, named value slot of a specification or
dataset. "Variable" is the CDISC term for a dataset column.

## dataset

A dataset is a collection of structured data in a single file.
(CDISC Glossary [CDISC]; SDTMIG 3.4 Appendix B)

## domain

A domain is a collection of logically related observations with a
common, specific topic, normally collected for all subjects in a
clinical investigation.
(CDISC Glossary [after SDTMIG 3.2]; SDTMIG 3.4 Appendix B)

## derivation

A derivation is the documented computation that produces an analysis
value from source data.
(ADaMIG 1.3; CDISC Glossary: derived variable)

## parameter

An analysis parameter is a row identifier that uniquely characterizes
a group of values sharing a common definition, for example "Sitting
Systolic Blood Pressure (mmHg)". "Parameter" is a synonym for
"analysis parameter".
(ADaMIG 1.3, section 1.5.2)

## baseline

A baseline is an assessment of a subject on trial entry, before any
treatment is received. Baseline values are the reference for
change-from-baseline computations.
(CDISC Glossary)

## visit

A visit is a clinical encounter encompassing planned and unplanned
trial interventions, procedures, and assessments performed on a
subject. A visit has a start and an end, each described with a rule.
(CDISC Glossary [CDISC Trial Design Project])
