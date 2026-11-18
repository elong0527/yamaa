/* opensas reference solution for the yamaa benchmark adam-adppk-relative-times.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

options missing=' ';

data ex;
  infile "/app/input/ex.csv" dsd dlm=',' firstobs=2 truncover;
  length STUDYID $40 USUBJID $40 EXSEQ 8 EXTRT $40 EXDOSE 8 EXDOSU $20 EXSTDTC $32;
  input STUDYID $ USUBJID $ EXSEQ EXTRT $ EXDOSE EXDOSU $ EXSTDTC $;
run;

data pc;
  infile "/app/input/pc.csv" dsd dlm=',' firstobs=2 truncover;
  length STUDYID $40 USUBJID $40 PCSEQ 8 PCTESTCD $20 PCTPT $40 PCDTC $32;
  input STUDYID $ USUBJID $ PCSEQ PCTESTCD $ PCTPT $ PCDTC $;
run;

data events;
  length STUDYID $40 USUBJID $40 DTC $32;
  set ex(in=inex) pc(in=inpc);
  if inex then do;
    if not (EXDOSE > 0) then delete;
    DTC = EXSTDTC;
    EVID = 1;
    AMT = EXDOSE;
    SRCSEQ = EXSEQ;
  end;
  else do;
    DTC = PCDTC;
    EVID = 0;
    AMT = .;
    SRCSEQ = PCSEQ;
  end;
  DTC = compress(DTC, , 'kw');
  DTC = strip(DTC);
  _d = input(substr(DTC, 1, 10), yymmdd10.);
  _hh = 0; _mi = 0; _ss = 0;
  if length(DTC) >= 13 then _hh = input(substr(DTC, 12, 2), 2.);
  if length(DTC) >= 16 then _mi = input(substr(DTC, 15, 2), 2.);
  if length(DTC) >= 19 then _ss = input(substr(DTC, 18, 2), 2.);
  ADTMN = _d * 86400 + _hh * 3600 + _mi * 60 + _ss;
  keep STUDYID USUBJID EVID AMT SRCSEQ ADTMN;
run;

proc sort data=events;
  by USUBJID ADTMN EVID SRCSEQ;
run;

data first;
  set events;
  by USUBJID ADTMN EVID SRCSEQ;
  retain _done;
  if first.USUBJID then _done = 0;
  if EVID = 1 and _done = 0 then do;
    _done = 1;
    FANLN = ADTMN;
    FDOSE = AMT;
    output;
  end;
  keep USUBJID FANLN FDOSE;
run;

data adppk;
  merge events(in=a) first;
  by USUBJID;
  if a;
  length ADTM $19 FANLDTM $19;
  retain _prevn _prevamt;
  if first.USUBJID then do;
    ASEQ = 0;
    _prevn = .;
    _prevamt = .;
  end;
  ASEQ + 1;
  ADTM = put(datepart(ADTMN), yymmdd10.) || 'T' || put(timepart(ADTMN), tod8.);
  if FANLN ne . then do;
    FANLDTM = put(datepart(FANLN), yymmdd10.) || 'T' || put(timepart(FANLN), tod8.);
    AFRLT = (ADTMN - FANLN) / 3600;
  end;
  if EVID = 1 then do;
    APRLT = 0;
    DOSEA = AMT;
    _prevn = ADTMN;
    _prevamt = AMT;
  end;
  else do;
    if _prevn ne . then do;
      APRLT = (ADTMN - _prevn) / 3600;
      DOSEA = _prevamt;
    end;
    else do;
      APRLT = AFRLT;
      DOSEA = FDOSE;
    end;
  end;
  keep STUDYID USUBJID ASEQ ADTM EVID AMT FANLDTM AFRLT APRLT DOSEA;
run;

data adppk;
  retain STUDYID USUBJID ASEQ ADTM EVID AMT FANLDTM AFRLT APRLT DOSEA;
  set adppk;
run;

proc export data=adppk outfile="/app/output/adppk.csv" dbms=csv replace;
run;
