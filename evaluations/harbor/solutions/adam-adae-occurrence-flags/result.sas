/* opensas reference solution for the yamaa benchmark adam-adae-occurrence-flags.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adae_raw;
  length STUDYID USUBJID AEBODSYS AEDECOD ASTDT TRTEMFL $1024;
  infile "/app/input/adae_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AEBODSYS : $char1024. AEDECOD : $char1024. ASTDT : $char1024. TRTEMFL : $char1024.;
run;

proc sort data=adae_raw(where=(TRTEMFL='Y')) out=eligible; by STUDYID USUBJID ASTDT AESEQ; run;
data subject; set eligible; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID AESEQ; run;
proc sort data=eligible; by STUDYID USUBJID AEBODSYS ASTDT AESEQ; run;
data system; set eligible; by STUDYID USUBJID AEBODSYS; if first.AEBODSYS; keep STUDYID USUBJID AESEQ; run;
proc sort data=eligible; by STUDYID USUBJID AEBODSYS AEDECOD ASTDT AESEQ; run;
data term; set eligible; by STUDYID USUBJID AEBODSYS AEDECOD; if first.AEDECOD; keep STUDYID USUBJID AESEQ; run;
proc sql; create table result as select a.*,
  case when not missing(b.AESEQ) then 'Y' else '' end as AOCCFL,
  case when not missing(c.AESEQ) then 'Y' else '' end as AOCCSFL,
  case when not missing(d.AESEQ) then 'Y' else '' end as AOCCPFL
  from adae_raw as a left join subject as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AESEQ=b.AESEQ
  left join system as c on a.STUDYID=c.STUDYID and a.USUBJID=c.USUBJID and a.AESEQ=c.AESEQ
  left join term as d on a.STUDYID=d.STUDYID and a.USUBJID=d.USUBJID and a.AESEQ=d.AESEQ; quit;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AEBODSYS, AEDECOD, ASTDT, TRTEMFL, AOCCFL, AOCCSFL, AOCCPFL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
