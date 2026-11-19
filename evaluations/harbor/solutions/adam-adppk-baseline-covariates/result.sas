/* opensas reference solution for the yamaa benchmark adam-adppk-baseline-covariates.
   Reads the benchmark inputs and derives the requested output.
   Execute with the existing opensas runtime: opensas /app/output/result.sas. */

data adsl;
  infile "/app/input/adsl.csv" dsd dlm=',' firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 AGE 8 AGEU $10 SEX $1 RACE $60;
  input STUDYID $ USUBJID $ AGE AGEU $ SEX $ RACE $;
  RACE = compress(RACE, "0D"x);
run;

data ex;
  infile "/app/input/ex.csv" dsd dlm=',' firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 EXSEQ 8 EXTRT $40 EXDOSE 8 EXDOSU $10 EXSTDTC $25;
  input STUDYID $ USUBJID $ EXSEQ EXTRT $ EXDOSE EXDOSU $ EXSTDTC $;
  EXSTDTC = compress(EXSTDTC, "0D"x);
run;

data pc;
  infile "/app/input/pc.csv" dsd dlm=',' firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 PCSEQ 8 PCTESTCD $8 PCTPT $20 PCDTC $25;
  input STUDYID $ USUBJID $ PCSEQ PCTESTCD $ PCTPT $ PCDTC $;
  PCDTC = compress(PCDTC, "0D"x);
run;

data lb;
  infile "/app/input/lb.csv" dsd dlm=',' firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 LBSEQ 8 LBTESTCD $8 LBSTRESN 8 LBSTRESU $10 LBBLFL $1 VISIT $20 LBDTC $25;
  input STUDYID $ USUBJID $ LBSEQ LBTESTCD $ LBSTRESN LBSTRESU $ LBBLFL $ VISIT $ LBDTC $;
run;

data vs;
  infile "/app/input/vs.csv" dsd dlm=',' firstobs=2 truncover;
  length STUDYID $20 USUBJID $40 VSSEQ 8 VSTESTCD $8 VSSTRESN 8 VSSTRESU $10 VSBLFL $1 VISIT $20 VSDTC $25;
  input STUDYID $ USUBJID $ VSSEQ VSTESTCD $ VSSTRESN VSSTRESU $ VSBLFL $ VISIT $ VSDTC $;
run;

proc sort data=vs out=vsb;
  by STUDYID USUBJID VSSEQ;
  where VSBLFL = "Y" and VSTESTCD in ("WEIGHT", "HEIGHT");
run;

data vsbl(keep=STUDYID USUBJID WTBL HTBL);
  set vsb;
  by STUDYID USUBJID;
  retain WTBL HTBL;
  if first.USUBJID then do; WTBL = .; HTBL = .; end;
  if VSTESTCD = "WEIGHT" then WTBL = VSSTRESN;
  else if VSTESTCD = "HEIGHT" then HTBL = VSSTRESN;
  if last.USUBJID then output;
run;

proc sort data=lb out=lbb;
  by STUDYID USUBJID LBSEQ;
  where LBBLFL = "Y" and LBTESTCD in ("CREAT", "ALT", "BILI");
run;

data lbbl(keep=STUDYID USUBJID CREATBL ALTBL TBILBL);
  set lbb;
  by STUDYID USUBJID;
  retain CREATBL ALTBL TBILBL;
  if first.USUBJID then do; CREATBL = .; ALTBL = .; TBILBL = .; end;
  if LBTESTCD = "CREAT" then CREATBL = LBSTRESN;
  else if LBTESTCD = "ALT" then ALTBL = LBSTRESN;
  else if LBTESTCD = "BILI" then TBILBL = LBSTRESN;
  if last.USUBJID then output;
run;

proc sort data=adsl; by STUDYID USUBJID; run;

data cov;
  merge adsl(in=a keep=STUDYID USUBJID AGE SEX RACE) vsbl lbbl;
  by STUDYID USUBJID;
  if a;
  length SEXN RACEN BMIBL BSABL CRCLBL EGFRBL 8;
  if SEX = "M" then SEXN = 1;
  else if SEX = "F" then SEXN = 2;
  if RACE = "ASIAN" then RACEN = 2;
  else if RACE = "BLACK OR AFRICAN AMERICAN" then RACEN = 3;
  else if RACE = "WHITE" then RACEN = 5;
  else RACEN = 6;
  if not missing(WTBL) and not missing(HTBL) then do;
    BMIBL = WTBL / ((HTBL / 100) ** 2);
    BSABL = sqrt(HTBL * WTBL / 3600);
  end;
  if not missing(AGE) and not missing(WTBL) and not missing(CREATBL) then do;
    if SEX = "M" then CRCLBL = (140 - AGE) * WTBL * 1.23 / CREATBL;
    else if SEX = "F" then CRCLBL = (140 - AGE) * WTBL * 1.04 / CREATBL;
  end;
  if not missing(AGE) and not missing(CREATBL) then do;
    if SEX = "M" then do;
      r = CREATBL / 88.42 / 0.9;
      if r > 1 then ex_a = -1.2; else ex_a = -0.302;
      EGFRBL = 142 * (r ** ex_a) * (0.9938 ** AGE);
    end;
    else if SEX = "F" then do;
      r = CREATBL / 88.42 / 0.7;
      if r > 1 then ex_a = -1.2; else ex_a = -0.241;
      EGFRBL = 142 * (r ** ex_a) * (0.9938 ** AGE) * 1.012;
    end;
  end;
  drop r ex_a;
run;

data events(keep=STUDYID USUBJID ADTM EVID SRCSEQ);
  length STUDYID $20 USUBJID $40 ADTM $19 EVID 8 SRCSEQ 8 dtc $25;
  set ex(in=inex keep=STUDYID USUBJID EXSEQ EXSTDTC) pc(keep=STUDYID USUBJID PCSEQ PCDTC);
  if inex then do; EVID = 1; dtc = EXSTDTC; SRCSEQ = EXSEQ; end;
  else do; EVID = 0; dtc = PCDTC; SRCSEQ = PCSEQ; end;
  dtc = strip(dtc);
  if length(dtc) = 16 then ADTM = cats(dtc, ":00");
  else ADTM = dtc;
run;

proc sort data=events;
  by STUDYID USUBJID ADTM descending EVID SRCSEQ;
run;

data adppk;
  retain STUDYID USUBJID ASEQ ADTM EVID AGE SEX SEXN RACE RACEN WTBL HTBL
         BMIBL BSABL CREATBL ALTBL TBILBL CRCLBL EGFRBL;
  merge events(in=e) cov;
  by STUDYID USUBJID;
  if e;
  if first.USUBJID then ASEQ = 0;
  ASEQ + 1;
  keep STUDYID USUBJID ASEQ ADTM EVID AGE SEX SEXN RACE RACEN WTBL HTBL
       BMIBL BSABL CREATBL ALTBL TBILBL CRCLBL EGFRBL;
run;

data _null_;
  set adppk;
  file "/app/output/adppk.csv";
  length line $600 v $40;
  array nums{14} ASEQ EVID AGE SEXN RACEN WTBL HTBL BMIBL BSABL CREATBL ALTBL TBILBL CRCLBL EGFRBL;
  array txt{14} $40 t1-t14;
  do i = 1 to 14;
    if missing(nums{i}) then txt{i} = "";
    else do;
      v = strip(put(nums{i}, best32.));
      if index(v, "E") > 0 then v = strip(put(nums{i}, best16.));
      txt{i} = v;
    end;
  end;
  if _n_ = 1 then put "STUDYID,USUBJID,ASEQ,ADTM,EVID,AGE,SEX,SEXN,RACE,RACEN,WTBL,HTBL,BMIBL,BSABL,CREATBL,ALTBL,TBILBL,CRCLBL,EGFRBL";
  line = trimn(STUDYID) || "," || trimn(USUBJID) || "," || trimn(t1) || "," || trimn(ADTM) || "," ||
         trimn(t2) || "," || trimn(t3) || "," || trimn(SEX) || "," || trimn(t4) || "," ||
         trimn(RACE) || "," || trimn(t5);
  do i = 6 to 14;
    line = trimn(line) || "," || trimn(txt{i});
  end;
  put line;
run;
