/* opensas reference solution for the yamaa benchmark adam-adpp-comparison-sets.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data raw;
  length STUDYID USUBJID PARAMCD PARAM TRTA $200;
  infile "/app/input/adpp_raw.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char200. USUBJID : $char200. PARAMCD : $char200.
        PARAM : $char200. APERIOD TRTA : $char200. AVAL;
  ORD = _n_;
run;

data adsl;
  length STUDYID USUBJID TRTSEQP $200;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char200. USUBJID : $char200. TRTSEQP : $char200.;
run;

data compare;
  length COMPARISON TESTTRT REFTRT $200;
  infile "/app/input/compare.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input COMPNO COMPARISON : $char200. TESTTRT : $char200. REFTRT : $char200.;
run;

/* Each comparison's two treatments, one column pair per comparison. */
data pairs;
  length TEST1 REF1 TEST2 REF2 $200;
  retain TEST1 REF1 TEST2 REF2 N1 N2 0;
  set compare end=last;
  if COMPNO = 1 then do; TEST1 = TESTTRT; REF1 = REFTRT; N1 = N1 + 1; end;
  if COMPNO = 2 then do; TEST2 = TESTTRT; REF2 = REFTRT; N2 = N2 + 1; end;
  if last then do;
    if N1 ne 1 or N2 ne 1 then abort;
    output;
  end;
  keep TEST1 REF1 TEST2 REF2;
run;

/* The other treatment of each comparison for every record. */
data others;
  length OTHER1 OTHER2 $200;
  if _n_ = 1 then set pairs;
  set raw;
  if TRTA = TEST1 then OTHER1 = REF1;
  else if TRTA = REF1 then OTHER1 = TEST1;
  if TRTA = TEST2 then OTHER2 = REF2;
  else if TRTA = REF2 then OTHER2 = TEST2;
run;

data partner;
  length PSTUDY PSUBJ PPARAM PTRT $200;
  set raw;
  PSTUDY = STUDYID; PSUBJ = USUBJID; PPARAM = PARAMCD; PTRT = TRTA; PAVAL = AVAL;
  keep PSTUDY PSUBJ PPARAM PTRT PAVAL;
run;

proc sql;
  create table with1 as
    select o.*, p.PAVAL as PAVAL1
    from others as o left join partner as p
      on o.STUDYID = p.PSTUDY and o.USUBJID = p.PSUBJ and o.PARAMCD = p.PPARAM
         and o.OTHER1 = p.PTRT;
  create table with2 as
    select o.*, p.PAVAL as PAVAL2
    from with1 as o left join partner as p
      on o.STUDYID = p.PSTUDY and o.USUBJID = p.PSUBJ and o.PARAMCD = p.PPARAM
         and o.OTHER2 = p.PTRT;
  create table withseq as
    select o.*, a.TRTSEQP
    from with2 as o left join adsl as a
      on o.STUDYID = a.STUDYID and o.USUBJID = a.USUBJID;
quit;

/* Y when the record and its partner under the comparison's other treatment
   are both present and greater than 0. */
data final;
  length ANL01FL ANL02FL $1;
  set withseq;
  if not missing(OTHER1) and not missing(AVAL) and AVAL > 0
     and not missing(PAVAL1) and PAVAL1 > 0 then ANL01FL = 'Y';
  else ANL01FL = ' ';
  if not missing(OTHER2) and not missing(AVAL) and AVAL > 0
     and not missing(PAVAL2) and PAVAL2 > 0 then ANL02FL = 'Y';
  else ANL02FL = ' ';
run;

proc sort data=final; by ORD; run;

proc sql;
  create table adpp as
    select STUDYID, USUBJID, PARAMCD, PARAM, APERIOD, TRTA, TRTSEQP, AVAL,
           ANL01FL, ANL02FL
    from final;
quit;
proc export data=adpp outfile="/app/output/adpp.csv" dbms=csv replace; run;
