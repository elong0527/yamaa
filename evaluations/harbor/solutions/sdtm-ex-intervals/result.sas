/* opensas reference solution for the yamaa benchmark sdtm-ex-intervals.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ec_raw;
  length STUDYID USUBJID ECTRT ECDOSU ECDOSFRQ ECSTDTC ECADJ $1024;
  infile "/app/input/ec_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ECTRT : $char1024. ECDOSE ECDOSU : $char1024. ECDOSFRQ : $char1024. ECSTDTC : $char1024. ECADJ : $char1024.;
run;

proc sql;
create table intervals as select STUDYID,USUBJID,ECTRT as EXTRT,ECDOSE as EXDOSE,ECDOSU as EXDOSU,ECDOSFRQ as EXDOSFRQ,min(ECSTDTC) as EXSTDTC,max(ECSTDTC) as EXENDTC from ec_raw group by STUDYID,USUBJID,ECTRT,ECDOSE,ECDOSU,ECDOSFRQ;
create table reasons as select STUDYID,USUBJID,ECTRT,ECDOSE,ECDOSU,ECDOSFRQ,min(ECADJ) as EXADJ from ec_raw where not missing(ECADJ) group by STUDYID,USUBJID,ECTRT,ECDOSE,ECDOSU,ECDOSFRQ;
create table prepared as select 'EX' as DOMAIN,a.*,b.EXADJ from intervals a left join reasons b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.EXTRT=b.ECTRT and a.EXDOSE=b.ECDOSE and a.EXDOSU=b.ECDOSU and a.EXDOSFRQ=b.ECDOSFRQ; quit;
proc sort data=prepared; by STUDYID USUBJID EXSTDTC EXTRT; run;
data result; set prepared; by STUDYID USUBJID; retain EXSEQ;
  if first.USUBJID then EXSEQ=0;
  EXSEQ+1;
run;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXDOSFRQ, EXSTDTC, EXENDTC, EXADJ from result;
quit;
proc export data=final outfile="/app/output/ex.csv" dbms=csv replace; run;
