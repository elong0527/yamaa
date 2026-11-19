/* opensas reference solution for the yamaa benchmark adam-adppk-event-records.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

options missing='';

data ex;
  infile '/app/input/ex.csv' dsd dlm=',' firstobs=2 truncover;
  length STUDYID $40 USUBJID $40 EXSEQ 8 EXTRT $40 EXDOSE 8 EXDOSU $20 EXSTDTC $25;
  input STUDYID $ USUBJID $ EXSEQ EXTRT $ EXDOSE EXDOSU $ EXSTDTC $;
run;

data pc;
  infile '/app/input/pc.csv' dsd dlm=',' firstobs=2 truncover;
  length STUDYID $40 USUBJID $40 PCSEQ 8 PCTESTCD $8 PCSTRESC $20 PCSTRESN 8
         PCSTRESU $20 PCSTAT $20 PCTPT $40 PCDTC $25;
  input STUDYID $ USUBJID $ PCSEQ PCTESTCD $ PCSTRESC $ PCSTRESN PCSTRESU $ PCSTAT $ PCTPT $ PCDTC $;
run;

data dose;
  set ex;
  where EXDOSE > 0;
  length ADTM $19 PARAMCD $8;
  ADTM = strip(compress(EXSTDTC, "0D"x));
  if length(ADTM) = 16 then ADTM = cats(ADTM, ':00');
  else if length(ADTM) = 13 then ADTM = cats(ADTM, ':00:00');
  else if length(ADTM) = 10 then ADTM = cats(ADTM, 'T00:00:00');
  EVID = 1;
  CMT = 1;
  PARAMCD = 'DOSE';
  AMT = EXDOSE;
  DV = .;
  MDV = 1;
  keep STUDYID USUBJID ADTM EVID CMT PARAMCD AMT DV MDV;
run;

proc sort data=dose(keep=USUBJID) out=dosed nodupkey;
  by USUBJID;
run;

proc sort data=pc out=pcs;
  by USUBJID;
run;

data conc;
  merge pcs(in=inpc) dosed(in=indose);
  by USUBJID;
  if inpc and indose;
  if upcase(strip(PCSTAT)) = 'NOT DONE' then delete;
  length ADTM $19 PARAMCD $8;
  ADTM = strip(compress(PCDTC, "0D"x));
  if length(ADTM) = 16 then ADTM = cats(ADTM, ':00');
  else if length(ADTM) = 13 then ADTM = cats(ADTM, ':00:00');
  else if length(ADTM) = 10 then ADTM = cats(ADTM, 'T00:00:00');
  EVID = 0;
  if PCTESTCD = 'DRGX' then CMT = 2;
  else if PCTESTCD = 'MET1' then CMT = 3;
  PARAMCD = PCTESTCD;
  AMT = .;
  DV = PCSTRESN;
  if missing(DV) then MDV = 1;
  else MDV = 0;
  keep STUDYID USUBJID ADTM EVID CMT PARAMCD AMT DV MDV;
run;

data events;
  set dose conc;
run;

proc sort data=events;
  by USUBJID ADTM EVID CMT;
run;

data adppk;
  retain STUDYID USUBJID ID ASEQ ADTM EVID CMT PARAMCD AMT DV MDV;
  set events;
  by USUBJID;
  if first.USUBJID then do;
    ID + 1;
    ASEQ = 0;
  end;
  ASEQ + 1;
run;

proc export data=adppk outfile='/app/output/adppk.csv' dbms=csv replace;
run;
