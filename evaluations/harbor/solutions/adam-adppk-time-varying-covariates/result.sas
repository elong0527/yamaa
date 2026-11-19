/* opensas reference solution for the yamaa benchmark adam-adppk-time-varying-covariates.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

options missing=' ';

data adsl;
  infile '/app/input/adsl.csv' dsd dlm=',' firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 AGE 8 AGEU $10 SEX $1;
  input STUDYID $ USUBJID $ AGE AGEU $ SEX $;
  SEX = compress(SEX, "0D"x);
run;

data ex;
  infile '/app/input/ex.csv' dsd dlm=',' firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 EXSEQ 8 EXTRT $40 EXDOSE 8 EXDOSU $10 EXSTDTC $19;
  input STUDYID $ USUBJID $ EXSEQ EXTRT $ EXDOSE EXDOSU $ EXSTDTC $;
  EXSTDTC = compress(EXSTDTC, "0D"x);
run;

data pc;
  infile '/app/input/pc.csv' dsd dlm=',' firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 PCSEQ 8 PCTESTCD $8 PCTPT $40 PCDTC $19;
  input STUDYID $ USUBJID $ PCSEQ PCTESTCD $ PCTPT $ PCDTC $;
  PCDTC = compress(PCDTC, "0D"x);
run;

data vs;
  infile '/app/input/vs.csv' dsd dlm=',' firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 VSSEQ 8 VSTESTCD $8 VSSTRESN 8 VSSTRESU $10 VSSTAT $10 VISIT $40 VSDTC $19;
  input STUDYID $ USUBJID $ VSSEQ VSTESTCD $ VSSTRESN VSSTRESU $ VSSTAT $ VISIT $ VSDTC $;
  VSDTC = compress(VSDTC, "0D"x);
run;

data lb;
  infile '/app/input/lb.csv' dsd dlm=',' firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 LBSEQ 8 LBTESTCD $8 LBSTRESN 8 LBSTRESU $10 VISIT $40 LBDTC $19;
  input STUDYID $ USUBJID $ LBSEQ LBTESTCD $ LBSTRESN LBSTRESU $ VISIT $ LBDTC $;
  LBDTC = compress(LBDTC, "0D"x);
run;

/* one record per administration and per sample */
data events;
  length STUDYID $20 USUBJID $40 ADTM $19 ADT $10 EVID 8 SRCSEQ 8 DTC $19;
  keep STUDYID USUBJID ADTM ADT EVID SRCSEQ;
  set ex(in=inex) pc(in=inpc);
  if inex then do; EVID = 1; DTC = strip(EXSTDTC); SRCSEQ = EXSEQ; end;
  else do; EVID = 0; DTC = strip(PCDTC); SRCSEQ = PCSEQ; end;
  if length(DTC) = 10 then ADTM = cats(DTC, 'T00:00:00');
  else if length(DTC) = 13 then ADTM = cats(DTC, ':00:00');
  else if length(DTC) = 16 then ADTM = cats(DTC, ':00');
  else ADTM = DTC;
  ADT = substr(ADTM, 1, 10);
run;

proc sort data=events;
  by STUDYID USUBJID ADTM EVID SRCSEQ;
run;

data events;
  set events;
  by STUDYID USUBJID;
  if first.USUBJID then ASEQ = 0;
  ASEQ + 1;
run;

/* covariate results that carry a value */
data wt;
  set vs;
  where VSTESTCD = 'WEIGHT' and VSSTRESN is not missing;
  length CDT $10;
  CDT = substr(VSDTC, 1, 10);
  keep STUDYID USUBJID CDT VSDTC VSSEQ VSSTRESN;
run;

data creat;
  set lb;
  where LBTESTCD = 'CREAT' and LBSTRESN is not missing;
  length CDT $10;
  CDT = substr(LBDTC, 1, 10);
  keep STUDYID USUBJID CDT LBDTC LBSEQ LBSTRESN;
run;

proc sql;
  create table ewt as
  select e.STUDYID, e.USUBJID, e.ASEQ, w.VSDTC, w.VSSEQ, w.VSSTRESN
  from events as e left join wt as w
    on e.STUDYID = w.STUDYID and e.USUBJID = w.USUBJID and w.CDT <= e.ADT
  order by e.STUDYID, e.USUBJID, e.ASEQ, w.VSDTC, w.VSSEQ;

  create table ecr as
  select e.STUDYID, e.USUBJID, e.ASEQ, c.LBDTC, c.LBSEQ, c.LBSTRESN
  from events as e left join creat as c
    on e.STUDYID = c.STUDYID and e.USUBJID = c.USUBJID and c.CDT <= e.ADT
  order by e.STUDYID, e.USUBJID, e.ASEQ, c.LBDTC, c.LBSEQ;
quit;

data ewt;
  set ewt;
  by STUDYID USUBJID ASEQ;
  if last.ASEQ;
  WT = VSSTRESN;
  keep STUDYID USUBJID ASEQ WT;
run;

data ecr;
  set ecr;
  by STUDYID USUBJID ASEQ;
  if last.ASEQ;
  CREAT = LBSTRESN;
  keep STUDYID USUBJID ASEQ CREAT;
run;

proc sort data=adsl out=adsl_s(keep=STUDYID USUBJID AGE SEX);
  by STUDYID USUBJID;
run;

data adppk0;
  merge events(in=ine) ewt ecr;
  by STUDYID USUBJID ASEQ;
  if ine;
run;

data adppk;
  retain STUDYID USUBJID ASEQ ADTM EVID WT CREAT CRCL;
  merge adppk0(in=ine) adsl_s;
  by STUDYID USUBJID;
  if ine;
  CRCL = .;
  if not missing(WT) and not missing(CREAT) and not missing(AGE) then do;
    if SEX = 'M' then CRCL = (140 - AGE) * WT * 1.23 / CREAT;
    else if SEX = 'F' then CRCL = (140 - AGE) * WT * 1.04 / CREAT;
  end;
  keep STUDYID USUBJID ASEQ ADTM EVID WT CREAT CRCL;
run;

proc sort data=adppk;
  by STUDYID USUBJID ASEQ;
run;

/* CSV written with full numeric precision; a missing value is an empty field */
data _null_;
  set adppk;
  file '/app/output/adppk.csv';
  length line $400 cwt ccreat ccrcl $40;
  if _n_ = 1 then put 'STUDYID,USUBJID,ASEQ,ADTM,EVID,WT,CREAT,CRCL';
  if missing(WT) then cwt = ''; else cwt = strip(put(WT, best32.));
  if missing(CREAT) then ccreat = ''; else ccreat = strip(put(CREAT, best32.));
  if missing(CRCL) then ccrcl = ''; else ccrcl = strip(put(CRCL, best32.));
  line = catx(',', STUDYID, USUBJID, strip(put(ASEQ, best32.)), ADTM, strip(put(EVID, best32.)));
  line = strip(line) || ',' || strip(cwt) || ',' || strip(ccreat) || ',' || strip(ccrcl);
  put line;
run;
