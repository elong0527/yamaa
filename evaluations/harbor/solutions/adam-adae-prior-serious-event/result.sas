/* SAS language reference solution for the yamaa benchmark adam-adae-prior-serious-event.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data ae;
  length STUDYID USUBJID AEDECOD AESER $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AEDECOD : $char1024. AESER : $char1024.;
run;

proc sort data=ae(where=(AESER='Y')) out=serious; by STUDYID USUBJID AESEQ; run;
data firsts; set serious; by STUDYID USUBJID; if first.USUBJID; run;
proc sql; create table joined as select a.*,b.AEDECOD as firstterm,b.AESEQ as firstseq from ae as a left join firsts as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
proc sort data=joined; by STUDYID USUBJID AESEQ; run;
data result;
  length PRIOR_SAEFL $1 PRIOR_SAEDECOD FIRST_SAEDECOD PREV_AEDECOD lastterm prevterm $1024;
  retain lastterm lastseq prevterm;
  set joined; by STUDYID USUBJID;
  if first.USUBJID then do; lastterm=''; lastseq=.; prevterm=''; end;
  PREV_AEDECOD=prevterm;
  if AESER ne 'Y' then do;
    PRIOR_SAEDECOD=lastterm; PRIOR_SAESEQ=lastseq;
    FIRST_SAEDECOD=firstterm; FIRST_SAESEQ=firstseq;
    if not missing(lastseq) then PRIOR_SAEFL='Y';
  end;
  if AESER='Y' then do; lastterm=AEDECOD; lastseq=AESEQ; end;
  prevterm=AEDECOD;
run;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AEDECOD, AESER, PRIOR_SAEFL, PRIOR_SAEDECOD, PRIOR_SAESEQ, FIRST_SAEDECOD, FIRST_SAESEQ, PREV_AEDECOD from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
