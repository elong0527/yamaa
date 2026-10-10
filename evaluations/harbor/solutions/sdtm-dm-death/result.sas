/* SAS language reference solution for the yamaa benchmark sdtm-dm-death.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AETERM AEOUT AEENDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AEOUT : $char1024. AEENDTC : $char1024.;
run;

data dm_raw;
  length STUDYID USUBJID $1024;
  infile "/app/input/dm_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024.;
run;

data ds;
  length STUDYID USUBJID DSDECOD DSCAT DSSTDTC $1024;
  infile "/app/input/ds.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DSSEQ DSDECOD : $char1024. DSCAT : $char1024. DSSTDTC : $char1024.;
run;

proc sql;
create table dsdeath as select STUDYID,USUBJID,max(DSSTDTC) as DSDATE from ds where DSDECOD='DEATH' and not missing(DSSTDTC) group by STUDYID,USUBJID;
create table aedeath as select STUDYID,USUBJID,max(AEENDTC) as AEDATE from ae where AEOUT='FATAL' and not missing(AEENDTC) group by STUDYID,USUBJID;
create table joined as select a.*,b.DSDATE,c.AEDATE from dm_raw a left join dsdeath b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID left join aedeath c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID; quit;
data result; set joined; length DOMAIN $2 DTHDTC $1024 DTHFL $1; DOMAIN='DM'; DTHDTC=coalescec(DSDATE,AEDATE); if not missing(DTHDTC) then DTHFL='Y'; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, DTHDTC, DTHFL from result;
quit;
proc export data=final outfile="/app/output/dm.csv" dbms=csv replace; run;
