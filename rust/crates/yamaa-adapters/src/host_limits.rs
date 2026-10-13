//! Capacity policies for copied host metadata and retained evidence. Both bindings
//! use these limits; they are separate from normative language requirements.
pub const IDENTITY_BYTES: usize = 65_536;
pub const REPLY_BYTES: usize = 16_777_216;
pub const RECORDS: usize = 65_536;
pub const SOURCE_BYTES: usize = 67_108_864;
pub const FUNCTIONS: usize = 1024;
pub const VERSION_FINDINGS: usize = 2_049;

pub fn source_evidence<'a>(
    sources: impl Iterator<Item = (&'a str, &'a [u8])>,
) -> Result<(), &'static str> {
    let mut text = REPLY_BYTES;
    let mut bytes = SOURCE_BYTES;
    let mut count = RECORDS;
    for (name, source) in sources {
        count = count.checked_sub(1).ok_or("source evidence limit")?;
        text = text
            .checked_sub(name.len())
            .ok_or("source evidence limit")?;
        bytes = bytes
            .checked_sub(source.len())
            .ok_or("source evidence limit")?;
    }
    Ok(())
}
