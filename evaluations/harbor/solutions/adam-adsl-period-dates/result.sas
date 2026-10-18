/* opensas reference solution for the yamaa benchmark adam-adsl-period-dates.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data dm;
  length STUDYID USUBJID $1024;
  infile "/app/input/dm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024.;
  ORDER=_n_;
run;

data ex;
  length STUDYID USUBJID EPOCH EXSTDTC EXENDTC $1024;
  infile "/app/input/ex.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. EXSEQ EPOCH : $char1024. EXSTDTC : $char1024.
        EXENDTC : $char1024.;
  STARTDT=.; ENDDT=.;
  if not missing(EXSTDTC) then STARTDT=input(EXSTDTC, yymmdd10.);
  if not missing(EXENDTC) then ENDDT=input(EXENDTC, yymmdd10.);
run;

/* The last exposure end counts every record, with or without a start. */
proc sql;
  create table p1 as select USUBJID, min(STARTDT) as P1 from ex where EPOCH='TREATMENT 1' group by USUBJID;
  create table p2 as select USUBJID, min(STARTDT) as P2 from ex where EPOCH='TREATMENT 2' group by USUBJID;
  create table lastend as select USUBJID, max(ENDDT) as LASTEND from ex group by USUBJID;
  create table subject as
  select d.STUDYID, d.USUBJID, d.ORDER, a.P1, b.P2, c.LASTEND
  from dm as d
  left join p1 as a on d.USUBJID=a.USUBJID
  left join p2 as b on d.USUBJID=b.USUBJID
  left join lastend as c on d.USUBJID=c.USUBJID
  order by d.ORDER;
quit;

/* Period one ends the day before period two starts; the last period a
   subject entered ends 30 days after the last exposure end. */
data final;
  length AP01SDT AP01EDT AP02SDT AP02EDT $10;
  set subject;
  AP01SDT=''; AP01EDT=''; AP02SDT=''; AP02EDT='';
  if not missing(P1) then AP01SDT=put(P1, yymmdd10.);
  if not missing(P2) then AP02SDT=put(P2, yymmdd10.);
  if not missing(LASTEND) then do;
    if not missing(P2) then do;
      AP01EDT=put(P2-1, yymmdd10.);
      AP02EDT=put(LASTEND+30, yymmdd10.);
    end;
    else AP01EDT=put(LASTEND+30, yymmdd10.);
  end;
  keep STUDYID USUBJID AP01SDT AP01EDT AP02SDT AP02EDT;
run;

proc sql;
  create table out as select STUDYID, USUBJID, AP01SDT, AP01EDT, AP02SDT, AP02EDT from final;
quit;
proc export data=out outfile="/app/output/adsl.csv" dbms=csv replace; run;
