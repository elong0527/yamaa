/* opensas reference solution for the yamaa benchmark adam-adae-onset-emergence.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDTM TRTENDTM TEENDDTM $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDTM : $char1024. TRTENDTM : $char1024. TEENDDTM : $char1024.;
run;

data ae;
  length STUDYID USUBJID AETERM AESTDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESTDTC : $char1024.;
run;

proc sql; create table joined as select a.*,b.TRTSDTM,b.TEENDDTM from ae as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data events;
  length ASTDTM $19 TRTEMFL $1;
  set joined; ASTDTM=''; TRTEMFL='';
  /* Datetimes collected without seconds are completed to the second so the
     onset and the first exposure compare at the moment, not the day. */
  if not missing(AESTDTC) then ASTDTM=cats(AESTDTC,ifc(lengthn(AESTDTC)=16,':00',''));
  onset=input(ASTDTM,e8601dt.); treatment=input(TRTSDTM,e8601dt.); window_end=input(TEENDDTM,e8601dt.);
  /* The window rule: onset inside [first exposure, window end] is
     treatment-emergent. A missing onset cannot be ruled pre-treatment,
     so a treated subject's event with a missing onset is flagged Y; an
     untreated subject (no first exposure) is never treatment-emergent. */
  if not missing(treatment) then do;
    if missing(onset) then TRTEMFL='Y';
    else if onset>=treatment and onset<=window_end then TRTEMFL='Y';
  end;
run;
proc sort data=events(where=(TRTEMFL='Y' and not missing(onset))) out=eligible; by STUDYID USUBJID onset AESEQ; run;
data firstevent; set eligible; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID AESEQ; run;
proc sql; create table result as select a.*,case when not missing(b.AESEQ) then 'Y' else '' end as AOCCFL from events as a left join firstevent as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AESEQ=b.AESEQ; quit;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AETERM, ASTDTM, TRTSDTM, TEENDDTM, TRTEMFL, AOCCFL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
