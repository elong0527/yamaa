# ADaM ADSL: reject a site name stored in another encoding

This example uses a collected subject listing to attempt one record per
subject:

- `SITENM` is the name of the site the subject enrolled at.

One site name was written by a system that stored its accented letter in an
older encoding, and those bytes spell no text. A reader that replaced the
byte with a substitute character would store a site name the study never
recorded, and one that dropped it would store a different name again. The run
must fail and no artifact is accepted.

## How to fix

Export the listing as UTF-8, so every collected character keeps the value it
was entered with:

    CTX,CTX-02,Hopital Saint-Antoine

Converting a file after the fact is safe only while the original encoding is
known. A study that stores names in an unstated encoding cannot say which
letters it collected.
