/* opensas reference solution for the yamaa benchmark adam-adppk-blq-methods.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

proc import datafile="/app/input/ex.csv" out=ex dbms=csv replace;
  getnames=yes;
  guessingrows=max;
run;

proc import datafile="/app/input/pc.csv" out=pc dbms=csv replace;
  getnames=yes;
  guessingrows=max;
run;

data dose;
  length STUDYID $20 USUBJID $40 ADTM $19;
  set ex(keep=STUDYID USUBJID EXDOSE EXSTDTC);
  ADTM = strip(EXSTDTC);
  if length(ADTM) = 16 then ADTM = cats(ADTM, ":00");
  EVID = 1;
  AMT = EXDOSE;
  ALLOQ = .;
  BLQ = 0;
  RESN = .;
  keep STUDYID USUBJID ADTM EVID AMT ALLOQ BLQ RESN;
run;

data conc;
  length STUDYID $20 USUBJID $40 ADTM $19;
  set pc(keep=STUDYID USUBJID PCSTRESC PCSTRESN PCSTAT PCLLOQ PCDTC);
  if upcase(strip(PCSTAT)) = "NOT DONE" then delete;
  ADTM = strip(PCDTC);
  if length(ADTM) = 16 then ADTM = cats(ADTM, ":00");
  EVID = 0;
  AMT = .;
  ALLOQ = PCLLOQ;
  BLQ = 0;
  if missing(PCSTRESN) and substr(strip(PCSTRESC), 1, 1) = "<" then BLQ = 1;
  RESN = PCSTRESN;
  keep STUDYID USUBJID ADTM EVID AMT ALLOQ BLQ RESN;
run;

data all;
  set conc dose;
run;

proc sort data=all;
  by STUDYID USUBJID ADTM EVID;
run;

data adppk;
  retain STUDYID USUBJID ASEQ ADTM EVID AMT ALLOQ CENS DV MDV DVM6 MDVM6;
  set all;
  by STUDYID USUBJID;
  retain PREVQ;
  if first.USUBJID then do;
    ASEQ = 0;
    PREVQ = 0;
  end;
  ASEQ + 1;
  CENS = 0;
  DV = .;
  DVM6 = .;
  if EVID = 0 then do;
    if BLQ = 1 then do;
      CENS = 1;
      DV = ALLOQ;
      if PREVQ = 1 then DVM6 = ALLOQ / 2;
      PREVQ = 0;
    end;
    else do;
      DV = RESN;
      DVM6 = RESN;
      if not missing(RESN) then PREVQ = 1;
      else PREVQ = 0;
    end;
  end;
  MDV = missing(DV);
  MDVM6 = missing(DVM6);
  keep STUDYID USUBJID ASEQ ADTM EVID AMT ALLOQ CENS DV MDV DVM6 MDVM6;
run;

proc export data=adppk outfile="/app/output/adppk.csv" dbms=csv replace;
run;
