/* opensas reference solution for the yamaa benchmark adam-adis-log-titer.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data is;
  length STUDYID USUBJID ISTESTCD $40 ISTEST $60 VISIT $40;
  infile "/app/input/is.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. USUBJID : $char40. ISTESTCD : $char40.
        ISTEST : $char60. VISITNUM VISIT : $char40. ISSTRESN ISLLOQ;
run;

/* A titer below the lower limit of quantitation gives no record; a record
   whose titer has no value stays. */
data adis;
  set is;
  where missing(ISSTRESN) or ISSTRESN >= ISLLOQ;
  length PARAMCD $8 AVISIT $40;
  PARAMCD = 'NABLOG10';
  AVISIT = VISIT;
  if not missing(ISSTRESN) then AVAL = log10(ISSTRESN);
  keep STUDYID USUBJID PARAMCD AVISIT AVAL;
run;

proc sql;
  create table final as
    select STUDYID, USUBJID, PARAMCD, AVISIT, AVAL from adis;
quit;
proc export data=final outfile="/app/output/adis.csv" dbms=csv replace; run;
