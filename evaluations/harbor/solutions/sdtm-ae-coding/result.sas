/* opensas reference solution for the yamaa benchmark sdtm-ae-coding.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae_raw;
  length STUDYID USUBJID AETERM $1024;
  infile "/app/input/ae_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024.;
run;

data meddra_26_1;
  length LLTNAME PTNAME SOCNAME $1024;
  infile "/app/input/meddra_26_1.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input LLTNAME : $char1024. PTNAME : $char1024. SOCNAME : $char1024.;
run;

proc sql;
  create table result as
  select 'AE' as DOMAIN,a.*,coalescec(b.PTNAME,'NOT CODED') as AEDECOD,coalescec(b.SOCNAME,'NOT CODED') as AEBODSYS from ae_raw a left join meddra_26_1 b on a.AETERM=b.LLTNAME;
quit;

proc sql;
  create table final as select DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AEDECOD, AEBODSYS from result;
quit;
proc export data=final outfile="/app/output/ae.csv" dbms=csv replace; run;
