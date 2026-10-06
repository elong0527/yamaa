//! Source callbacks run on the R thread; conditions are raised only after native return.
use extendr_api::prelude::*;
use yamaa_adapters::inheritance_transport;

#[extendr]
fn traverse_inheritance(schema_request: Raw, request: Raw, dispatch: Function) -> List {
    let run = || -> std::result::Result<String, String> {
        if schema_request.len() > inheritance_transport::MAX_REQUEST_BYTES
            || request.len() > inheritance_transport::MAX_REQUEST_BYTES
        {
            return Err("inheritance request exceeds byte limit".into());
        }
        let schema_request = std::str::from_utf8(schema_request.as_slice())
            .map_err(|_| "invalid UTF-8 schema request".to_owned())?;
        let request = std::str::from_utf8(request.as_slice())
            .map_err(|_| "invalid UTF-8 inheritance request".to_owned())?;
        inheritance_transport::interpret(schema_request, request, |message, maximum| {
            let result = dispatch
                .call(pairlist!(
                    request = Raw::from_bytes(message.as_bytes()),
                    maximum = maximum as i32
                ))
                .map_err(|_| "source dispatcher failed".to_owned())?;
            let result = result
                .as_list()
                .filter(|v| v.len() == 2)
                .ok_or_else(|| "invalid source dispatcher result".to_owned())?;
            let status = result
                .elt(0)
                .map_err(|_| "invalid source dispatcher result".to_owned())?
                .as_integer()
                .ok_or_else(|| "invalid source dispatcher status".to_owned())?;
            if status == 1 {
                return Err("source callback failed".to_owned());
            }
            if status != 0 {
                return Err("invalid source dispatcher status".to_owned());
            }
            let payload = result
                .elt(1)
                .map_err(|_| "invalid source dispatcher result".to_owned())?;
            let bytes = payload
                .as_raw()
                .ok_or_else(|| "source callback must return JSON text".to_owned())?;
            if bytes.len() > maximum {
                return Err("inheritance source reply exceeds byte limit".to_owned());
            }
            let text = std::str::from_utf8(bytes.as_slice())
                .map_err(|_| "invalid UTF-8 source reply".to_owned())?;
            Ok(text.to_owned())
        })
        .map_err(|error| match error {
            inheritance_transport::Failure::Host(message) => message,
            inheritance_transport::Failure::Transport(error) => error.to_string(),
        })
    };
    match run() {
        Ok(value) => list!(value = value, error = NULL),
        Err(error) => list!(value = NULL, error = error),
    }
}

extendr_module! {
    mod inheritance_callback;
    fn traverse_inheritance;
}
