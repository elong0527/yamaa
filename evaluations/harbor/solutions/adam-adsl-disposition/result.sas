/* opensas reference solution for the yamaa benchmark adam-adsl-disposition.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024.;
run;

data ds;
  length STUDYID USUBJID DSCAT DSDECOD DSTERM DSSTDTC $1024;
  infile "/app/input/ds.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DSSEQ DSCAT : $char1024. DSDECOD : $char1024. DSTERM : $char1024. DSSTDTC : $char1024.;
run;

proc sort data=ds(where=(DSCAT='DISPOSITION EVENT' and DSSTDTC ne '')) out=ordered;
  by STUDYID USUBJID descending DSSTDTC descending DSSEQ;
run;
data lastds; set ordered; by STUDYID USUBJID; if first.USUBJID; run;
proc sql;
  create table joined as select a.*, b.DSSTDTC as EOSDT,b.DSDECOD as EOSDECOD,b.DSTERM as EOSREAS from dm as a left join lastds as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;
data result;
  length EOSSTT $12 DCSREAS $1024;
  set joined;
  if missing(EOSDT) then EOSSTT='ONGOING';
  else if EOSDECOD='COMPLETED' then EOSSTT='COMPLETED';
  else EOSSTT='DISCONTINUED';
  if EOSSTT='DISCONTINUED' then DCSREAS=EOSREAS;
run;

proc sql;
  create table final as select STUDYID, USUBJID, EOSDT, EOSDECOD, EOSREAS, EOSSTT, DCSREAS from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
