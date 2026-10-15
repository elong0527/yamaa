/* opensas reference: long-format operations from the safety population. */
data subjects;
  length STUDYID USUBJID $80 TRT01P $20 SAFFL $1;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover;
  input STUDYID : $80. USUBJID : $80. TRT01P : $20. TRT01PN SAFFL : $1.;
  if SAFFL='Y';
run;
data events;
  length STUDYID USUBJID $80 TRTEMFL $1;
  infile "/app/input/adae.csv" dsd dlm="," firstobs=2 truncover;
  input STUDYID : $80. USUBJID : $80. ASEQ TRTEMFL : $1.;
  if TRTEMFL='Y';
run;
proc sql;
  create table qualifying as select distinct STUDYID, USUBJID from events;
  create table counted as select s.STUDYID, s.USUBJID, s.TRT01P, s.TRT01PN,
    case when e.USUBJID is not null then 1 else 0 end as EVENTN
  from subjects s left join qualifying e
    on s.STUDYID=e.STUDYID and s.USUBJID=e.USUBJID;
  create table totals as select STUDYID, TRT01P, TRT01PN,
    sum(EVENTN) as NSUBJ, count(USUBJID) as DENOM
  from counted group by STUDYID, TRT01P, TRT01PN;
quit;
data results;
  length ANALYSISID $8 ANALYSISSETID $8 DATASET $8 VARIABLE $8
    ANALYSISSET DATASUBSET $24 GROUPINGID $8 GROUPLEVEL $20
    METHODID $16 OPERATIONID $8;
  set totals;
  ANALYSISID='AEINC'; ANALYSISSETID='SAFETY'; DATASET='ADAE';
  VARIABLE='USUBJID'; ANALYSISSET="SAFFL = 'Y'";
  DATASUBSET="TRTEMFL = 'Y'"; GROUPINGID='TRT01P';
  GROUPORDER=TRT01PN; GROUPLEVEL=TRT01P; METHODID='AEINCIDENCE';
  OPERATIONID='COUNT'; RESULT=NSUBJ; output;
  OPERATIONID='PERCENT'; RESULT=100*NSUBJ/DENOM; output;
run;
proc sql;
  create table final as select STUDYID, ANALYSISID, ANALYSISSETID,
    DATASET, VARIABLE, ANALYSISSET, DATASUBSET, GROUPINGID, GROUPORDER,
    GROUPLEVEL, METHODID, OPERATIONID, RESULT from results
  order by STUDYID, GROUPORDER, OPERATIONID;
quit;
proc export data=final outfile="/app/output/results.csv" dbms=csv replace;
run;
