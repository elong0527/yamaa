/* opensas reference solution for the yamaa benchmark adam-adppk-dose-intervals.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adppk;
  infile "/app/input/ex.csv" dsd dlm=',' firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 EXSEQ 8 EXTRT $40 EXDOSE 8 EXDOSU $20
         EXDOSFRQ $20 EXSTDTC $30 EXENDTC $30
         ADTM $19 ATMF $1 EVID 8 AMT 8 II 8 ADDL 8;
  input STUDYID $ USUBJID $ EXSEQ EXTRT $ EXDOSE EXDOSU $ EXDOSFRQ $ EXSTDTC $ EXENDTC $;
  EXSTDTC = strip(compress(EXSTDTC, '0D'x));
  EXENDTC = strip(compress(EXENDTC, '0D'x));
  EXDOSFRQ = upcase(strip(compress(EXDOSFRQ, '0D'x)));
  if EXDOSE > 0;

  stdt = input(substr(EXSTDTC, 1, 10), yymmdd10.);
  if index(EXSTDTC, 'T') = 11 and length(EXSTDTC) >= 16 then do;
    sth = input(substr(EXSTDTC, 12, 2), 2.);
    stm = input(substr(EXSTDTC, 15, 2), 2.);
    ATMF = ' ';
  end;
  else do;
    sth = 0;
    stm = 0;
    ATMF = 'H';
  end;
  stdtm = stdt * 86400 + sth * 3600 + stm * 60;
  ADTM = substr(EXSTDTC, 1, 10) || 'T' || put(sth, z2.) || ':' || put(stm, z2.) || ':00';

  EVID = 1;
  AMT = EXDOSE;

  if EXDOSFRQ = 'QD' then step = 24;
  else if EXDOSFRQ = 'BID' then step = 12;
  else step = 0;

  ADDL = 0;
  if step > 0 and EXENDTC ne ' ' then do;
    endt = input(substr(EXENDTC, 1, 10), yymmdd10.);
    if index(EXENDTC, 'T') = 11 and length(EXENDTC) >= 16 then
      endtm = endt * 86400 + input(substr(EXENDTC, 12, 2), 2.) * 3600
              + input(substr(EXENDTC, 15, 2), 2.) * 60;
    else endtm = endt * 86400 + 86399;
    if endtm >= stdtm then ADDL = floor((endtm - stdtm) / (step * 3600));
  end;
  if ADDL > 0 then II = step;
  else II = 0;

  keep STUDYID USUBJID EXSEQ ADTM ATMF EVID AMT II ADDL;
run;

data adppk;
  retain STUDYID USUBJID EXSEQ ADTM ATMF EVID AMT II ADDL;
  set adppk;
run;

proc export data=adppk outfile="/app/output/adppk.csv" dbms=csv replace;
run;
