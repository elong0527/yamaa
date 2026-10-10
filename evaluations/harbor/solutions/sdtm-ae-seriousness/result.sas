/* opensas reference solution for the yamaa benchmark sdtm-ae-seriousness.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae_raw;
  length STUDYID USUBJID AETERM AESDTH AESLIFE AESHOSP AESDISAB AESCONG AESMIE $1024;
  infile "/app/input/ae_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESDTH : $char1024. AESLIFE : $char1024. AESHOSP : $char1024. AESDISAB : $char1024. AESCONG : $char1024. AESMIE : $char1024.;
run;

data result; set ae_raw; length DOMAIN $2 AESER $1; DOMAIN='AE'; AESER='N';
if AESDTH='Y' or AESLIFE='Y' or AESHOSP='Y' or AESDISAB='Y' or AESCONG='Y' or AESMIE='Y' then AESER='Y'; run;
proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESER, AESDTH, AESLIFE, AESHOSP, AESDISAB, AESCONG, AESMIE from result;
quit;
proc export data=final outfile="/app/output/ae.csv" dbms=csv replace; run;
