/* opensas reference solution for the yamaa benchmark adam-adlb-reference-limits.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data lb;
  length STUDYID DOMAIN USUBJID LBTESTCD LBTEST LBSTRESU LBSTAT $40;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. DOMAIN : $char40. USUBJID : $char40. LBSEQ
        LBTESTCD : $char40. LBTEST : $char40. LBSTRESN LBSTRESU : $char40.
        LBSTAT : $char40.;
run;

/* The results of each parameter; a test not done has none and takes no
   part. */
data results;
  set lb;
  if not missing(LBSTRESN);
run;

/* Each parameter's mean and sample standard deviation (divisor n - 1), and
   its limits at the mean less and plus the standard normal 0.975 quantile,
   probit(0.975) = 1.959963984540054, times that deviation. */
proc sql;
  create table means as
    select LBTESTCD, mean(LBSTRESN) as M, count(*) as N
    from results group by LBTESTCD;
  create table squares as
    select r.LBTESTCD, m.M, m.N, (r.LBSTRESN - m.M) * (r.LBSTRESN - m.M) as SQ
    from results as r inner join means as m on m.LBTESTCD = r.LBTESTCD;
  create table limits as
    select LBTESTCD,
           M - 1.959963984540054 * sqrt(sum(SQ) / (N - 1)) as ANRLO,
           M + 1.959963984540054 * sqrt(sum(SQ) / (N - 1)) as ANRHI
    from squares group by LBTESTCD, M, N;
  create table final as
    select l.STUDYID, l.USUBJID, l.LBTESTCD as PARAMCD length=40,
           l.LBSTRESN as AVAL, s.ANRLO, s.ANRHI
    from lb as l left join limits as s on s.LBTESTCD = l.LBTESTCD
    order by 1, 2, 3;
quit;

proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
