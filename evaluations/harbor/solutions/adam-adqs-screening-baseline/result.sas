/* opensas reference solution for the yamaa benchmark adam-adqs-screening-baseline.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  length STUDYID USUBJID TRTSDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDT : $char1024.;
run;

/* ISO dates are kept as text, so they compare in date order. A not-done
   questionnaire has no rating. */
data qs;
  length STUDYID USUBJID QSCAT PARAMCD PARAM QSSTAT VISIT ADT $1024;
  infile "/app/input/qs.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. QSSEQ QSCAT : $char1024.
        PARAMCD : $char1024. PARAM : $char1024. AVAL QSSTAT : $char1024.
        VISITNUM VISIT : $char1024. ADT : $char1024.;
  if upcase(QSSTAT) = "NOT DONE" then AVAL = .;
run;

/* Treatment start date from ADSL on every QS record. */
proc sql;
  create table qs2 as
    select a.STUDYID, a.USUBJID, a.PARAMCD, a.PARAM, a.VISITNUM, a.VISIT,
           a.ADT, b.TRTSDT, a.AVAL
    from qs as a left join adsl as b
    on a.STUDYID = b.STUDYID and a.USUBJID = b.USUBJID;
quit;

proc sort data=qs2;
  by STUDYID USUBJID PARAMCD VISITNUM;
run;

/* One row per subject and parameter that has a baseline: the CYCLE 1
   DAY 1 rating dated on or before TRTSDT, otherwise the SCREENING rating. */
data basesel;
  set qs2;
  by STUDYID USUBJID PARAMCD;
  length BLVISIT $20;
  retain c1d1ok scrok c1d1val scrval;
  if first.PARAMCD then do;
    c1d1ok = 0;
    scrok = 0;
    c1d1val = .;
    scrval = .;
  end;
  if VISIT = "CYCLE 1 DAY 1" and not missing(AVAL) and not missing(ADT)
     and not missing(TRTSDT) and ADT <= TRTSDT then do;
    c1d1ok = 1;
    c1d1val = AVAL;
  end;
  if VISIT = "SCREENING" and not missing(AVAL) then do;
    scrok = 1;
    scrval = AVAL;
  end;
  if last.PARAMCD then do;
    if c1d1ok = 1 then do;
      BLVISIT = "CYCLE 1 DAY 1";
      BASE = c1d1val;
      output;
    end;
    else if scrok = 1 then do;
      BLVISIT = "SCREENING";
      BASE = scrval;
      output;
    end;
  end;
  keep STUDYID USUBJID PARAMCD BLVISIT BASE;
run;

proc sql;
  create table combined as
    select a.*, b.BLVISIT, b.BASE
    from qs2 as a left join basesel as b
    on a.STUDYID = b.STUDYID and a.USUBJID = b.USUBJID
       and a.PARAMCD = b.PARAMCD;
quit;

/* Flags, BASE on every record of the subject and parameter, and CHG on
   the non-baseline records dated after TRTSDT. */
data adqs1;
  set combined;
  length ABLFL SCBLFL $1;
  ABLFL = "";
  SCBLFL = "";
  if not missing(BLVISIT) and VISIT = BLVISIT then do;
    ABLFL = "Y";
    if VISIT = "SCREENING" then SCBLFL = "Y";
  end;
  CHG = .;
  if ABLFL ne "Y" and not missing(AVAL) and not missing(BASE)
     and not missing(ADT) and not missing(TRTSDT) and ADT > TRTSDT then
    CHG = AVAL - BASE;
run;

proc sql;
  create table adqs as
    select STUDYID, USUBJID, PARAMCD, PARAM, VISITNUM, VISIT, ADT, TRTSDT,
           AVAL, ABLFL, SCBLFL, BASE, CHG
    from adqs1
    order by STUDYID, USUBJID, PARAMCD, VISITNUM;
quit;

proc export data=adqs outfile="/app/output/adqs.csv" dbms=csv replace;
run;
