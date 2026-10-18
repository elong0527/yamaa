/* opensas reference solution for the yamaa benchmark adam-adlb-metadata.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data lb;
  length STUDYID USUBJID LBTESTCD LBTEST LBSTRESC LBSTRESU $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. LBSEQ LBTESTCD : $char1024.
    LBTEST : $char1024. LBSTRESC : $char1024. LBSTRESN LBSTRESU : $char1024.;
run;

data result;
  set lb;
  length PARAMCD $8 PARAM $40 AVALC $200 AVALU $200;
  PARAMCD = LBTESTCD;
  if LBTESTCD = 'GLUC' then PARAM = 'Glucose (mg/dL)';
  else if LBTESTCD = 'CREAT' then PARAM = 'Creatinine (umol/L)';
  else if LBTESTCD = 'HCG' then PARAM = 'Choriogonadotropin Beta';
  AVAL = LBSTRESN;
  AVALC = LBSTRESC;
  AVALU = LBSTRESU;
run;
proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, AVAL, AVALC, AVALU
    from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
