/* opensas reference solution for the yamaa benchmark sdtm-dm-dates.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AETERM AESTDTC AEENDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESTDTC : $char1024. AEENDTC : $char1024.;
run;

data dm_raw;
  length STUDYID USUBJID RFICDTC $1024;
  infile "/app/input/dm_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. RFICDTC : $char1024.;
run;

data ds;
  length STUDYID USUBJID DSCAT DSDECOD DSSTDTC $1024;
  infile "/app/input/ds.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DSSEQ DSCAT : $char1024. DSDECOD : $char1024. DSSTDTC : $char1024.;
run;

data ex;
  length STUDYID USUBJID EXTRT EXSTDTC EXENDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ EXTRT : $char1024. EXDOSE EXSTDTC : $char1024. EXENDTC : $char1024.;
run;

proc sql;
create table starts as select STUDYID,USUBJID,min(EXSTDTC) as RFXSTDTC from ex where not missing(EXSTDTC) group by STUDYID,USUBJID;
create table ends as select STUDYID,USUBJID,max(EXENDTC) as RFXENDTC from ex group by STUDYID,USUBJID;
create table alldates as select STUDYID,USUBJID,EXENDTC as DTC from ex union all select STUDYID,USUBJID,DSSTDTC as DTC from ds where DSCAT='DISPOSITION EVENT' union all select STUDYID,USUBJID,AEENDTC as DTC from ae;
create table lastdate as select STUDYID,USUBJID,max(DTC) as RFPENDTC from alldates group by STUDYID,USUBJID;
create table joined as select a.*,b.RFXSTDTC,c.RFXENDTC,d.RFPENDTC from dm_raw a left join starts b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID left join ends c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID left join lastdate d on a.STUDYID=d.STUDYID and a.USUBJID=d.USUBJID; quit;
data result; set joined; length DOMAIN $2 RFSTDTC RFENDTC $1024; DOMAIN='DM'; RFSTDTC=RFXSTDTC; if not missing(RFSTDTC) then RFENDTC=RFPENDTC; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, RFICDTC, RFXSTDTC, RFXENDTC, RFSTDTC, RFPENDTC, RFENDTC from result;
quit;
proc export data=final outfile="/app/output/dm.csv" dbms=csv replace; run;
