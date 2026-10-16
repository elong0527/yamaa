/* opensas reference solution for the yamaa benchmark adam-adpc-relative-times.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data pc;
  length STUDYID USUBJID PCTESTCD PCTEST PCTPT PCDTC $1024;
  infile "/app/input/pc.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. PCSEQ PCTESTCD : $char1024. PCTEST : $char1024.
        PCTPT : $char1024. PCDTC : $char1024.;
run;

data ex;
  length STUDYID USUBJID EXTRT EXDOSU EXSTDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ EXTRT : $char1024. EXDOSE EXDOSU : $char1024.
        EXSTDTC : $char1024.;
run;

/* A date without a time is not a moment; a time without seconds counts as
   second zero. Clock readings, no time zone. */
data doses;
  set ex;
  length DRUG $1024;
  DRUG=EXTRT;
  if lengthn(EXSTDTC)>10 then START=input(cats(EXSTDTC, ifc(lengthn(EXSTDTC)=16, ':00', '')), e8601dt.);
  if not missing(START);
  keep USUBJID DRUG START;
run;

data samples;
  set pc;
  length DRUG $1024;
  if PCTESTCD in ('DRGX', 'MET1') then DRUG='DRUG X';
  else if PCTESTCD='DRGY' then DRUG='DRUG Y';
  if lengthn(PCDTC)>10 then ADTMN=input(cats(PCDTC, ifc(lengthn(PCDTC)=16, ':00', '')), e8601dt.);
run;

proc sql;
  create table first as select USUBJID, DRUG, min(START) as FIRSTN
    from doses group by USUBJID, DRUG;
  create table latest as select s.USUBJID, s.PCSEQ, max(d.START) as LATESTN
    from samples as s inner join doses as d on s.USUBJID=d.USUBJID and s.DRUG=d.DRUG
    where not missing(s.ADTMN) and d.START <= s.ADTMN
    group by s.USUBJID, s.PCSEQ;
  create table joined as select s.*, f.FIRSTN, l.LATESTN
    from samples as s
    left join first as f on s.USUBJID=f.USUBJID and s.DRUG=f.DRUG
    left join latest as l on s.USUBJID=l.USUBJID and s.PCSEQ=l.PCSEQ
    order by s.USUBJID, s.PCSEQ;
quit;

data result;
  length PARAMCD ATPT $1024 ADTM FANLDTM PCRFTDTM $19;
  set joined;
  PARAMCD=PCTESTCD; ATPT=PCTPT;
  ADTM=''; FANLDTM=''; PCRFTDTM='';
  /* A sample taken before every dose refers to the first dose. */
  REFN=.;
  if not missing(ADTMN) and not missing(FIRSTN) then REFN=coalesce(LATESTN, FIRSTN);
  if not missing(ADTMN) then ADTM=put(ADTMN, e8601dt19.);
  if not missing(FIRSTN) then FANLDTM=put(FIRSTN, e8601dt19.);
  if not missing(REFN) then PCRFTDTM=put(REFN, e8601dt19.);
  AFRLT=.; ARRLT=.;
  format AFRLT ARRLT best32.;
  if not missing(ADTMN) and not missing(FIRSTN) then AFRLT=(ADTMN-FIRSTN)/3600;
  if not missing(ADTMN) and not missing(REFN) then ARRLT=(ADTMN-REFN)/3600;
run;

proc sql;
  create table final as select STUDYID, USUBJID, PCSEQ, PARAMCD, ATPT, ADTM, FANLDTM, PCRFTDTM, AFRLT, ARRLT
    from result;
quit;
proc export data=final outfile="/app/output/adpc.csv" dbms=csv replace; run;
