/* opensas reference solution for the yamaa benchmark adam-adcm-end-date-bound.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data cm;
  length STUDYID USUBJID CMTRT CMENDTC $1024;
  infile "/app/input/cm.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. CMSEQ CMTRT : $char1024. CMENDTC : $char1024.;
run;

data adsl;
  length STUDYID USUBJID DTHDT $1024;
  infile "/app/input/adsl.csv" dsd dlm="," firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. DTHDT : $char1024.;
run;

proc sql;
  create table joined as select a.*, b.DTHDT
  from cm as a left join adsl as b on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID
  order by a.STUDYID, a.USUBJID, a.CMSEQ;
quit;

/* A full date stands as collected; a year and month runs to its last day,
   a year alone to December 31, never past a date of death that falls inside
   the collected month or year. When the whole collected month or year lies
   after death, the last day stands. */
data result;
  length AENDT $10 AENDTF $1;
  set joined;
  AENDT=''; AENDTF='';
  len=lengthn(CMENDTC);
  death=.;
  if not missing(DTHDT) then death=input(DTHDT, yymmdd10.);
  if len=10 then AENDT=CMENDTC;
  else if len in (4, 7) then do;
    if len=7 then do;
      first=input(cats(CMENDTC, '-01'), yymmdd10.);
      last=intnx('month', first, 0, 'end');
      flag='D';
    end;
    else do;
      first=mdy(1, 1, input(CMENDTC, 4.));
      last=mdy(12, 31, input(CMENDTC, 4.));
      flag='M';
    end;
    if not missing(death) then do;
      if death >= first then last=min(last, death);
    end;
    if not missing(last) then do;
      AENDT=put(last, yymmdd10.);
      AENDTF=flag;
    end;
  end;
run;

proc sql;
  create table final as select STUDYID, USUBJID, CMSEQ, CMTRT, CMENDTC, AENDT, AENDTF, DTHDT from result;
quit;
proc export data=final outfile="/app/output/adcm.csv" dbms=csv replace; run;
