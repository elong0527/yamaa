/* opensas reference solution for the yamaa benchmark adam-adpp-carryover-exclusion.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data pp;
  length STUDYID DOMAIN USUBJID PPTESTCD PPTEST PPCAT PPSTRESU PPSPEC VISIT PPRFTDTC $200;
  infile "/app/input/pp.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char200. DOMAIN : $char200. USUBJID : $char200. PPSEQ
        PPTESTCD : $char200. PPTEST : $char200. PPCAT : $char200. PPSTRESN
        PPSTRESU : $char200. PPSPEC : $char200. VISITNUM VISIT : $char200.
        PPRFTDTC : $char200.;
  ORD = _n_;
run;

data pc;
  length STUDYID DOMAIN USUBJID PCTESTCD PCTEST PCORRES PCORRESU PCSTRESC PCSTRESU
         PCSTAT VISIT PCDTC PCTPT PCRFTDTC $200;
  infile "/app/input/pc.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char200. DOMAIN : $char200. USUBJID : $char200. PCSEQ
        PCTESTCD : $char200. PCTEST : $char200. PCORRES : $char200.
        PCORRESU : $char200. PCSTRESC : $char200. PCSTRESN PCSTRESU : $char200.
        PCSTAT : $char200. PCLLOQ VISITNUM VISIT : $char200. PCDTC : $char200.
        PCTPT : $char200. PCTPTNUM PCRFTDTC : $char200.;
run;

/* The period's pre-dose sample: planned at PREDOSE, with the parameter's
   reference dose time. A below-limit result counts as 0; a sample not done
   has no result and gives no value. */
data predose;
  length PRFT $200;
  set pc;
  if PCTPT = 'PREDOSE';
  PRFT = PCRFTDTC;
  if index(PCSTRESC, '<') = 1 then PRECONC = 0;
  else PRECONC = PCSTRESN;
  keep STUDYID USUBJID PRFT PRECONC;
run;

/* The same subject's Cmax in the same period. */
data cmax;
  length CRFT $200;
  set pp;
  if PPTESTCD = 'CMAX';
  CRFT = PPRFTDTC;
  CMAXV = PPSTRESN;
  keep STUDYID USUBJID CRFT CMAXV;
run;

proc sql;
  create table withpre as
    select p.*, d.PRECONC
    from pp as p left join predose as d
      on p.STUDYID = d.STUDYID and p.USUBJID = d.USUBJID and p.PPRFTDTC = d.PRFT;
  create table withcmax as
    select p.*, c.CMAXV
    from withpre as p left join cmax as c
      on p.STUDYID = c.STUDYID and p.USUBJID = c.USUBJID and p.PPRFTDTC = c.CRFT;
quit;

/* Left out only when the percentage is known and above 5. */
data final;
  length PARAMCD PARAM $200 ANL01FL $1;
  set withcmax;
  PARAMCD = PPTESTCD;
  PARAM = PPTEST;
  AVAL = PPSTRESN;
  if not missing(PRECONC) and not missing(CMAXV) and CMAXV ne 0 then
    PREPCMAX = 100 * PRECONC / CMAXV;
  ANL01FL = 'Y';
  if not missing(PREPCMAX) and PREPCMAX > 5 then ANL01FL = ' ';
run;

proc sort data=final; by ORD; run;

proc sql;
  create table adpp as
    select STUDYID, USUBJID, VISITNUM, VISIT, PARAMCD, PARAM, AVAL, PRECONC,
           PREPCMAX, ANL01FL
    from final;
quit;
proc export data=adpp outfile="/app/output/adpp.csv" dbms=csv replace; run;
