/* SAS language reference solution for the yamaa benchmark adam-adae-death.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AEDECOD AEOUT ASTDT $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AEDECOD : $char1024. AEOUT : $char1024. ASTDT : $char1024.;
run;

data dm;
  length STUDYID USUBJID DTHDT $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DTHDT : $char1024.;
run;

proc sort data=ae(where=(AEOUT='FATAL')) out=fatal; by STUDYID USUBJID descending ASTDT descending AESEQ; run;
data latest; set fatal; by STUDYID USUBJID; if first.USUBJID; run;
proc sql;
  create table result as select a.*,case when not missing(b.USUBJID) or not missing(c.DTHDT) then 'Y' else '' end as DTHFL,
  b.AEDECOD as DTHCAUS,case when not missing(b.USUBJID) then b.ASTDT else c.DTHDT end as DTHDT
  from ae as a left join latest as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID left join dm as c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, DTHFL, DTHCAUS, DTHDT from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
