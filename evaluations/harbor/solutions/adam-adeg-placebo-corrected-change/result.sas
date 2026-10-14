/* opensas reference solution for the yamaa benchmark adam-adeg-placebo-corrected-change.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adeg;
  length STUDYID USUBJID COHORT TRTA PARAMCD PARAM ATPT AVALU ABLFL BASETYPE $1024;
  infile "/app/input/adeg_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. COHORT : $char1024. TRTA : $char1024. PARAMCD : $char1024. PARAM : $char1024. APERIOD APERDAY ATPT : $char1024. ATPTN AVAL AVALU : $char1024. ABLFL : $char1024. BASE CHG BASETYPE : $char1024.;
run;

/* The comparator records: placebo records that are not baseline records. */
data flagged;
  length ACOMPFL $1;
  set adeg;
  if ABLFL ne 'Y' then AFTER = 1;
  else AFTER = 0;
  if AFTER = 1 and TRTA = 'Placebo' then ACOMPFL = 'Y';
  else ACOMPFL = '';
run;

/* The placebo group: every comparator record of the same parameter and
   nominal time point, all cohorts together; each mean skips missing values. */
data comparator;
  length G_PARAMCD $1024;
  set flagged;
  if ACOMPFL = 'Y';
  G_PARAMCD = PARAMCD; G_ATPTN = ATPTN;
  keep G_PARAMCD G_ATPTN AVAL BASE CHG;
run;

proc sql;
  create table groups as
    select G_PARAMCD, G_ATPTN, mean(AVAL) as G_COMP, mean(BASE) as G_COMPBASE,
           mean(CHG) as G_COMPCHG
    from comparator
    group by G_PARAMCD, G_ATPTN;
  create table matched as
    select a.*, b.G_COMP, b.G_COMPBASE, b.G_COMPCHG
    from flagged as a left join groups as b
      on a.PARAMCD = b.G_PARAMCD and a.ATPTN = b.G_ATPTN;
quit;

/* Placebo records carry the group's means like every other record after
   baseline; the double delta is on drug records only. */
data final;
  length COMPTYPE $1024;
  set matched;
  if AFTER = 1 then do;
    COMP = G_COMP; COMPBASE = G_COMPBASE; COMPCHG = G_COMPCHG;
    COMPTYPE = 'Placebo group at ' || strip(ATPT);
  end;
  else do;
    COMP = .; COMPBASE = .; COMPCHG = .;
    COMPTYPE = '';
  end;
  if AFTER = 1 and ACOMPFL ne 'Y' and not missing(CHG) and not missing(COMPCHG)
    then CCOMPCHG = CHG - COMPCHG;
  else CCOMPCHG = .;
run;

proc sql;
  create table out as
    select STUDYID, USUBJID, COHORT, TRTA, PARAMCD, PARAM, APERIOD, APERDAY,
           ATPT, ATPTN, AVAL, AVALU, ABLFL, BASE, CHG, BASETYPE, ACOMPFL,
           COMP, COMPBASE, COMPCHG, CCOMPCHG, COMPTYPE
    from final;
quit;
proc export data=out outfile="/app/output/adeg.csv" dbms=csv replace; run;
