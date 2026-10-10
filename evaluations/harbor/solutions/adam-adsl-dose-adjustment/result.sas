/* SAS language reference solution for the yamaa benchmark adam-adsl-dose-adjustment.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024.;
run;

data ec;
  length STUDYID USUBJID ECADJ $1024;
  infile "/app/input/ec.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ECADJ : $char1024.;
run;

data ex;
  length STUDYID USUBJID EXADJ $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXADJ : $char1024.;
run;

data fa;
  length STUDYID USUBJID FATESTCD FAOBJ FASTRESC $1024;
  infile "/app/input/fa.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. FATESTCD : $char1024. FAOBJ : $char1024. FASTRESC : $char1024.;
run;

proc sql;
  create table evidence as
  select STUDYID,USUBJID,case when not missing(EXADJ) then 1 else 0 end as adjusted from ex
  union all select STUDYID,USUBJID,case when not missing(ECADJ) then 1 else 0 end from ec
  union all select STUDYID,USUBJID,case when FATESTCD='OCCUR' and FAOBJ='DOSE ADJUSTMENT' and FASTRESC='Y' then 1 else 0 end from fa;
  create table flags as select STUDYID,USUBJID,max(adjusted) as adjusted,count(*) as records from evidence group by STUDYID,USUBJID;
  create table result as select a.*, case when b.adjusted=1 then 'Y' when b.records>0 then 'N' else '' end as DOSADJFL from adsl as a left join flags as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, DOSADJFL from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
