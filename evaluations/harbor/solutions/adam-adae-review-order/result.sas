/* SAS language reference solution for the yamaa benchmark adam-adae-review-order.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AETERM AESTDTC AESEV $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESTDTC : $char1024. AESEV : $char1024.;
run;

proc sql;
  create table result as
  select STUDYID,USUBJID,AESEQ as ASEQ,AETERM,AESTDTC as ASTDT,AESEV as ASEV from ae;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, ASEQ, AETERM, ASTDT, ASEV from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
