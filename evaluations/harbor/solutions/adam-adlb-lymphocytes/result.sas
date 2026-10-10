/* SAS language reference solution for the yamaa benchmark adam-adlb-lymphocytes.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing openSAS runtime: sas /app/output/result.sas. */

data lb;
  length USUBJID LBTESTCD LBTEST VISIT $1024;
  infile "/app/input/lb.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input USUBJID : $char1024. LBTESTCD : $char1024. LBSTRESN LBTEST : $char1024. VISIT : $char1024.;
run;

proc sql; create table derived as
select a.USUBJID,'LYMPH' as PARAMCD,a.LBSTRESN*b.LBSTRESN as AVAL,'Lymphocytes Abs (10^9/L)' as PARAM,a.VISIT,'CALCULATION' as DTYPE from lb as a inner join lb as b on a.USUBJID=b.USUBJID and a.VISIT=b.VISIT left join lb as c on a.USUBJID=c.USUBJID and a.VISIT=c.VISIT and c.LBTESTCD='LYMPH' where a.LBTESTCD='LYMLE' and b.LBTESTCD='WBC' and not missing(a.LBSTRESN) and not missing(b.LBSTRESN) and missing(c.LBTESTCD)
union all
select a.USUBJID,'NEUT' as PARAMCD,a.LBSTRESN*b.LBSTRESN as AVAL,'Neutrophils Abs (10^9/L)' as PARAM,a.VISIT,'CALCULATION' as DTYPE from lb as a inner join lb as b on a.USUBJID=b.USUBJID and a.VISIT=b.VISIT left join lb as c on a.USUBJID=c.USUBJID and a.VISIT=c.VISIT and c.LBTESTCD='NEUT' where a.LBTESTCD='NEUTLE' and b.LBTESTCD='WBC' and not missing(a.LBSTRESN) and not missing(b.LBSTRESN) and missing(c.LBTESTCD)
union all
select a.USUBJID,'MONO' as PARAMCD,a.LBSTRESN*b.LBSTRESN as AVAL,'Monocytes Abs (10^9/L)' as PARAM,a.VISIT,'CALCULATION' as DTYPE from lb as a inner join lb as b on a.USUBJID=b.USUBJID and a.VISIT=b.VISIT left join lb as c on a.USUBJID=c.USUBJID and a.VISIT=c.VISIT and c.LBTESTCD='MONO' where a.LBTESTCD='MONOLE' and b.LBTESTCD='WBC' and not missing(a.LBSTRESN) and not missing(b.LBSTRESN) and missing(c.LBTESTCD)
union all
select a.USUBJID,'EOS' as PARAMCD,a.LBSTRESN*b.LBSTRESN as AVAL,'Eosinophils Abs (10^9/L)' as PARAM,a.VISIT,'CALCULATION' as DTYPE from lb as a inner join lb as b on a.USUBJID=b.USUBJID and a.VISIT=b.VISIT left join lb as c on a.USUBJID=c.USUBJID and a.VISIT=c.VISIT and c.LBTESTCD='EOS' where a.LBTESTCD='EOSLE' and b.LBTESTCD='WBC' and not missing(a.LBSTRESN) and not missing(b.LBSTRESN) and missing(c.LBTESTCD)
union all
select a.USUBJID,'BASO' as PARAMCD,a.LBSTRESN*b.LBSTRESN as AVAL,'Basophils Abs (10^9/L)' as PARAM,a.VISIT,'CALCULATION' as DTYPE from lb as a inner join lb as b on a.USUBJID=b.USUBJID and a.VISIT=b.VISIT left join lb as c on a.USUBJID=c.USUBJID and a.VISIT=c.VISIT and c.LBTESTCD='BASO' where a.LBTESTCD='BASOLE' and b.LBTESTCD='WBC' and not missing(a.LBSTRESN) and not missing(b.LBSTRESN) and missing(c.LBTESTCD); quit;
data collected; length PARAMCD $16 PARAM $1024 DTYPE $11; set lb; PARAMCD=LBTESTCD; PARAM=LBTEST; AVAL=LBSTRESN; run;
data result; set collected derived; run;

proc sql;
  create table final as select USUBJID, PARAMCD, AVAL, PARAM, VISIT, DTYPE from result;
quit;
proc export data=final outfile="/app/output/adlb.csv" dbms=csv replace; run;
