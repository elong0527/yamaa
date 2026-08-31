# Portable function candidate inventory

This inventory records evidence and promotion decisions. The normative
threshold and boundaries are R017; this file applies them to current examples.

## Numeric and mathematical

| Candidate | Evidence | Decision |
|---|---|---|
| Existing R010 scalar set | BMI, dose, duration, and change examples | Core |
| `NORMAL_CDF` | ADVS growth percentile plus the published LMS pattern | Core |
| `ROUND` | Reporting display only; cross-runtime tie modes differ | Reject |
| `LOG` | Runtime conventions disagree on the implicit base | Reject |

`NORMAL_CDF` closes the one missing deterministic scalar step in
`adam-advs-growth-percentile`. The LMS arithmetic already composes from core
operators. Its central and tail fixtures close the accuracy contract.

## Statistical and probability reducers

| Candidate | Evidence | Decision |
|---|---|---|
| Existing reducers | Repeated grouped ADaM and SDTM examples | Core |
| `MEAN` | `adam-adlb-mean` and the arithmetic definition in R013 | Core |
| `MEDIAN` | No second example; interpolation is not yet fixed | Defer |

`MEAN` stays a reducer rather than a scalar because it changes grain.
`MEDIAN` needs both repeated evidence and a declared interpolation rule before
it can enter core or an extension pack.

## Missing-value and variadic selection

| Candidate | Evidence | Decision |
|---|---|---|
| `COALESCE` | Fallback formulas and the existing row-wise expression | Core |
| Numeric extremes | Formulas plus row-wise extreme examples | Core |

These names are scalar only inside R010's numeric grammar. The typed
`coalesce`, `greatest`, and `least` expressions remain the portable forms for
non-numeric values.

## String and temporal

The current examples use typed expressions such as `str_upper`, `date_diff`,
and `study_day`. No repeated case requires a string or temporal function inside
the numeric grammar. Adding duplicate callable spellings would weaken the
typed expression boundary, so there is no candidate to promote now.

## Deliberately project-specific

| Candidate | Evidence | Decision |
|---|---|---|
| `bmi` | `adam-adsl-bmi-function` | Keep behind `function` |
| `growth_percentile` | Earlier ADVS project routine | Do not register |
| Sponsor grading | Study-specific medical policy | Keep behind `function` |

`bmi` is already expressible with portable arithmetic, so registering a
project helper would add a duplicate spelling rather than a capability.
`growth_percentile` combines reference selection, LMS arithmetic, and a CDF;
the example now composes those portable parts instead of standardizing one
project routine. Sponsor grading remains policy whose inputs and behavior vary
by study and cannot be closed as a generally portable scalar contract.
