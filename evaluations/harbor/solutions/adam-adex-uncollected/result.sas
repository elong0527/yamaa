/* opensas reference solution for the yamaa benchmark adam-adex-uncollected.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ex;
  length STUDYID DOMAIN USUBJID EXTRT EXDOSU EXSTDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. DOMAIN : $char1024. USUBJID : $char1024. EXSEQ EXTRT : $char1024. EXDOSE EXDOSU : $char1024. EXSTDTC : $char1024.;
run;

data adsl;
  length STUDYID USUBJID TRT01P $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRT01P : $char1024.;
run;

proc sql;
  create table totals as select STUDYID,USUBJID,sum(EXDOSE) as DOSECUM,count(*) as NDOSREC,count(EXDOSE) as NDOSVAL from ex group by STUDYID,USUBJID;
  create table result as select a.*,b.DOSECUM,b.NDOSREC,b.NDOSVAL from adsl as a left join totals as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;

proc export data=result(keep=STUDYID USUBJID TRT01P DOSECUM NDOSREC NDOSVAL) outfile="/app/output/adex.csv" dbms=csv replace; run;
