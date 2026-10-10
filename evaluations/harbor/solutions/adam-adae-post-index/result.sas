/* opensas reference solution for the yamaa benchmark adam-adae-post-index.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AEDECOD AESTDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AEDECOD : $char1024. AESTDTC : $char1024.;
run;

proc sort data=ae(where=(AEDECOD='COVID-19' and AESTDTC ne '')) out=indices; by STUDYID USUBJID AESTDTC AESEQ; run;
data firstindex; set indices; by STUDYID USUBJID; if first.USUBJID; run;
proc sql;
  create table result as select a.*,a.AESTDTC as ASTDT,
  case when not missing(a.AESTDTC) and not missing(b.AESTDTC) and (a.AESTDTC>b.AESTDTC or (a.AESTDTC=b.AESTDTC and a.AESEQ>b.AESEQ)) then 'Y' else '' end as AFTIDXFL
  from ae as a left join firstindex as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, AFTIDXFL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
