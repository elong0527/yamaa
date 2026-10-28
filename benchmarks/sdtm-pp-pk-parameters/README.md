# Publish parameters from a pharmacokinetic analysis transfer

**Goal:** turn noncompartmental analysis estimates into pharmacokinetic
parameter (PP) records, with standard parameter codes and names.

**Input:** a noncompartmental analysis software export with subject,
analyte, software parameter name, estimate, unit and reference dose time.
The estimates have already been calculated from plasma concentrations.

**Variables:**

- `PPSEQ` numbers parameters within each subject by reference dose time,
  analyte and standard parameter code.
- `PPTESTCD` identifies the software's `Cmax`, `Tmax` and `AUClast` as
  `CMAX`, `TMAX` and `AUCLST`; it is missing when the parameter is absent.
- `PPTEST` names those parameters as Max Conc, Time of CMAX Observation and
  AUC to Last Nonzero Conc; it is missing when the parameter code is absent.
- `PPCAT` carries the analyte name, or is missing when none is supplied.
- `PPORRES` retains the estimate as text, including its reported decimal
  places; it is missing when the estimate is absent.
- `PPORRESU` retains the analysis unit supplied for that estimate, without
  assigning a unit from the parameter code. It is missing when the unit
  is absent, independently of whether an estimate is supplied.
- `PPSTRESC` retains the estimate as text with its reported decimal places;
  it is missing when the estimate is absent, regardless of the unit.
- `PPSTRESN` holds the estimate as a number; it is missing when the estimate
  is absent, regardless of the unit.
- `PPSTRESU` retains the supplied analysis unit without conversion; it is
  missing when the unit is absent, regardless of the estimate.
- `PPSPEC` is always `PLASMA`.
- `PPRFTDTC` identifies the reference dose date and time supplied by the
  analysis software, or is missing when none is supplied.

**Note:** this step publishes the supplied estimates; it does not
recalculate them from a concentration profile or change their units. An
unsupported parameter name or an estimate that is not numeric stops
publication.
