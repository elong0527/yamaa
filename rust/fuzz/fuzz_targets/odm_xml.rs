//! ODM XML clinical items through the reader both hosts' `read_odm` calls:
//! format detection, plain and gzip TAR member selection, the XML tokenizer
//! and the clinical-item table, under the default options. That reader takes a
//! path, so each input is first written to one scratch file. Its `.xml` name
//! leaves the format to the reader's content detection. The same bytes then
//! go through the shared source decode as a specification's `.xml` input,
//! which reads them as XML alone, as an ordinary and as an ODM input.
#![no_main]

use std::{path::PathBuf, sync::LazyLock};

use libfuzzer_sys::fuzz_target;
use yamaa_adapters::{odm_xml, public_table::TableError, source_decoding};
use yamaa_core::{
    odm_items::{Error, Options},
    specification::{SourceInterpretation, SourceProfile},
};
use yamaa_fuzz::{decode, source};

/// One scratch file per fuzzing process, rewritten for each input.
static SCRATCH: LazyLock<PathBuf> = LazyLock::new(|| {
    std::env::temp_dir().join(format!("yamaa-fuzz-odm-{}.xml", std::process::id()))
});

fuzz_target!(|data: &[u8]| {
    std::fs::write(&*SCRATCH, data).expect("the scratch file is writable");
    // Input bytes are read or refused for a reason they hold. The file was
    // just written, so a missing file or a failed read is the reader's own
    // fault, as are an internal table error and a build without the reader.
    match odm_xml::read(&SCRATCH, &Options::default()) {
        Err(odm_xml::Error::Resource(_) | odm_xml::Error::Read(Error::Io { .. })) => {
            panic!("the ODM reader could not read the file it was given")
        }
        Err(odm_xml::Error::Table(TableError::Internal)) => {
            panic!("the ODM reader returned an internal table error for input bytes")
        }
        Err(odm_xml::Error::Unavailable) => panic!("this build omits the ODM reader"),
        Ok(_) | Err(odm_xml::Error::Read(_) | odm_xml::Error::Table(_)) => {}
    }
    // An input's held bytes are read, or refused as a document; never as a
    // host read, an option, a missing reader or an internal fault.
    let input = source(SourceProfile::OdmXml, Vec::new(), None);
    for interpretation in [SourceInterpretation::Ordinary, SourceInterpretation::Odm] {
        match decode(&input, data, interpretation) {
            Ok(_) => {}
            Err(source_decoding::Error::OdmSource(error)) if error.refuses_document() => {}
            Err(error) => panic!("an `.xml` input was refused for no reason it holds: {error:?}"),
        }
    }
});
