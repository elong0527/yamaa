/* opensas reference solution for the yamaa benchmark adam-adsl-crossover-sequence.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID ARMCD ARM ACTARMCD ACTARM ARMNRS $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ARMCD : $char1024. ARM : $char1024. ACTARMCD : $char1024. ACTARM : $char1024. ARMNRS : $char1024.;
run;

data ta;
  length STUDYID ARMCD ARM ETCD EPOCH $1024;
  infile "/app/input/ta.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. ARMCD : $char1024. ARM : $char1024. TAETORD ETCD : $char1024. EPOCH : $char1024.;
run;

data te;
  length STUDYID ETCD ELEMENT TESTRL TEDUR $1024;
  infile "/app/input/te.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. ETCD : $char1024. ELEMENT : $char1024. TESTRL : $char1024. TEDUR : $char1024.;
run;

/* Only elements in an epoch whose name begins with TREATMENT count, taken
   in planned order within each arm. */
data treatment;
  set ta;
  where substr(EPOCH, 1, 9) = 'TREATMENT';
run;
proc sort data=treatment;
  by STUDYID ARMCD TAETORD;
run;
data ranked;
  set treatment;
  by STUDYID ARMCD;
  if first.ARMCD then TRTNUM = 0;
  TRTNUM + 1;
run;

proc sql;
  create table periods as
    select r.STUDYID, r.ARMCD, r.TRTNUM, t.ELEMENT
    from ranked as r
    left join te as t on r.STUDYID = t.STUDYID and r.ETCD = t.ETCD;
  create table period1 as
    select STUDYID, ARMCD, ELEMENT as TRT01P from periods where TRTNUM = 1;
  create table period2 as
    select STUDYID, ARMCD, ELEMENT as TRT02P from periods where TRTNUM = 2;
  create table joined as
    select a.STUDYID, a.USUBJID, b.TRT01P, c.TRT02P
    from dm as a
    left join period1 as b on a.STUDYID = b.STUDYID and a.ARMCD = b.ARMCD
    left join period2 as c on a.STUDYID = c.STUDYID and a.ARMCD = c.ARMCD;
quit;

data result;
  length TRT01P TRT02P TRTSEQP $1024;
  set joined;
  if not missing(TRT01P) and not missing(TRT02P) then
    TRTSEQP = trim(TRT01P) || '/' || trim(TRT02P);
run;

proc sql;
  create table final as select STUDYID, USUBJID, TRT01P, TRT02P, TRTSEQP from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
