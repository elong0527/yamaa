/* opensas reference solution for the yamaa benchmark adam-adsl-period-status.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024.;
run;

data ex;
  length STUDYID USUBJID EPOCH EXTRT EXSTDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ EPOCH : $char1024. EXTRT : $char1024. EXSTDTC : $char1024.;
run;

data ds;
  length STUDYID USUBJID DSCAT EPOCH DSTERM DSDECOD DSSTDTC $1024;
  infile "/app/input/ds.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DSSEQ DSCAT : $char1024. EPOCH : $char1024. DSTERM : $char1024. DSDECOD : $char1024. DSSTDTC : $char1024.;
run;

proc sql;
  create table entered1 as
    select distinct STUDYID, USUBJID, 1 as P01EX from ex
    where EPOCH = 'TREATMENT 1';
  create table entered2 as
    select distinct STUDYID, USUBJID, 1 as P02EX from ex
    where EPOCH = 'TREATMENT 2';
  create table event1 as
    select STUDYID, USUBJID, DSDECOD as P01DECOD from ds
    where DSCAT = 'DISPOSITION EVENT' and EPOCH = 'TREATMENT 1';
  create table event2 as
    select STUDYID, USUBJID, DSDECOD as P02DECOD from ds
    where DSCAT = 'DISPOSITION EVENT' and EPOCH = 'TREATMENT 2';
  create table joined as
    select a.STUDYID, a.USUBJID, b.P01EX, c.P02EX, d.P01DECOD, e.P02DECOD
    from dm as a
    left join entered1 as b on a.STUDYID = b.STUDYID and a.USUBJID = b.USUBJID
    left join entered2 as c on a.STUDYID = c.STUDYID and a.USUBJID = c.USUBJID
    left join event1 as d on a.STUDYID = d.STUDYID and a.USUBJID = d.USUBJID
    left join event2 as e on a.STUDYID = e.STUDYID and a.USUBJID = e.USUBJID;
quit;

data result;
  length EOP01STT $12 DCP01RS $1024 EOP02STT $12 DCP02RS $1024;
  set joined;
  if missing(P01EX) then EOP01STT = '';
  else if P01DECOD = 'COMPLETED' then EOP01STT = 'COMPLETED';
  else if not missing(P01DECOD) then EOP01STT = 'DISCONTINUED';
  else EOP01STT = 'ONGOING';
  if EOP01STT = 'DISCONTINUED' then DCP01RS = P01DECOD;
  if missing(P02EX) then EOP02STT = '';
  else if P02DECOD = 'COMPLETED' then EOP02STT = 'COMPLETED';
  else if not missing(P02DECOD) then EOP02STT = 'DISCONTINUED';
  else EOP02STT = 'ONGOING';
  if EOP02STT = 'DISCONTINUED' then DCP02RS = P02DECOD;
run;

proc sql;
  create table final as select STUDYID, USUBJID, EOP01STT, DCP01RS, EOP02STT, DCP02RS from result;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
