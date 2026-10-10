/* SAS language reference solution for the yamaa benchmark adam-adsl-rescue-med.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data cm;
  length STUDYID USUBJID CMTRT CMCAT CMSTDTC $1024;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CMSEQ CMTRT : $char1024. CMCAT : $char1024. CMSTDTC : $char1024.;
run;

data dm;
  length STUDYID USUBJID $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024.;
run;

data rescue; set cm; if CMCAT='RESCUE MEDICATION'; nodate=missing(CMSTDTC); run;
proc sort data=rescue; by STUDYID USUBJID nodate CMSTDTC CMSEQ; run;
data firstcm; set rescue; by STUDYID USUBJID; if first.USUBJID; run;
proc sql; create table result as select a.*,b.CMTRT as RESCTRT from dm as a left join firstcm as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;

proc sql;
  create table final as select STUDYID, USUBJID, RESCTRT from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
