/* opensas reference solution for the yamaa benchmark sdtm-ta-sponsor-epoch.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data design;
  length STUDYID ARMCD ARM ETCD ELEMENT EPOCH $1024;
  infile "/app/input/design.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. ARMCD : $char1024. ARM : $char1024. TAETORD
    ETCD : $char1024. ELEMENT : $char1024. EPOCH : $char1024.;
run;

data result; set design; length DOMAIN $2; DOMAIN='TA'; run;
proc sql;
  create table final as select DOMAIN, STUDYID, ARMCD, ARM, TAETORD, ETCD, ELEMENT,
    EPOCH from result;
quit;
proc export data=final outfile="/app/output/ta.csv" dbms=csv replace; run;
