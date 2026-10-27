/* Derive numeric ADaM treatment date and datetime values. */
proc import datafile='/app/input/adsl_raw.csv' out=adsl_raw dbms=csv replace;
    getnames=yes;
run;

data adsl;
    retain STUDYID USUBJID TRT01A AGE HEIGHTBL TRTSDT TRTSDTM;
    set adsl_raw;
    length _trtsdtc $19;
    format TRTSDT yymmdd10. TRTSDTM e8601dt19.;
    if not missing(TRTSDTC) then do;
        _trtsdtc=strip(TRTSDTC);
        if lengthn(_trtsdtc)=16 then _trtsdtc=cats(_trtsdtc,':00');
        TRTSDTM=input(_trtsdtc,e8601dt.);
        TRTSDT=datepart(TRTSDTM);
    end;
    else do;
        TRTSDT=.;
        TRTSDTM=.;
    end;
    keep STUDYID USUBJID TRT01A AGE HEIGHTBL TRTSDT TRTSDTM;
run;

/* CSV export requires explicit formatted character representations. */
data adsl_csv;
    retain STUDYID USUBJID TRT01A AGE HEIGHTBL TRTSDT TRTSDTM;
    length STUDYID $200 USUBJID $200 TRT01A $200 AGE 8 HEIGHTBL 8 TRTSDT $10 TRTSDTM $19;
    set adsl(rename=(TRTSDT=_date TRTSDTM=_datetime));
    if not missing(_datetime) then do;
        TRTSDT=put(_date,yymmdd10.);
        TRTSDTM=put(_datetime,e8601dt19.);
    end;
    else do;
        TRTSDT='';
        TRTSDTM='';
    end;
    keep STUDYID USUBJID TRT01A AGE HEIGHTBL TRTSDT TRTSDTM;
run;

proc export data=adsl_csv outfile='/app/output/adsl.csv' dbms=csv replace;
run;
