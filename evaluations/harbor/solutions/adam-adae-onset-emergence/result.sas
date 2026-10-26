/* opensas reference solution for the yamaa benchmark adam-adae-onset-emergence.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDTM TRTEDTM EOTSTT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDTM : $char1024. TRTEDTM : $char1024. EOTSTT : $char1024.;
run;

data ae;
  length STUDYID USUBJID AETERM AESER AESTDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESER : $char1024. AESTDTC : $char1024.;
run;

proc sql; create table joined as select a.*,b.TRTSDTM,b.TRTEDTM,b.EOTSTT from ae as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID; quit;
data events;
  length ASTDTM TEENDDTM $19 TRTEMFL $1;
  set joined; ASTDTM=''; TEENDDTM=''; TRTEMFL='';
  /* Datetimes collected without seconds are completed to the second so the
     onset and the first exposure compare at the moment, not the day. */
  if not missing(AESTDTC) then ASTDTM=cats(AESTDTC,ifc(lengthn(AESTDTC)=16,':00',''));
  /* The window ends 30 days after last exposure for an event recorded as
     non-serious and 90 days after for any other; while treatment is
     ongoing it has no end. Whole days keep the time of day. */
  window_days=90;
  if AESER='N' then window_days=30;
  if upcase(EOTSTT) ne 'ONGOING' and not missing(TRTEDTM) then
    TEENDDTM=cats(put(input(substr(TRTEDTM,1,10),yymmdd10.)+window_days,yymmdd10.),substr(TRTEDTM,11));
  onset=input(ASTDTM,e8601dt.); treatment=input(TRTSDTM,e8601dt.); window_end=input(TEENDDTM,e8601dt.);
  /* The window rule: onset at or after first exposure and, unless
     treatment is ongoing, at or before the window end is
     treatment-emergent. A missing onset cannot be ruled pre-treatment,
     so a treated subject's event with a missing onset is flagged Y; an
     untreated subject (no first exposure) is never treatment-emergent. */
  if not missing(treatment) then do;
    if missing(onset) then TRTEMFL='Y';
    else if onset>=treatment and (upcase(EOTSTT)='ONGOING' or (not missing(window_end) and onset<=window_end)) then TRTEMFL='Y';
  end;
run;
proc sort data=events(where=(TRTEMFL='Y' and not missing(onset))) out=eligible; by STUDYID USUBJID onset AESEQ; run;
data firstevent; set eligible; by STUDYID USUBJID; if first.USUBJID; keep STUDYID USUBJID AESEQ; run;
proc sql; create table result as select a.*,case when not missing(b.AESEQ) then 'Y' else '' end as AOCCFL from events as a left join firstevent as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AESEQ=b.AESEQ; quit;

proc sql;
  create table final as select STUDYID, USUBJID, AESEQ, AETERM, AESER, ASTDTM, TRTSDTM, TRTEDTM, EOTSTT, TEENDDTM, TRTEMFL, AOCCFL from result;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
