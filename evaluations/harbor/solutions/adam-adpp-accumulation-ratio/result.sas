/* opensas reference solution for the yamaa benchmark adam-adpp-accumulation-ratio.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data pp;
  length STUDYID DOMAIN USUBJID PPSEQ PPTESTCD PPTEST PPSTRESU PPSTAT PPREASND VISITNUM VISIT $1024;
  infile "/app/input/pp.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. DOMAIN : $char1024. USUBJID : $char1024. PPSEQ : $char1024.
        PPTESTCD : $char1024. PPTEST : $char1024. PPSTRESN PPSTRESU : $char1024.
        PPSTAT : $char1024. PPREASND : $char1024. VISITNUM : $char1024. VISIT : $char1024.;
  ORD = _n_;
run;

data adsl;
  length STUDYID USUBJID ARMCD ARM SSVISIT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. ARMCD : $char1024. ARM : $char1024.;
  /* The steady-state profile is the last planned dosing day of the arm. */
  if ARMCD = 'A' then SSVISIT = 'DAY 7';
  else if ARMCD = 'B' then SSVISIT = 'DAY 5';
  else SSVISIT = '';
run;

/* Every PP record, a parameter that was not calculated included. */
data collected;
  length STUDYID USUBJID PARAMCD PARAM AVISIT AVALU $1024;
  set pp;
  PARAMCD = PPTESTCD; PARAM = PPTEST; AVISIT = VISIT; AVAL = PPSTRESN; AVALU = PPSTRESU;
  keep STUDYID USUBJID PARAMCD PARAM AVISIT AVAL AVALU ORD;
run;

data firstdose;
  set pp;
  if VISIT = 'DAY 1';
  FIRSTVAL = PPSTRESN;
  keep STUDYID USUBJID PPTESTCD FIRSTVAL;
run;

proc sql;
  create table steady as
    select p.STUDYID, p.USUBJID, p.PPTESTCD, p.VISIT, p.PPSTRESN as SSVAL, p.ORD
    from pp as p inner join adsl as a
      on p.STUDYID = a.STUDYID and p.USUBJID = a.USUBJID and p.VISIT = a.SSVISIT;
  create table pairs as
    select s.STUDYID, s.USUBJID, s.PPTESTCD, s.VISIT, s.SSVAL, f.FIRSTVAL, s.ORD
    from steady as s inner join firstdose as f
      on s.STUDYID = f.STUDYID and s.USUBJID = f.USUBJID and s.PPTESTCD = f.PPTESTCD;
quit;

/* A ratio needs both values; it sits at the steady-state visit. */
data ratios;
  length STUDYID USUBJID PARAMCD PARAM AVISIT AVALU $1024;
  set pairs;
  if not missing(SSVAL) and not missing(FIRSTVAL) and FIRSTVAL ne 0;
  if PPTESTCD = 'AUCTAU' then do; PARAMCD = 'ARAUC'; PARAM = 'Accumulation Ratio AUCTAU'; end;
  else if PPTESTCD = 'CMAX' then do; PARAMCD = 'ARCMAX'; PARAM = 'Accumulation Ratio Cmax'; end;
  else delete;
  AVISIT = VISIT; AVAL = SSVAL / FIRSTVAL; AVALU = 'RATIO';
  ORD = ORD + 100000;
  keep STUDYID USUBJID PARAMCD PARAM AVISIT AVAL AVALU ORD;
run;

data result; set collected ratios; run;
proc sort data=result; by ORD; run;

proc sql;
  create table final as select STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU from result;
quit;
proc export data=final outfile="/app/output/adpp.csv" dbms=csv replace; run;
