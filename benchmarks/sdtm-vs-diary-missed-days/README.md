# Missed Temperature Diary Days

**Goal:** derive `VSSEQ`, `VSTESTCD`, `VSTEST`, `VSORRES`, `VSORRESU`,
`VSSTAT`, `VSREASND`, `VSDTC`, `VSTPT`, `VSTPTNUM`, `VSTPTREF` and
`VSRFTDTC` for every day of the temperature diary after each vaccination in
Vital Signs (VS), including the days no temperature was entered.

**Input:** the vaccination log, one record per planned vaccination with its
date when given; the protocol's diary period, one record per diary day with
its label; and the e-diary transfer, one record per temperature entered, with
its date and unit.

**Variables:**

- `VSSEQ` numbers the subject's records by vaccination date, then diary day.
- `VSTESTCD` is always `TEMP`.
- `VSTEST` is always `Temperature`.
- `VSORRES` is the temperature entered on the diary date, empty on a day with
  no entry.
- `VSORRESU` is the unit of the temperature entered, empty on a day with no
  entry.
- `VSSTAT` is `NOT DONE` on a diary day with no temperature entered, and
  empty otherwise.
- `VSREASND` is `NOT ENTERED` on a day marked `NOT DONE`, and empty
  otherwise.
- `VSDTC` is the diary date: the vaccination date on Day 1, one day later
  for each later diary day.
- `VSTPT` is the diary day's label (`DAY 1`, ...).
- `VSTPTNUM` is the diary day's number, 1 on the day of vaccination.
- `VSTPTREF` names the vaccination the diary day follows.
- `VSRFTDTC` is that vaccination's date.

**Note:** every vaccination that was given expects one record for each day of
the diary period, whether or not the e-diary holds a temperature for it. A
vaccination with no date was not given and expects no diary records.
