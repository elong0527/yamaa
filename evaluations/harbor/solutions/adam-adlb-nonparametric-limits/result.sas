/* opensas reference solution for the yamaa benchmark
   adam-adlb-nonparametric-limits.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas.
   The limits follow R's default definition (type 7), which none of the
   percentile definitions of the procedures gives, so a DATA step computes
   them from the sorted values. */

data lb;
  length STUDYID DOMAIN USUBJID LBTESTCD LBTEST LBSTRESU LBSTAT $40;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. DOMAIN : $char40. USUBJID : $char40. LBSEQ
        LBTESTCD : $char40. LBTEST : $char40. LBSTRESN LBSTRESU : $char40.
        LBSTAT : $char40.;
run;

data adsl;
  length STUDYID USUBJID SEX $40;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. USUBJID : $char40. SEX : $char40.;
run;

/* Each record with its subject's sex. */
proc sql;
  create table withsex as
    select l.STUDYID, l.USUBJID, a.SEX, l.LBTESTCD, l.LBSTRESN
    from lb as l left join adsl as a
      on a.STUDYID = l.STUDYID and a.USUBJID = l.USUBJID;
quit;

/* The results of each parameter and sex in ascending order, numbered from
   1; a test not done has none and takes no part. */
proc sort data=withsex(where=(not missing(LBSTRESN))) out=sorted;
  by LBTESTCD SEX LBSTRESN;
run;

data ranked;
  set sorted;
  by LBTESTCD SEX;
  if first.SEX then RANK = 0;
  RANK + 1;
  keep LBTESTCD SEX RANK LBSTRESN;
run;

proc sql;
  create table counts as
    select LBTESTCD, SEX, count(*) as N, mean(LBSTRESN) as M
    from ranked group by LBTESTCD, SEX;
quit;

/* Type 7: the quantile at proportion P sits at position H = (N - 1) * P + 1,
   between the values ranked J = floor(H) and K = J + 1, at fraction G. */
data positions;
  set counts;
  STAT = 1; P = 0.025; H = (N - 1) * P + 1; J = floor(H); G = H - J;
  K = min(J + 1, N); output;
  STAT = 2; P = 0.975; H = (N - 1) * P + 1; J = floor(H); G = H - J;
  K = min(J + 1, N); output;
run;

/* The limits, and the sample variance with divisor N - 1. */
proc sql;
  create table quantiles as
    select p.LBTESTCD, p.SEX, p.STAT,
           lo.LBSTRESN + p.G * (hi.LBSTRESN - lo.LBSTRESN) as Q
    from positions as p
      inner join ranked as lo
        on lo.LBTESTCD = p.LBTESTCD and lo.SEX = p.SEX and lo.RANK = p.J
      inner join ranked as hi
        on hi.LBTESTCD = p.LBTESTCD and hi.SEX = p.SEX and hi.RANK = p.K;
  create table squares as
    select r.LBTESTCD, r.SEX, c.N,
           (r.LBSTRESN - c.M) * (r.LBSTRESN - c.M) as SQ
    from ranked as r inner join counts as c
      on c.LBTESTCD = r.LBTESTCD and c.SEX = r.SEX;
  create table variances as
    select LBTESTCD, SEX,
           case when N > 1 then sum(SQ) / (N - 1) else . end as AVALVAR
    from squares group by LBTESTCD, SEX, N;
  create table limits as
    select q.LBTESTCD, q.SEX,
           max(case when q.STAT = 1 then q.Q else . end) as ANRLO,
           max(case when q.STAT = 2 then q.Q else . end) as ANRHI
    from quantiles as q group by q.LBTESTCD, q.SEX;
  create table final as
    select w.STUDYID, w.USUBJID, w.SEX, w.LBTESTCD as PARAMCD length=40,
           w.LBSTRESN as AVAL, s.ANRLO, s.ANRHI, v.AVALVAR
    from withsex as w
      left join limits as s on s.LBTESTCD = w.LBTESTCD and s.SEX = w.SEX
      left join variances as v on v.LBTESTCD = w.LBTESTCD and v.SEX = w.SEX
    order by 1, 2, 4;
quit;

proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
