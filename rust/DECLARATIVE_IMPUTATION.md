# Declarative date completion and precision

The shared compiler lowers `date_impute`, `date_precision`, `datetime_impute`,
`datetime_precision` and `to_date` into owned core declarations. The engine reads
bound operands in the ordinary column, selected case and row-template scopes.
Known type and calendar-policy failures precede study reads.

Partial-date completion preserves collected year/month precision. Complete dates,
sources below the declared minimum precision and recovered missing/invalid values
skip `not_before`. A required bound is read and type-checked before the completed
calendar is validated. Bound failure keeps the original opaque error and stops
later reads. Missing and invalid handlers distinguish absent handlers from
explicit null, count each recovery and undergo one output conversion.

Metadata, work, source text, diagnostic context and retained row keys have separate
admission boundaries. The pure scalar evaluator requires caller-admitted text;
the engine charges it before parsing or copying. Constructor and calendar-policy
limits refuse separately from semantic findings.

Two unchanged original positives (`schema-birth-date`, `sdtm-dm-age`) and ten
original negatives supply public installed-host witnesses. Six negatives specify
complete diagnostic truth. Four specify required context fields and also match
independently captured complete reference findings, including `operation` and
row keys. The original conformance contract permits additional context fields;
all original expected artifacts remain unchanged.

This slice does not qualify all temporal pipelines. Source-schema, secondary
relation and broader expression-family gates remain visible. The original row
compiler still refuses whole-column cases; direct row operations and selected
implicit-key cases have independent coverage. Full language and release gates
remain under #1751, #1753, #1742 and #1585.
