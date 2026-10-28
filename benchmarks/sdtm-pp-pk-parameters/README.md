# Publish parameters from a pharmacokinetic analysis transfer

**Goal:** turn noncompartmental analysis estimates into pharmacokinetic
parameter (PP) records, with standard parameter codes and names.

**Input:** a noncompartmental analysis software export with subject,
analyte, software parameter name, estimate, unit and reference dose time.
The estimates have already been calculated from plasma concentrations.

**Variables:**

- `PPSEQ` numbers parameters within each subject by reference dose time,
  analyte and standard parameter code.
- `PPTESTCD` and `PPTEST` identify the software's `Cmax`, `Tmax` and
  `AUClast` as `CMAX` (Max Conc), `TMAX` (Time of CMAX Observation) and
  `AUCLST` (AUC to Last Nonzero Conc).
- `PPCAT` carries the analyte name.
- `PPORRES` and `PPSTRESC` retain the estimate as text, including its
  reported decimal places; `PPSTRESN` holds the same value as a number.
- `PPORRESU` and `PPSTRESU` retain the analysis units: `ng/mL` for maximum
  concentration, `h` for its time and `h*ng/mL` for the area under the
  concentration-time curve (AUC).
- `PPSPEC` is `PLASMA`. `PPRFTDTC` identifies the reference dose date and
  time supplied by the analysis software.

**Note:** this step publishes the supplied estimates; it does not
recalculate them from a concentration profile or change their units.
