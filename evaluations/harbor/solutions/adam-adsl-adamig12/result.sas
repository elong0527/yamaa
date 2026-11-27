/* Derive numeric ADaM treatment date and datetime values. */
proc import datafile='/app/input/adsl_raw.csv' out=adsl_raw dbms=csv replace;
    getnames=yes;
run;

data adsl;
    retain STUDYID USUBJID TRT01A AGE HEIGHTBL TRTSDT TRTSDTM TRTSTMF;
    set adsl_raw;
    length _trtsdtc $19 TRTSTMF $1;
    format TRTSDT yymmdd10. TRTSDTM e8601dt19.;
    if not missing(TRTSDTC) then do;
        _trtsdtc=strip(TRTSDTC);
        /* Seconds are collected for other subjects: flag imputed ones. */
        TRTSTMF='';
        if lengthn(_trtsdtc)=16 then do; _trtsdtc=cats(_trtsdtc,':00'); TRTSTMF='S'; end;
        TRTSDTM=input(_trtsdtc,e8601dt.);
        TRTSDT=datepart(TRTSDTM);
    end;
    else do;
        TRTSDT=.;
        TRTSDTM=.;
        TRTSTMF='';
    end;
    keep STUDYID USUBJID TRT01A AGE HEIGHTBL TRTSDT TRTSDTM TRTSTMF;
run;

/* The SELECT sets the requested column order. PROC EXPORT writes the
   attached date and datetime formats. */
proc sql;
    create table final as
    select STUDYID, USUBJID, TRT01A, AGE, HEIGHTBL, TRTSDT, TRTSDTM, TRTSTMF
    from adsl;
quit;

proc export data=final outfile='/app/output/adsl.csv' dbms=csv replace;
run;
