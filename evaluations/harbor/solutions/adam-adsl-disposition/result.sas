/* opensas reference solution for the yamaa benchmark adam-adsl-disposition.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID RFXSTDTC $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RFXSTDTC : $char1024.;
run;

data ds;
  length STUDYID USUBJID DSCAT EPOCH DSDECOD DSTERM DSSTDTC $1024;
  infile "/app/input/ds.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DSSEQ DSCAT : $char1024. EPOCH : $char1024. DSDECOD : $char1024. DSTERM : $char1024. DSSTDTC : $char1024.;
run;

/* A blank DSSTDTC sorts last in descending order, so an undated record is
   the last record only when the subject has no dated one. */
proc sort data=ds(where=(DSCAT='DISPOSITION EVENT' and EPOCH ne 'TREATMENT')) out=ordered_study;
  by STUDYID USUBJID descending DSSTDTC descending DSSEQ;
run;
data lastds; set ordered_study; by STUDYID USUBJID; if first.USUBJID; run;

proc sort data=ds(where=(DSCAT='DISPOSITION EVENT' and EPOCH='TREATMENT')) out=ordered_trt;
  by STUDYID USUBJID descending DSSTDTC descending DSSEQ;
run;
data lasttrt; set ordered_trt; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID DSSTDTC DSDECOD DSTERM; run;

proc sql;
  create table joined as
    select a.STUDYID, a.USUBJID, a.RFXSTDTC, b.DSSTDTC as EOSDT, b.DSDECOD as EOSDECOD, b.DSTERM as EOSREAS,
           c.DSSTDTC as EOTDT, c.DSDECOD as EOTDECOD, c.DSTERM as EOTREAS
    from dm as a
    left join lastds as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID
    left join lasttrt as c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID;
quit;
data result;
  length EOSSTT $12 DCSREAS $1024 EOTSTT $12 DCTREAS $1024 DCTREASP $1024;
  set joined;
  /* A screen failure never entered the study, so it has no end date,
     reason or status. */
  if EOSDECOD='SCREEN FAILURE' then do; EOSDT=''; EOSREAS=''; EOSSTT=''; end;
  else if missing(EOSDECOD) then EOSSTT='ONGOING';
  else if EOSDECOD='COMPLETED' then EOSSTT='COMPLETED';
  else EOSSTT='DISCONTINUED';
  if EOSSTT='DISCONTINUED' then DCSREAS=EOSDECOD;
  /* A subject never treated has no treatment status. */
  if missing(RFXSTDTC) then EOTSTT='';
  else if missing(EOTDECOD) then EOTSTT='ONGOING';
  else if EOTDECOD='COMPLETED' then EOTSTT='COMPLETED';
  else EOTSTT='DISCONTINUED';
  if EOTSTT='DISCONTINUED' then DCTREAS=EOTDECOD;
  if DCTREAS='OTHER' then DCTREASP=EOTREAS;
run;

proc sql;
  create table final as select STUDYID, USUBJID, EOSDT, EOSREAS, EOSSTT, DCSREAS, EOTDT, EOTSTT, DCTREAS, DCTREASP from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
