// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! OTLP logs (`ExportLogsServiceRequest`) as log lines, from protobuf or JSON:
//! `<time> <LEVEL> <service.name> [<scope>] <body> {k=v …} trace=<id> span=<id>`.
//!
//! The protobuf reader is a small decoder of the wire format over the handful of messages
//! used, by field number from `opentelemetry/proto/logs/v1/logs.proto` (unknown fields
//! skipped), so no code generator or extra crate is needed. JSON follows the OTLP JSON
//! mapping (hex trace and span ids, 64-bit integers as strings).

use serde_json::Value;

/// Attributes written per line at most.
const MAX_ATTRIBUTES: usize = 32;
/// Nesting of values read at most (a hostile body cannot recurse without end).
const MAX_DEPTH: usize = 16;

/// A value of the OTLP data model.
#[derive(Debug, Clone, PartialEq)]
pub enum AnyValue {
    Str(String),
    Bool(bool),
    Int(i64),
    Double(f64),
    Bytes(Vec<u8>),
    Array(Vec<AnyValue>),
    Map(Vec<(String, AnyValue)>),
    Empty,
}

impl AnyValue {
    /// The value as text: strings as they are, maps and arrays as compact JSON.
    fn text(&self) -> String {
        match self {
            AnyValue::Str(s) => s.clone(),
            other => other.json().to_string(),
        }
    }

    fn json(&self) -> Value {
        match self {
            AnyValue::Str(s) => Value::String(s.clone()),
            AnyValue::Bool(b) => Value::Bool(*b),
            AnyValue::Int(i) => Value::from(*i),
            AnyValue::Double(d) => Value::from(*d),
            AnyValue::Bytes(b) => Value::String(hex(b)),
            AnyValue::Array(a) => Value::Array(a.iter().map(AnyValue::json).collect()),
            AnyValue::Map(m) => {
                Value::Object(m.iter().map(|(k, v)| (k.clone(), v.json())).collect())
            }
            AnyValue::Empty => Value::Null,
        }
    }
}

/// One log record with what the line shows of its resource and scope.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Record {
    pub time_unix_nano: u64,
    pub observed_unix_nano: u64,
    pub severity_number: i32,
    pub severity_text: String,
    pub body: Option<AnyValue>,
    pub attributes: Vec<(String, AnyValue)>,
    pub trace_id: Vec<u8>,
    pub span_id: Vec<u8>,
    pub service: String,
    pub scope: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The level word of a severity number, else of the severity text, else `INFO`.
pub fn level_word(number: i32, text: &str) -> &'static str {
    match number {
        1..=4 => "TRACE",
        5..=8 => "DEBUG",
        9..=12 => "INFO",
        13..=16 => "WARN",
        17..=20 => "ERROR",
        21..=24 => "FATAL",
        _ => match text.to_ascii_uppercase().as_str() {
            t if t.starts_with("TRACE") => "TRACE",
            t if t.starts_with("DEBUG") => "DEBUG",
            t if t.starts_with("WARN") => "WARN",
            t if t.starts_with("ERR") => "ERROR",
            t if t.starts_with("FATAL") || t.starts_with("CRIT") => "FATAL",
            _ => "INFO",
        },
    }
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` of nanoseconds since the epoch.
fn iso(nanos: u64) -> String {
    let ms = (nanos / 1_000_000) as i64;
    let days = ms.div_euclid(86_400_000);
    let in_day = ms.rem_euclid(86_400_000);
    let (y, m, d) = crate::timestamp::days_to_date(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        in_day / 3_600_000,
        in_day / 60_000 % 60,
        in_day / 1000 % 60,
        in_day % 1000
    )
}

impl Record {
    /// The record as a log line; `now` is the receive time when it has none.
    pub fn line(&self, now: &dyn Fn() -> String) -> String {
        let time = match (self.time_unix_nano, self.observed_unix_nano) {
            (0, 0) => now(),
            (0, t) | (t, _) => iso(t),
        };
        let mut line = format!(
            "{time} {} {}",
            level_word(self.severity_number, &self.severity_text),
            if self.service.is_empty() {
                "-"
            } else {
                &self.service
            }
        );
        if !self.scope.is_empty() {
            line.push_str(&format!(" [{}]", self.scope));
        }
        line.push(' ');
        line.push_str(&self.body.as_ref().map(AnyValue::text).unwrap_or_default());
        if !self.attributes.is_empty() {
            let shown: Vec<String> = self
                .attributes
                .iter()
                .take(MAX_ATTRIBUTES)
                .map(|(k, v)| format!("{k}={}", v.text()))
                .collect();
            line.push_str(&format!(" {{{}}}", shown.join(" ")));
        }
        if self.trace_id.iter().any(|&b| b != 0) {
            line.push_str(&format!(" trace={}", hex(&self.trace_id)));
        }
        if self.span_id.iter().any(|&b| b != 0) {
            line.push_str(&format!(" span={}", hex(&self.span_id)));
        }
        // One record, one line.
        line.replace(['\r', '\n'], " ")
    }
}

// ---- protobuf wire format ----

/// A field of a message: its number and value.
enum Field<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    /// Read past (`flags`): its value is not shown.
    Fixed32,
}

struct Reader<'a> {
    b: &'a [u8],
}

impl<'a> Reader<'a> {
    fn varint(&mut self) -> Option<u64> {
        let mut v = 0u64;
        for i in 0..10 {
            let (&byte, rest) = self.b.split_first()?;
            self.b = rest;
            v |= u64::from(byte & 0x7f) << (7 * i);
            if byte & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.b.len() < n {
            return None;
        }
        let (head, rest) = self.b.split_at(n);
        self.b = rest;
        Some(head)
    }

    /// The next field, `None` at the end; `Err` for a malformed message.
    fn next(&mut self) -> Result<Option<(u32, Field<'a>)>, ()> {
        if self.b.is_empty() {
            return Ok(None);
        }
        let key = self.varint().ok_or(())?;
        let number = (key >> 3) as u32;
        let field = match key & 7 {
            0 => Field::Varint(self.varint().ok_or(())?),
            1 => Field::Fixed64(u64::from_le_bytes(
                self.take(8).ok_or(())?.try_into().map_err(|_| ())?,
            )),
            2 => {
                let len = usize::try_from(self.varint().ok_or(())?).map_err(|_| ())?;
                Field::Bytes(self.take(len).ok_or(())?)
            }
            5 => {
                self.take(4).ok_or(())?;
                Field::Fixed32
            }
            _ => return Err(()),
        };
        Ok(Some((number, field)))
    }
}

fn fields(b: &[u8]) -> impl Iterator<Item = Result<(u32, Field<'_>), ()>> {
    let mut r = Reader { b };
    std::iter::from_fn(move || r.next().transpose())
}

fn utf8(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn pb_any(b: &[u8], depth: usize) -> Result<AnyValue, ()> {
    if depth > MAX_DEPTH {
        return Err(());
    }
    let mut value = AnyValue::Empty;
    for f in fields(b) {
        value = match f? {
            (1, Field::Bytes(s)) => AnyValue::Str(utf8(s)),
            (2, Field::Varint(v)) => AnyValue::Bool(v != 0),
            (3, Field::Varint(v)) => AnyValue::Int(v as i64),
            (4, Field::Fixed64(v)) => AnyValue::Double(f64::from_bits(v)),
            (5, Field::Bytes(a)) => {
                let mut items = Vec::new();
                for f in fields(a) {
                    if let (1, Field::Bytes(v)) = f? {
                        items.push(pb_any(v, depth + 1)?);
                    }
                }
                AnyValue::Array(items)
            }
            (6, Field::Bytes(m)) => AnyValue::Map(pb_kvlist(m, depth + 1)?),
            (7, Field::Bytes(v)) => AnyValue::Bytes(v.to_vec()),
            _ => value,
        };
    }
    Ok(value)
}

fn pb_kv(b: &[u8], depth: usize) -> Result<(String, AnyValue), ()> {
    let (mut key, mut value) = (String::new(), AnyValue::Empty);
    for f in fields(b) {
        match f? {
            (1, Field::Bytes(k)) => key = utf8(k),
            (2, Field::Bytes(v)) => value = pb_any(v, depth)?,
            _ => {}
        }
    }
    Ok((key, value))
}

/// A `KeyValueList` (or the attributes of a message): its field 1 entries.
fn pb_kvlist(b: &[u8], depth: usize) -> Result<Vec<(String, AnyValue)>, ()> {
    let mut out = Vec::new();
    for f in fields(b) {
        if let (1, Field::Bytes(kv)) = f? {
            out.push(pb_kv(kv, depth)?);
        }
    }
    Ok(out)
}

/// The `service.name` attribute of a `Resource`.
fn pb_service(resource: &[u8]) -> Result<String, ()> {
    for (k, v) in pb_kvlist(resource, 0)? {
        if k == "service.name" {
            return Ok(v.text());
        }
    }
    Ok(String::new())
}

fn pb_record(b: &[u8], service: &str, scope: &str) -> Result<Record, ()> {
    let mut r = Record {
        service: service.to_string(),
        scope: scope.to_string(),
        ..Default::default()
    };
    for f in fields(b) {
        match f? {
            (1, Field::Fixed64(t)) => r.time_unix_nano = t,
            (11, Field::Fixed64(t)) => r.observed_unix_nano = t,
            (2, Field::Varint(n)) => r.severity_number = n as i32,
            (3, Field::Bytes(s)) => r.severity_text = utf8(s),
            (5, Field::Bytes(v)) => r.body = Some(pb_any(v, 0)?),
            (6, Field::Bytes(kv)) => r.attributes.push(pb_kv(kv, 0)?),
            (9, Field::Bytes(id)) => r.trace_id = id.to_vec(),
            (10, Field::Bytes(id)) => r.span_id = id.to_vec(),
            _ => {}
        }
    }
    Ok(r)
}

/// The records of a protobuf `ExportLogsServiceRequest`; `None` when malformed.
pub fn decode_protobuf(body: &[u8]) -> Option<Vec<Record>> {
    let mut out = Vec::new();
    for f in fields(body) {
        let (1, Field::Bytes(resource_logs)) = f.ok()? else {
            continue;
        };
        let mut service = String::new();
        let mut scopes = Vec::new();
        for f in fields(resource_logs) {
            match f.ok()? {
                (1, Field::Bytes(res)) => service = pb_service(res).ok()?,
                (2, Field::Bytes(sl)) => scopes.push(sl),
                _ => {}
            }
        }
        for sl in scopes {
            let mut scope = String::new();
            let mut records = Vec::new();
            for f in fields(sl) {
                match f.ok()? {
                    (1, Field::Bytes(sc)) => {
                        for f in fields(sc) {
                            if let (1, Field::Bytes(name)) = f.ok()? {
                                scope = utf8(name);
                            }
                        }
                    }
                    (2, Field::Bytes(rec)) => records.push(rec),
                    _ => {}
                }
            }
            for rec in records {
                out.push(pb_record(rec, &service, &scope).ok()?);
            }
        }
    }
    Some(out)
}

// ---- JSON ----

fn js_u64(v: &Value) -> u64 {
    match v {
        Value::String(s) => s.parse().unwrap_or(0),
        Value::Number(n) => n.as_u64().unwrap_or(0),
        _ => 0,
    }
}

fn js_any(v: &Value, depth: usize) -> AnyValue {
    if depth > MAX_DEPTH {
        return AnyValue::Empty;
    }
    let Some(obj) = v.as_object() else {
        return AnyValue::Empty;
    };
    if let Some(s) = obj.get("stringValue").and_then(Value::as_str) {
        return AnyValue::Str(s.to_string());
    }
    if let Some(b) = obj.get("boolValue").and_then(Value::as_bool) {
        return AnyValue::Bool(b);
    }
    if let Some(i) = obj.get("intValue") {
        return AnyValue::Int(match i {
            Value::String(s) => s.parse().unwrap_or(0),
            other => other.as_i64().unwrap_or(0),
        });
    }
    if let Some(d) = obj.get("doubleValue").and_then(Value::as_f64) {
        return AnyValue::Double(d);
    }
    if let Some(a) = obj
        .get("arrayValue")
        .and_then(|a| a.get("values"))
        .and_then(Value::as_array)
    {
        return AnyValue::Array(a.iter().map(|x| js_any(x, depth + 1)).collect());
    }
    if let Some(m) = obj.get("kvlistValue").and_then(|m| m.get("values")) {
        return AnyValue::Map(js_kvlist(m, depth + 1));
    }
    if let Some(b) = obj.get("bytesValue").and_then(Value::as_str) {
        return AnyValue::Str(b.to_string());
    }
    AnyValue::Empty
}

fn js_kvlist(v: &Value, depth: usize) -> Vec<(String, AnyValue)> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|kv| {
                    (
                        kv.get("key")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        kv.get("value")
                            .map_or(AnyValue::Empty, |x| js_any(x, depth)),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Hex ids of the JSON mapping as bytes.
fn js_id(v: Option<&Value>) -> Vec<u8> {
    let Some(s) = v.and_then(Value::as_str) else {
        return Vec::new();
    };
    (0..s.len() / 2)
        .filter_map(|i| u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok())
        .collect()
}

/// The records of a JSON `ExportLogsServiceRequest`; `None` when it is not JSON.
pub fn decode_json(body: &[u8]) -> Option<Vec<Record>> {
    let root: Value = serde_json::from_slice(body).ok()?;
    let mut out = Vec::new();
    for rl in root
        .get("resourceLogs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let service = rl
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .map(|a| js_kvlist(a, 0))
            .unwrap_or_default()
            .into_iter()
            .find(|(k, _)| k == "service.name")
            .map(|(_, v)| v.text())
            .unwrap_or_default();
        for sl in rl
            .get("scopeLogs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let scope = sl
                .get("scope")
                .and_then(|s| s.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            for r in sl
                .get("logRecords")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                out.push(Record {
                    time_unix_nano: r.get("timeUnixNano").map_or(0, js_u64),
                    observed_unix_nano: r.get("observedTimeUnixNano").map_or(0, js_u64),
                    severity_number: r.get("severityNumber").and_then(Value::as_i64).unwrap_or(0)
                        as i32,
                    severity_text: r
                        .get("severityText")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    body: r.get("body").map(|b| js_any(b, 0)),
                    attributes: r
                        .get("attributes")
                        .map(|a| js_kvlist(a, 0))
                        .unwrap_or_default(),
                    trace_id: js_id(r.get("traceId")),
                    span_id: js_id(r.get("spanId")),
                    service: service.clone(),
                    scope: scope.to_string(),
                });
            }
        }
    }
    Some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A small protobuf writer for the fixtures (the same wire format an SDK writes).
    pub(crate) struct W(pub Vec<u8>);

    impl W {
        fn varint(&mut self, mut v: u64) {
            loop {
                let byte = (v & 0x7f) as u8;
                v >>= 7;
                if v == 0 {
                    self.0.push(byte);
                    return;
                }
                self.0.push(byte | 0x80);
            }
        }
        pub(crate) fn bytes(&mut self, n: u32, b: &[u8]) -> &mut Self {
            self.varint(u64::from(n) << 3 | 2);
            self.varint(b.len() as u64);
            self.0.extend_from_slice(b);
            self
        }
        pub(crate) fn uint(&mut self, n: u32, v: u64) -> &mut Self {
            self.varint(u64::from(n) << 3);
            self.varint(v);
            self
        }
        pub(crate) fn fixed64(&mut self, n: u32, v: u64) -> &mut Self {
            self.varint(u64::from(n) << 3 | 1);
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
    }

    fn msg(f: impl FnOnce(&mut W)) -> Vec<u8> {
        let mut w = W(Vec::new());
        f(&mut w);
        w.0
    }

    fn kv(key: &str, value: &[u8]) -> Vec<u8> {
        msg(|w| {
            w.bytes(1, key.as_bytes()).bytes(2, value);
        })
    }

    fn string_value(s: &str) -> Vec<u8> {
        msg(|w| {
            w.bytes(1, s.as_bytes());
        })
    }

    /// An `ExportLogsServiceRequest` with one record of service `checkout`.
    pub(crate) fn sample_request() -> Vec<u8> {
        let record = msg(|w| {
            w.fixed64(1, 1_761_207_330_123_000_000)
                .uint(2, 17)
                .bytes(3, b"ERROR")
                .bytes(5, &string_value("payment failed"))
                .bytes(
                    6,
                    &kv(
                        "order.id",
                        &msg(|w| {
                            w.uint(3, 42);
                        }),
                    ),
                )
                .bytes(9, &[0xab; 16])
                .bytes(10, &[0x01; 8]);
        });
        let scope_logs = msg(|w| {
            w.bytes(
                1,
                &msg(|w| {
                    w.bytes(1, b"shop.payments");
                }),
            )
            .bytes(2, &record);
        });
        let resource = msg(|w| {
            w.bytes(1, &kv("service.name", &string_value("checkout")));
        });
        let resource_logs = msg(|w| {
            w.bytes(1, &resource).bytes(2, &scope_logs);
        });
        msg(|w| {
            w.bytes(1, &resource_logs);
        })
    }

    fn now() -> String {
        "2026-10-23T10:00:00.000Z".to_string()
    }

    #[test]
    fn protobuf_records() {
        let records = decode_protobuf(&sample_request()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].line(&now),
            "2025-10-23T08:15:30.123Z ERROR checkout [shop.payments] payment failed {order.id=42} \
             trace=abababababababababababababababab span=0101010101010101"
        );
        assert!(
            decode_protobuf(&[0x0a, 0xff]).is_none(),
            "a length past the end"
        );
    }

    #[test]
    fn json_records() {
        let body = br#"{"resourceLogs":[{"resource":{"attributes":[{"key":"service.name","value":{"stringValue":"api"}}]},
            "scopeLogs":[{"scope":{"name":"http"},"logRecords":[
              {"timeUnixNano":"1761207330000000000","severityText":"warning","body":{"kvlistValue":{"values":[{"key":"code","value":{"intValue":"503"}}]}}},
              {"observedTimeUnixNano":"1761207331000000000","severityNumber":5,"body":{"stringValue":"two\nlines"},"traceId":"0102"}]}]}]}"#;
        let records = decode_json(body).unwrap();
        assert_eq!(
            records[0].line(&now),
            r#"2025-10-23T08:15:30.000Z WARN api [http] {"code":503}"#
        );
        assert_eq!(
            records[1].line(&now),
            "2025-10-23T08:15:31.000Z DEBUG api [http] two lines trace=0102"
        );
        assert!(decode_json(b"not json").is_none());
    }

    #[test]
    fn severities_and_the_receive_time() {
        assert_eq!(level_word(1, ""), "TRACE");
        assert_eq!(level_word(9, "x"), "INFO");
        assert_eq!(level_word(24, ""), "FATAL");
        assert_eq!(level_word(0, "Critical"), "FATAL");
        assert_eq!(level_word(0, ""), "INFO");
        let r = Record {
            body: Some(AnyValue::Str("hi".into())),
            ..Default::default()
        };
        assert_eq!(r.line(&now), "2026-10-23T10:00:00.000Z INFO - hi");
    }

    #[test]
    fn deep_nesting_is_refused() {
        let mut v = string_value("x");
        for _ in 0..100 {
            let array = msg(|w| {
                w.bytes(1, &v);
            });
            v = msg(|w| {
                w.bytes(5, &array);
            });
        }
        assert!(pb_any(&v, 0).is_err());
    }
}
