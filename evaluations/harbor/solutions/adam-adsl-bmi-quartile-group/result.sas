/* opensas reference solution for the yamaa benchmark adam-adsl-bmi-quartile-group.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas.
   The quartiles follow R's default definition (type 7), which none of the
   percentile definitions of the procedures gives, so a DATA step computes
   them from the sorted values. */

data adsl;
  length STUDYID USUBJID $40;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char40. USUBJID : $char40. BMIBL;
run;

/* The baseline values of each study in ascending order, numbered from 1. */
proc sort data=adsl(where=(not missing(BMIBL))) out=sorted;
  by STUDYID BMIBL;
run;

data ranked;
  set sorted;
  by STUDYID;
  if first.STUDYID then RANK = 0;
  RANK + 1;
  keep STUDYID RANK BMIBL;
run;

proc sql;
  create table counts as
    select STUDYID, count(*) as N from ranked group by STUDYID;
quit;

/* Type 7: the quantile at proportion P sits at position H = (N - 1) * P + 1,
   between the values ranked J = floor(H) and K = J + 1, at fraction G. */
data positions;
  set counts;
  STAT = 1; P = 0.25; H = (N - 1) * P + 1; J = floor(H); G = H - J;
  K = min(J + 1, N); output;
  STAT = 2; P = 0.5; H = (N - 1) * P + 1; J = floor(H); G = H - J;
  K = min(J + 1, N); output;
  STAT = 3; P = 0.75; H = (N - 1) * P + 1; J = floor(H); G = H - J;
  K = min(J + 1, N); output;
run;

proc sql;
  create table quartiles as
    select p.STUDYID, p.STAT, lo.BMIBL + p.G * (hi.BMIBL - lo.BMIBL) as Q
    from positions as p
      inner join ranked as lo on lo.STUDYID = p.STUDYID and lo.RANK = p.J
      inner join ranked as hi on hi.STUDYID = p.STUDYID and hi.RANK = p.K;
  create table cutpoints as
    select STUDYID,
           max(case when STAT = 1 then Q else . end) as BMIBLQ1,
           max(case when STAT = 2 then Q else . end) as BMIBLMED,
           max(case when STAT = 3 then Q else . end) as BMIBLQ3
    from quartiles group by STUDYID;
  create table joined as
    select a.STUDYID, a.USUBJID, a.BMIBL, c.BMIBLQ1, c.BMIBLMED, c.BMIBLQ3
    from adsl as a left join cutpoints as c on c.STUDYID = a.STUDYID;
quit;

data grouped;
  set joined;
  length BMIBLGR1 $10;
  if missing(BMIBL) then BMIBLGR1 = '';
  else if BMIBL <= BMIBLQ1 then BMIBLGR1 = 'Quartile 1';
  else if BMIBL <= BMIBLMED then BMIBLGR1 = 'Quartile 2';
  else if BMIBL <= BMIBLQ3 then BMIBLGR1 = 'Quartile 3';
  else BMIBLGR1 = 'Quartile 4';
run;

proc sql;
  create table final as
    select STUDYID, USUBJID, BMIBL, BMIBLQ1, BMIBLMED, BMIBLQ3, BMIBLGR1
    from grouped order by STUDYID, USUBJID;
quit;
proc export data=final outfile="/app/output/adsl.csv" dbms=csv replace; run;
