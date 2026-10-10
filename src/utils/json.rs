use serde::Serialize;
use serde_json::Serializer;
use serde_json::ser::PrettyFormatter;

const INDENT: &[u8; 4] = b"    ";

pub fn pretty_json<T: Serialize>(data: &T) -> String {
    let mut buf = Vec::with_capacity(512);
    let formatter = PrettyFormatter::with_indent(INDENT);
    let mut serializer = Serializer::with_formatter(&mut buf, formatter);
    if data.serialize(&mut serializer).is_ok() {
        String::from_utf8(buf).unwrap_or_else(|_| "<utf8 error>".into())
    } else {
        "<invalid json>".into()
    }
}
