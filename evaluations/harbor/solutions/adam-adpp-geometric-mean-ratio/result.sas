/* opensas reference solution for the yamaa benchmark adam-adpp-geometric-mean-ratio.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data pp;
  length STUDYID DOMAIN USUBJID PPTESTCD PPTEST PPSTRESU PPSTAT VISIT $40;
  infile "/app/input/pp.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. DOMAIN : $char40. USUBJID : $char40. PPSEQ
        PPTESTCD : $char40. PPTEST : $char40. PPSTRESN PPSTRESU : $char40.
        PPSTAT : $char40. VISITNUM VISIT : $char40.;
run;

data pc;
  length STUDYID DOMAIN USUBJID PCTESTCD PCTEST PCSTRESU VISIT PCTPT $40;
  infile "/app/input/pc.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. DOMAIN : $char40. USUBJID : $char40. PCSEQ
        PCTESTCD : $char40. PCTEST : $char40. PCSTRESN PCSTRESU : $char40.
        PCREPNUM VISITNUM VISIT : $char40. PCTPT : $char40. PCTPTNUM;
run;

/* The natural logarithm of each metabolite ratio above zero. */
data logs;
  set pp;
  if PPTESTCD = 'MRCMAX' and not missing(PPSTRESN) and PPSTRESN > 0;
  LOGRATIO = log(PPSTRESN);
run;

/* The replicate 2-hour concentrations and their mean per subject. */
data reps;
  set pc;
  if PCTPT = '2H' and not missing(PCSTRESN);
run;

proc sql;
  create table means as
    select STUDYID, USUBJID, mean(PCSTRESN) as M, count(*) as N
    from reps group by STUDYID, USUBJID;
  create table squares as
    select r.STUDYID, r.USUBJID, m.N,
           (r.PCSTRESN - m.M) * (r.PCSTRESN - m.M) as SQ
    from reps as r inner join means as m
      on m.STUDYID = r.STUDYID and m.USUBJID = r.USUBJID;
  create table subjects as
    select distinct STUDYID, USUBJID from pp
    union
    select distinct STUDYID, USUBJID from pc;
  create table geomean as
    select STUDYID, USUBJID, exp(mean(LOGRATIO)) as AVAL
    from logs group by STUDYID, USUBJID;
  create table sd as
    select STUDYID, USUBJID,
           case when N >= 2 then sqrt(sum(SQ) / (N - 1)) else . end as AVAL
    from squares group by STUDYID, USUBJID, N;
  create table final as
    select s.STUDYID, s.USUBJID, 'MRCMAXGM' as PARAMCD length=8,
           'Geometric Mean Metabolite Ratio Cmax' as PARAM length=40,
           g.AVAL, 'RATIO' as AVALU length=8
    from subjects as s left join geomean as g
      on g.STUDYID = s.STUDYID and g.USUBJID = s.USUBJID
    union all
    select s.STUDYID, s.USUBJID, 'C2HREPSD' as PARAMCD length=8,
           'Replicate SD of 2 H Concentration' as PARAM length=40,
           d.AVAL, 'ng/mL' as AVALU length=8
    from subjects as s left join sd as d
      on d.STUDYID = s.STUDYID and d.USUBJID = s.USUBJID
    order by 1, 2, 3;
quit;

proc export data=final outfile="/app/output/adpp.csv" dbms=csv replace; run;
