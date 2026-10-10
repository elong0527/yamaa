/* SAS language reference solution for the yamaa benchmark adam-adce-worst-toxicity.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ce;
  length STUDYID USUBJID CETERM CESEV ASTDT $1024;
  infile "/app/input/ce.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CESEQ CETERM : $char1024. CESEV : $char1024. ASTDT : $char1024.;
run;

data graded;
  length ASEV $1024;
  set ce; ASEV=CESEV;
  select(ASEV); when('MILD') ASEVN=1; when('MODERATE') ASEVN=2; when('SEVERE') ASEVN=3; otherwise ASEVN=.; end;
  ATOXGRN=ASEVN;
run;
proc sort data=graded(where=(not missing(ATOXGRN))) out=eligible; by STUDYID USUBJID descending ATOXGRN ASTDT CESEQ; run;
data worst; set eligible; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID CESEQ; run;
proc sql; create table result as select a.*,case when not missing(b.CESEQ) then 'Y' else '' end as AOCCFL from graded as a left join worst as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.CESEQ=b.CESEQ; quit;

proc sql;
  create table final as select STUDYID, USUBJID, CESEQ, CETERM, ASTDT, ASEV, ASEVN, ATOXGRN, AOCCFL from result;
quit;
proc export data=final outfile="/app/output/adce.csv" dbms=csv replace; run;
