/* opensas reference solution for the yamaa benchmark adam-adex-cumulative-dose.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ex;
  length STUDYID USUBJID EXTRT $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXTRT : $char1024. EXSEQ EXDOSE;
run;

data subject_treatment;
  length STUDYID USUBJID EXTRT EXDOSU $1024;
  infile "/app/input/subject_treatment.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXTRT : $char1024. EXDOSU : $char1024. PLANDOSE PLANCYC;
run;

proc sql;
  create table totals as select STUDYID,USUBJID,EXTRT,sum(EXDOSE) as DOSECUM,count(*) as NCYCLES from ex group by STUDYID,USUBJID,EXTRT;
  create table result as select a.*,b.DOSECUM,b.NCYCLES,case when a.PLANDOSE*a.PLANCYC ne 0 then 100*b.DOSECUM/(a.PLANDOSE*a.PLANCYC) else . end as RDI from subject_treatment as a left join totals as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.EXTRT=b.EXTRT;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, EXTRT, EXDOSU, DOSECUM, NCYCLES, RDI from result;
quit;
proc export data=final outfile="/app/output/adex.csv" dbms=csv replace; run;
