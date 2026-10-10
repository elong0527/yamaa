/* opensas reference solution for the yamaa benchmark adam-adsl-completion-flag.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl_raw;
  length STUDYID USUBJID TRTSDT $1024;
  infile "/app/input/adsl_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDT : $char1024.;
run;

data ds;
  length STUDYID USUBJID DSCAT DSDECOD DSDTC EPOCH $1024;
  infile "/app/input/ds.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DSSEQ DSCAT : $char1024. DSDECOD : $char1024. DSDTC : $char1024. EPOCH : $char1024.;
run;

proc sql;
create table completed as select distinct STUDYID,USUBJID from ds where EPOCH='FOLLOW-UP' and DSDECOD='COMPLETED';
create table result as select a.*,case when not missing(b.USUBJID) then 'Y' else 'N' end as COMPLFL from adsl_raw a left join completed b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
proc sql;
  create table final as select STUDYID, USUBJID, TRTSDT, COMPLFL from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
