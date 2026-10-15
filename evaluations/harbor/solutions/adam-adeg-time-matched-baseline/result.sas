/* opensas reference solution for the yamaa benchmark adam-adeg-time-matched-baseline.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adeg;
  length STUDYID USUBJID PARAMCD PARAM ATPT AVALU $1024;
  infile "/app/input/adeg_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PARAMCD : $char1024. PARAM : $char1024. APERIOD APERDAY ATPT : $char1024. ATPTN AVAL AVALU : $char1024.;
run;

/* Day -1 of each period is that period's baseline day; its records are the
   time-matched baselines, one per subject, parameter, period and time. */
data baseline;
  length B_STUDYID B_USUBJID B_PARAMCD $1024;
  set adeg;
  if APERDAY = -1;
  B_STUDYID = STUDYID; B_USUBJID = USUBJID; B_PARAMCD = PARAMCD;
  B_APERIOD = APERIOD; B_ATPTN = ATPTN; BASE = AVAL;
  keep B_STUDYID B_USUBJID B_PARAMCD B_APERIOD B_ATPTN BASE;
run;

proc sql;
  create table matched as
    select a.*, b.BASE
    from adeg as a left join baseline as b
      on a.STUDYID = b.B_STUDYID and a.USUBJID = b.B_USUBJID
      and a.PARAMCD = b.B_PARAMCD and a.APERIOD = b.B_APERIOD
      and a.ATPTN = b.B_ATPTN;
quit;

/* No change on the baseline day, and none without a baseline. */
data final;
  length ABLFL $1 BASETYPE $1024;
  set matched;
  if APERDAY = -1 then ABLFL = 'Y';
  else ABLFL = '';
  if APERDAY ne -1 and not missing(BASE) and not missing(AVAL) then CHG = AVAL - BASE;
  else CHG = .;
  BASETYPE = 'Period ' || strip(put(APERIOD, 8.)) || ' Baseline Day ' || strip(ATPT);
run;

proc sql;
  create table out as
    select STUDYID, USUBJID, PARAMCD, PARAM, APERIOD, APERDAY, ATPT, ATPTN,
           AVAL, AVALU, ABLFL, BASE, CHG, BASETYPE
    from final;
quit;
proc export data=out outfile="/app/output/adeg.csv" dbms=csv replace; run;
