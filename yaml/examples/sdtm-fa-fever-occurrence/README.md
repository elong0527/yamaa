# SDTM FA: fever occurrence

This example uses vital signs data and a specification to derive one fever
occurrence record for each reactogenicity temperature record:

- `FASEQ` is the collected vital signs sequence number, retaining the original
  row identity.
- `FATESTCD` and `FATEST` represent the occurrence indicator.
- `FACAT` and `FASCAT` represent reactogenicity and systemic event
  respectively.
- `FAOBJ` is the observed event, which is fever.
- `FAORRES` and `FASTRESC` are the occurrence status, which is `Y` when the
  collected Celsius temperature is 38 or higher, `N` when it is lower, and
  empty when the temperature was not collected.
- `FAREASND` is the reason the measurement was not done.
- `FAEVAL` is the evaluator.
- `FARFTDTC` is the date and time of reference time point.
- `FAEVLINT` is the evaluation interval.
- `FAEVINTX` is the evaluation interval text.
- `FADTC` is the date and time of collection.
- `FADY` is the study day of collection.
- `FATPTREF` is the time point reference.
- `FATPTNUM` is the time point number.
- `FALNKID` is the link identifier.
- `FALNKGRP` is the link group identifier.
- `FATPT` is the planned time point name.
- `VSSTRESN` is the collected numeric temperature in standard units.
