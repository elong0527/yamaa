# Carry-Over Exclusion of a Crossover Period

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adpp-carryover-exclusion.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each pharmacokinetic (PK) parameter of a two-period
bioequivalence crossover into ADPP with the period's pre-dose
concentration (`PRECONC`), that concentration as a percentage of the
period's maximum concentration (`PREPCMAX`), and the analysis flag
(`ANL01FL`) that leaves a period with carry-over out of the
bioequivalence analysis.

**Input:** PK parameters (PP) with their numeric result (`PPSTRESN`)
and the reference dose time of their period (`PPRFTDTC`), and PK
concentrations (PC) with planned time point (`PCTPT`), standard result
(`PCSTRESC`, `PCSTRESN`), status and the reference dose time
(`PCRFTDTC`).

**Variables:**

- `VISITNUM` and `VISIT` name the dosing period the parameter belongs
  to, as PP gives them.
- `AVAL` is the parameter's numeric result in standard units.
- `PRECONC` is the concentration of the period's pre-dose sample: the
  PC record planned at `PREDOSE` whose reference dose time is the
  parameter's. A result below the lower limit of quantitation counts
  as 0. Empty when the period has no pre-dose sample, or the sample was
  not done; no other sample stands in for it.
- `PREPCMAX` is `PRECONC` as a percentage of the same subject's
  maximum concentration (Cmax) in the same period, not rounded; empty
  when `PRECONC` is.
- `ANL01FL` is empty on every parameter of a period whose `PREPCMAX`
  is greater than 5, and `Y` on every other record, including a period
  whose `PREPCMAX` is empty. Exactly 5 percent stays in.

**Note:** every period is checked, the first one included, and the
period is the unit left out: the subject's other period keeps `Y`, and
every record stays in the dataset. The pre-dose result, the percentage
and the flag repeat on every parameter of the period.

**Standard:** ADaM | **Domain:** ADPP
