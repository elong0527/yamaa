/* opensas reference solution for the yamaa benchmark adam-adqs-subscale-score.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data qs;
  length STUDYID USUBJID VISIT QSCAT QSTESTCD QSTEST $1024;
  infile "/app/input/qs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. VISIT : $char1024. QSSEQ QSCAT : $char1024. QSTESTCD : $char1024. QSTEST : $char1024. QSSTRESN;
run;

proc sql;
  create table collected as select STUDYID,USUBJID,VISIT,QSTESTCD as PARAMCD,QSTEST as PARAM,QSSTRESN as AVAL from qs where QSTESTCD in ('PF01','PF02','PF03','PF04');
  create table anchors as select distinct STUDYID,USUBJID,VISIT from collected where PARAMCD='PF01';
  create table scores as select a.STUDYID,a.USUBJID,a.VISIT,'PFSCORE' as PARAMCD,'Physical Functioning Subscale Score' as PARAM,
    case when count(b.AVAL)>=3 then 25*mean(b.AVAL) else . end as AVAL from anchors as a left join collected as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.VISIT=b.VISIT group by a.STUDYID,a.USUBJID,a.VISIT;
quit;
data result; set collected scores; run;

proc sql;
  create table final as select STUDYID, USUBJID, VISIT, PARAMCD, PARAM, AVAL from result;
quit;
proc export data=final outfile="/app/output/adqs.csv" dbms=csv replace; run;
