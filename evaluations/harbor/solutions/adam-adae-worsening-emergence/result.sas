/* opensas reference for yamaa's adam-adae-worsening-emergence benchmark.
   Construct periods from one AE and its collected, dated FA findings. */
data ae;
  length STUDYID USUBJID AELNKID AEDECOD AESTDTC AEENDTC AESEV AETOXGR $1024;
  infile '/app/input/ae.csv' dsd dlm=',' firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. AESEQ
    AELNKID : $char1024. AEDECOD : $char1024. AESTDTC : $char1024.
    AEENDTC : $char1024. AESEV : $char1024. AETOXGR : $char1024.;
  if lengthn(AESTDTC)=16 then AESTDTC=cats(AESTDTC,':00');
  if lengthn(AEENDTC)=16 then AEENDTC=cats(AEENDTC,':00');
run;
data fa;
  length STUDYID USUBJID FALNKID FATESTCD FAOBJ FASTRESC FADTC $1024;
  infile '/app/input/fa.csv' dsd dlm=',' firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. FASEQ
    FALNKID : $char1024. FATESTCD : $char1024. FAOBJ : $char1024.
    FASTRESC : $char1024. FADTC : $char1024.;
  if lengthn(FADTC)=16 then FADTC=cats(FADTC,':00');
  SEVRANK=.;
  if FATESTCD='SEV' then do;
    select(FASTRESC);
      when('MILD') SEVRANK=1; when('MODERATE') SEVRANK=2;
      when('SEVERE') SEVRANK=3; otherwise SEVRANK=.;
    end;
  end;
run;
data adsl;
  length STUDYID USUBJID TRTSDTM $1024;
  infile '/app/input/adsl.csv' dsd dlm=',' firstobs=2 truncover lrecl=32767;
  input STUDYID : $char1024. USUBJID : $char1024. TRTSDTM : $char1024.;
  if lengthn(TRTSDTM)=16 then TRTSDTM=cats(TRTSDTM,':00');
run;
proc sql;
  create table changes as
    select STUDYID,USUBJID,FALNKID,FADTC as ASTDTM,
      max(SEVRANK) as SEVCOLN,
      max(case when FATESTCD='TOXGR' then input(FASTRESC,best32.)
        else . end) as GRDCOL
    from fa group by STUDYID,USUBJID,FALNKID,FADTC;
  create table joined as
    select a.*,c.ASTDTM,c.SEVCOLN,c.GRDCOL,b.TRTSDTM
    from changes as c inner join ae as a
      on c.STUDYID=a.STUDYID and c.USUBJID=a.USUBJID
        and c.FALNKID=a.AELNKID
    left join adsl as b
      on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID;
quit;
proc sort data=joined;
  by STUDYID USUBJID AESEQ ASTDTM;
run;
data states;
  length ASEV $1024;
  set joined;
  by STUDYID USUBJID AESEQ;
  retain ASEVN ATOXGRN;
  if first.AESEQ then do; ASEVN=.; ATOXGRN=.; end;
  if not missing(SEVCOLN) then ASEVN=SEVCOLN;
  if not missing(GRDCOL) then ATOXGRN=GRDCOL;
  select(ASEVN);
    when(1) ASEV='MILD'; when(2) ASEV='MODERATE';
    when(3) ASEV='SEVERE'; otherwise ASEV='';
  end;
run;
/* Read the next start by scanning periods in reverse chronological order. */
proc sort data=states;
  by STUDYID USUBJID AESEQ descending ASTDTM;
run;
data periods;
  length AENDTM NEXTSTART $1024;
  set states;
  by STUDYID USUBJID AESEQ;
  retain NEXTSTART;
  if first.AESEQ then NEXTSTART='';
  if not missing(NEXTSTART) then AENDTM=NEXTSTART;
  else AENDTM=AEENDTC;
  NEXTSTART=ASTDTM;
run;
proc sql;
  create table pretreatment as
    select STUDYID,USUBJID,AESEQ,max(ASTDTM) as BASEDTM
    from periods where not missing(TRTSDTM) and ASTDTM<TRTSDTM
    group by STUDYID,USUBJID,AESEQ;
  create table baseline as
    select a.STUDYID,a.USUBJID,a.AESEQ,
      a.ASEVN as PRESEV,a.ATOXGRN as PREGRD
    from periods as a inner join pretreatment as b
      on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID
        and a.AESEQ=b.AESEQ and a.ASTDTM=b.BASEDTM;
  create table compared as
    select a.*,b.PRESEV,b.PREGRD
    from periods as a left join baseline as b
      on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AESEQ=b.AESEQ;
  create table worsening as
    select STUDYID,USUBJID,AESEQ,min(ASTDTM) as FIRSTWRS
    from compared where not missing(TRTSDTM) and ASTDTM>=TRTSDTM and
      ((not missing(ASEVN) and not missing(PRESEV) and ASEVN>PRESEV) or
       (not missing(ATOXGRN) and not missing(PREGRD) and ATOXGRN>PREGRD))
    group by STUDYID,USUBJID,AESEQ;
  create table flagged as
    select a.*,b.FIRSTWRS from periods as a left join worsening as b
      on a.STUDYID=b.STUDYID and a.USUBJID=b.USUBJID and a.AESEQ=b.AESEQ;
quit;
proc sort data=flagged;
  by STUDYID USUBJID AESEQ ASTDTM;
run;
data result;
  length TRTEMFL $1;
  set flagged;
  by STUDYID USUBJID;
  retain ASEQ;
  if first.USUBJID then ASEQ=0;
  ASEQ=ASEQ+1;
  TRTEMFL='';
  if not missing(AESTDTC) and not missing(TRTSDTM) then do;
    if AESTDTC>=TRTSDTM or
      (not missing(FIRSTWRS) and ASTDTM>=FIRSTWRS) then TRTEMFL='Y';
  end;
run;
proc sql;
  create table final as
    select STUDYID,USUBJID,ASEQ,AESEQ,AEDECOD,AESTDTC,AEENDTC,
      ASTDTM,AENDTM,TRTSDTM,AESEV,AETOXGR,ASEV,ATOXGRN,TRTEMFL from result;
quit;
proc export data=final outfile='/app/output/adae.csv' dbms=csv replace; run;
