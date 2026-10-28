\name{yamaa_read_odm}
\alias{yamaa_read_odm}
\title{Read the clinical items of an ODM XML file}
\description{
Reads an ODM 1.3 or 2.0 XML file, or one member of a TAR archive, into one row
per clinical item.
}
\usage{
yamaa_read_odm(source, archive_member = NULL, max_archive_members = 10000,
               max_expanded_bytes = 268435456, max_xml_depth = 64)
}
\arguments{
  \item{source}{Path of an ODM XML file or TAR archive, taken as
    \code{\link{yamaa_spec}} takes its path: names and a class are dropped,
    and a leading \code{~} expands to the home directory. The
    \code{archive_member} name is never expanded.}
  \item{archive_member}{Name of the XML member to read from a TAR, or
    \code{NULL} to select the only XML member.}
  \item{max_archive_members}{Most members a TAR may hold.}
  \item{max_expanded_bytes}{Most bytes the input may expand to.}
  \item{max_xml_depth}{Deepest element nesting accepted.}
}
\value{
A data frame with one row per clinical item. A refusal raises a condition of
class \code{yamaa_error}.
}
\examples{
odm <- tempfile(fileext = ".xml")
writeLines('<?xml version="1.0" encoding="UTF-8"?>
<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3" ODMVersion="1.3" FileOID="F.1">
  <ClinicalData StudyOID="S.1" MetaDataVersionOID="M.1">
    <SubjectData SubjectKey="01">
      <StudyEventData StudyEventOID="SE.SCREEN">
        <FormData FormOID="F.DM">
          <ItemGroupData ItemGroupOID="IG.DM">
            <ItemData ItemOID="IT.DM.AGE" Value="35"/>
            <ItemData ItemOID="IT.DM.SEX" Value="F"/>
          </ItemGroupData>
        </FormData>
      </StudyEventData>
    </SubjectData>
  </ClinicalData>
</ODM>
', odm)

items <- yamaa_read_odm(odm)
items[, c("SubjectKey", "ItemOID", "Value")]

unlink(odm)
}
