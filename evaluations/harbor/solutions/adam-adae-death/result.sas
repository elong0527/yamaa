/* Reference solution for the yamaa benchmark adam-adae-death (SAS).

   Written from the benchmark's full prompt and its inputs alone, as an agent
   would write it. CI runs it with OpenSAS and checks that the grader scores
   it 1. */

data ae;
  infile "/app/input/ae.csv" dsd firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 AESEQ 8 AEDECOD $200 AEOUT $40 ASTDT 8;
  informat ASTDT yymmdd10.;
  input STUDYID USUBJID AESEQ AEDECOD AEOUT ASTDT;
run;

data dm;
  infile "/app/input/dm.csv" dsd firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 DTHDT 8;
  informat DTHDT yymmdd10.;
  input STUDYID USUBJID DTHDT;
run;

/* The fatal adverse event behind each death: the most recent one, and the
   highest AESEQ among those starting that day. */
proc sort data=ae(where=(AEOUT = "FATAL")) out=fatal;
  by USUBJID descending ASTDT descending AESEQ;
run;

data fatal;
  set fatal;
  by USUBJID;
  if first.USUBJID;
run;

/* One record per subject who died, from a fatal adverse event, DM, or both. */
proc sql;
  create table death as
  select coalescec(f.USUBJID, d.USUBJID) as USUBJID length=40,
         "Y" as DTHFL length=1,
         f.AEDECOD as DTHCAUS length=200,
         coalesce(f.ASTDT, d.DTHDT) as DTHDT
  from fatal as f
  full join (select USUBJID, DTHDT from dm where not missing(DTHDT)) as d
    on f.USUBJID = d.USUBJID;

  create table adae as
  select a.STUDYID, a.USUBJID, a.AESEQ, a.AEDECOD,
         case when missing(a.ASTDT) then "" else put(a.ASTDT, yymmdd10.) end
           as ASTDT,
         d.DTHFL, d.DTHCAUS,
         case when missing(d.DTHDT) then "" else put(d.DTHDT, yymmdd10.) end
           as DTHDT
  from ae as a
  left join death as d
    on a.USUBJID = d.USUBJID
  order by a.USUBJID, a.AESEQ;
quit;

proc export data=adae outfile="/app/output/adae.csv" dbms=csv replace;
run;
