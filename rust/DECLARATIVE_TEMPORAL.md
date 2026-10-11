# Complete-date declarations

Original `date_diff`, `study_day` and `to_epoch_day` declarations now compile to
owned core operations and ordered dataset operands. They use the same source,
output-column, intermediate and row binding services as other scalar declarations.
No formula substitution or host temporal evaluator is involved.

Calendar arithmetic uses admitted complete civil dates. Differences preserve
whole days and weeks, clamped month anniversaries and whole years, reversed
operand signs and authored day endpoint rules. Study days have no day zero;
epoch days count from 1970-01-01. Missing operands yield missing results after
later operands have been evaluated and checked in authored order. Output
conversion follows the complete operation once.

Known incompatible output declarations and authored CSV field types produce
REQ-0606 before study authority in column declarations. Direct row declarations
retain post-capture binding: missing input paths win, then complete declared-type
findings are produced before study-cell evaluation. Undeclared CSV fields use the
existing string default. Unknown resource schemas stay with binding and runtime; preparation
does not inspect a file to discover a field type. Non-day endpoint conflicts
retain REQ-0613. Row-local bare references retain the existing availability and
scope checkpoint. Bare temporal row operands pull their local defaults and
transitive default dependencies into the template. Runtime source errors stop before later operands and retain
their original opaque payload, without a diagnostic retry or conversion handler.
Operation metadata has a complete context byte ceiling before projection.

`adam-adsl-duration` and `adam-adsl-analysis-age` retain their original YAML and
complete independent output CSV files. Native and installed public-host witnesses
compare every output field and explicit saved byte. The four original negative
documents for datetime endpoints, units, study-day datetime and undeclared epoch
input retain their complete error fixtures and reject with study files removed.
Additional literal tests cover leap anniversaries, the full civil-date range,
study-day signs, missing values, nested cases and row templates. Caller-thread
errors requiring neither Clone nor Debug pin operand order, identity and recovery
precedence. Twelve additional independent complete captures pin row defaults,
chained numeric defaults, all three row date operand findings and input-missing
double faults. A separate bare-source row default still retains its explicit
row_output_reference gate.

This slice advances #1751 and #1753. Partial-date imputation, precision and
conversion services, intermediate-key modes, source-schema declarations and
remaining language families retain their existing explicit support gates. The
broader benchmark and migration acceptance lists remain open.
