/* opensas reference solution for the yamaa benchmark adam-adae-phase-emergence.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TR01SDT TR01EDT TR02SDT TR02EDT TR03SDT TR03EDT EOTSTT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TR01SDT : $char1024. TR01EDT : $char1024.
        TR02SDT : $char1024. TR02EDT : $char1024. TR03SDT : $char1024. TR03EDT : $char1024.
        EOTSTT : $char1024.;
run;

data ae;
  length STUDYID USUBJID AETERM AESER AESTDTC $1024;
  infile "/app/input/ae.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ AETERM : $char1024. AESER : $char1024.
        AESTDTC : $char1024.;
run;

proc sql;
  create table joined as
  select a.*, b.TR01SDT, b.TR01EDT, b.TR02SDT, b.TR02EDT, b.TR03SDT, b.TR03EDT, b.EOTSTT
  from ae as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;

data events;
  length ASTDT $10 TREM01FL TREM02FL TREM03FL TRTEMFL $1;
  set joined;
  ASTDT=AESTDTC; TREM01FL=''; TREM02FL=''; TREM03FL=''; TRTEMFL='';
  onset=.; s1=.; e1=.; s2=.; e2=.; s3=.; e3=.;
  if not missing(AESTDTC) then onset=input(AESTDTC, yymmdd10.);
  if not missing(TR01SDT) then s1=input(TR01SDT, yymmdd10.);
  if not missing(TR01EDT) then e1=input(TR01EDT, yymmdd10.);
  if not missing(TR02SDT) then s2=input(TR02SDT, yymmdd10.);
  if not missing(TR02EDT) then e2=input(TR02EDT, yymmdd10.);
  if not missing(TR03SDT) then s3=input(TR03SDT, yymmdd10.);
  if not missing(TR03EDT) then e3=input(TR03EDT, yymmdd10.);
  /* Phase 1 ends the day before the next phase the subject entered starts:
     surgery, or the first adjuvant dose for a subject who had no surgery. */
  next1=s2;
  if missing(next1) then next1=s3;
  /* The last phase a subject entered ends 30 days after its last date for a
     non-serious event and 90 days after it for a serious one, both days
     included, and has no end while treatment is ongoing. */
  lastend=e1;
  if not missing(s3) then lastend=e3;
  else if not missing(s2) then lastend=e2;
  tail=.;
  if AESER='Y' then tail=90;
  else if AESER='N' then tail=30;
  inlast=0;
  if EOTSTT='ONGOING' then inlast=1;
  else if not missing(onset) and not missing(lastend) and not missing(tail) then do;
    if onset<=lastend+tail then inlast=1;
  end;
  /* SAS missing dates sort below every date, so each phase start is tested
     for presence before the onset is compared with it. An untreated subject
     has no phase start and so no flag. */
  if not missing(onset) and not missing(s1) then do;
    if onset>=s1 then do;
      if not missing(next1) then do;
        if onset<next1 then TREM01FL='Y';
      end;
      else if inlast=1 then TREM01FL='Y';
    end;
  end;
  if not missing(onset) and not missing(s2) then do;
    if onset>=s2 then do;
      if not missing(s3) then do;
        if onset<s3 then TREM02FL='Y';
      end;
      else if inlast=1 then TREM02FL='Y';
    end;
  end;
  if not missing(onset) and not missing(s3) then do;
    if onset>=s3 and inlast=1 then TREM03FL='Y';
  end;
  if TREM01FL='Y' or TREM02FL='Y' or TREM03FL='Y' then TRTEMFL='Y';
run;

proc sql;
  create table final as
  select STUDYID, USUBJID, AESEQ, AETERM, AESER, ASTDT, TREM01FL, TREM02FL, TREM03FL, TRTEMFL
  from events;
quit;
proc export data=final outfile="/app/output/adae.csv" dbms=csv replace; run;
