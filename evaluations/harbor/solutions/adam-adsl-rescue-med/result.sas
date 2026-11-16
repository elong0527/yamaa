/* opensas reference solution for the yamaa benchmark adam-adsl-rescue-med.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data cm;
  length STUDYID USUBJID CMTRT CMDECOD CMCAT CMSTDTC $1024;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CMSEQ CMTRT : $char1024. CMDECOD : $char1024. CMCAT : $char1024. CMSTDTC : $char1024.;
run;

data dm;
  length STUDYID USUBJID $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024.;
run;

/* A partial start completes to its first possible day: a year and month
   to the first of the month (flag D), a year alone to January 1 (flag M). */
data rescue;
  length RESCDT $10 RESCDTF $1;
  set cm;
  if CMCAT='RESCUE MEDICATION';
  RESCDT=''; RESCDTF='';
  if lengthn(CMSTDTC)=4 then do; RESCDT=cats(CMSTDTC,'-01-01'); RESCDTF='M'; end;
  else if lengthn(CMSTDTC)=7 then do; RESCDT=cats(CMSTDTC,'-01'); RESCDTF='D'; end;
  else RESCDT=CMSTDTC;
  nodate=missing(RESCDT);
run;
proc sort data=rescue; by STUDYID USUBJID nodate RESCDT CMSEQ; run;
data firstcm; set rescue; by STUDYID USUBJID; if first.USUBJID; run;
proc sql;
  create table result as
    select a.STUDYID, a.USUBJID,
           case when b.USUBJID is null then 'N' else 'Y' end as RESCFL length=1,
           b.RESCDT, b.RESCDTF, b.CMDECOD as RESCTRT
    from dm as a left join firstcm as b
      on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;

proc export data=result outfile="/app/output/adsl.csv" dbms=csv replace; run;
