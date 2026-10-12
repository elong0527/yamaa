/* opensas reference solution for the yamaa benchmark adam-adae-worst-severity.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adae_raw;
  length STUDYID USUBJID AEBODSYS AEDECOD ASTDT AESEV TRTEMFL $1024;
  infile "/app/input/adae_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AEBODSYS : $char1024. AEDECOD : $char1024. ASTDT : $char1024. AESEV : $char1024. TRTEMFL : $char1024.;
run;

data graded;
  length ASEV $20;
  set adae_raw; NODATE=missing(ASTDT); ASEV=AESEV;
  select(ASEV); when('MILD') ASEVN=1; when('MODERATE') ASEVN=2; when('SEVERE') ASEVN=3; when('LIFE-THREATENING') ASEVN=4; otherwise ASEVN=.; end;
run;
proc sort data=graded(where=(TRTEMFL='Y' and not missing(ASEVN))) out=eligible; by STUDYID USUBJID AEDECOD descending ASEVN NODATE ASTDT AESEQ; run;
data worst; set eligible; by STUDYID USUBJID AEDECOD; if first.AEDECOD; keep STUDYID USUBJID AESEQ; run;
proc sql; create table result as select a.*,case when not missing(b.AESEQ) then 'Y' else '' end as AOCCPIFL from graded as a left join worst as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AESEQ=b.AESEQ; quit;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AEBODSYS, AEDECOD, ASTDT, AESEV, TRTEMFL, ASEVN, AOCCPIFL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
