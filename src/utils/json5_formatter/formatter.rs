use std::collections::{HashMap, VecDeque};
use std::io;
use std::io::Write;

use delegate::delegate;
use serde_json::ser::{Formatter, PrettyFormatter};

/// A JSON formatter that adds comments from a schema.
pub struct Json5Formatter<'a> {
    inner: PrettyFormatter<'a>,
    paths: VecDeque<String>,
    comments: &'a HashMap<String, String>,
}

impl<'a> Json5Formatter<'a> {
    pub fn new(
        indent: &'a [u8],
        paths: VecDeque<String>,
        comments: &'a HashMap<String, String>,
    ) -> Self {
        Self { inner: PrettyFormatter::with_indent(indent), paths, comments }
    }
}

impl<'a> Formatter for Json5Formatter<'a> {
    #[inline]
    fn begin_object_key<W>(&mut self, writer: &mut W, mut first: bool) -> io::Result<()>
    where
        W: ?Sized + Write,
    {
        if let Some(path) = self.paths.pop_front()
            && let Some(comment) = self.comments.get(&path)
        {
            writer.write_all(if first { b"\n// " } else { b",\n// " })?;
            write_sanitized_line(writer, comment)?;
            // after writing a comment line, the next should not be prefixed with a comma
            first = true;
        }
        self.inner.begin_object_key(writer, first)
    }

    delegate! {
        to self.inner {
            fn begin_array<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()>;
            fn end_array<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()>;
            fn begin_array_value<W: ?Sized + Write>(&mut self, writer: &mut W, first: bool) -> io::Result<()>;
            fn end_array_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()>;
            fn begin_object<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()>;
            fn end_object<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()>;
            fn begin_object_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()>;
            fn end_object_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()>;
        }
    }
}

fn write_sanitized_line<W: ?Sized + Write>(writer: &mut W, input: &str) -> io::Result<()> {
    let mut started = false;
    let mut buf = [0u8; 4];

    for c in input.chars() {
        let c = if c.is_control() { ' ' } else { c };

        if !started {
            if c == ' ' {
                continue;
            }
            started = true;
        }

        let s = c.encode_utf8(&mut buf);
        writer.write_all(s.as_bytes())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use serde_json::{Serializer, json};

    use super::*;
    use crate::utils::json5_formatter::schema::{collect_paths, extract_comments};

    #[test]
    fn test_json_formatter_comments() {
        let data = json!({
          "tun": { "enable": true, "device": "utun" },
          "log": { "level": "info" }
        });
        let json_schema = json!({
          "$schema": "https://json-schema.org/draft/2020-12/schema",
          "type": "object",
          "properties": {
            "tun": {
              "type": "object",
              "description": "TUN 配置",
              "properties": {
                "enable": {
                  "type": "boolean",
                  "description": "是否启用"
                },
                "device": {
                  "type": "string",
                  "description": "TUN 设备名称"
                }
              }
            },
            "log": {
              "type": "object",
              "properties": {
                "level": {
                  "type": "string",
                  "description": "日志级别",
                  "enum": ["error", "warn", "info", "debug", "trace"]
                }
              }
            }
          }
        });

        let paths = collect_paths(&data);
        let comments = extract_comments(&json_schema);
        let formatter = Json5Formatter::new(b"  ", paths, &comments);

        let mut buf = Vec::with_capacity(512);
        let mut ser = Serializer::with_formatter(&mut buf, formatter);
        data.serialize(&mut ser).unwrap();
        let string = String::from_utf8(buf).unwrap();
        println!("{}", string);
        assert_eq!(
            string,
            r###"{
// TUN 配置
  "tun": {
// 是否启用
    "enable": true,
// TUN 设备名称
    "device": "utun"
  },
  "log": {
// 日志级别, Allowed values: "error", "warn", "info", "debug", "trace"
    "level": "info"
  }
}"###
        );
    }

    #[test]
    fn test_write_sanitized_line() {
        use std::io::Cursor;
        let mut buf = Cursor::new(Vec::new());
        write_sanitized_line(&mut buf, "  Hello,\nWorld!\t").unwrap();
        let result = String::from_utf8(buf.into_inner()).unwrap();
        assert_eq!(result, "Hello, World! ");
    }
}
