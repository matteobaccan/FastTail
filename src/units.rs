// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! Numbers with units in field values: `900ms`, `1.2s`, `1m30s`, `3.4MB`, `12KiB`,
//! `250/s`. A value is read in its family's base unit (durations in milliseconds, sizes
//! in bytes, rates per second), so `900ms < 1.2s`, and shown again in the unit that reads
//! best (`1.2 s`, `3.4 MiB`). Decimal (`KB` = 1000) and binary (`KiB` = 1024) prefixes are
//! kept apart. Reading never allocates: it runs on the scan workers for every value.

/// The kind of quantity a value holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Family {
    /// A bare number (`%` included).
    #[default]
    Plain,
    /// A duration, in milliseconds.
    Time,
    /// A size, in bytes.
    Size,
    /// Events per second.
    Rate,
    /// Bytes per second.
    ByteRate,
}

/// Families, for tallies indexed by `Family::index`.
pub const FAMILIES: usize = 5;

impl Family {
    pub const ALL: [Family; FAMILIES] = [
        Family::Plain,
        Family::Time,
        Family::Size,
        Family::Rate,
        Family::ByteRate,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    /// The name shown in messages: `duration`, `size`...
    pub fn name(self) -> &'static str {
        match self {
            Family::Plain => "number",
            Family::Time => "duration",
            Family::Size => "size",
            Family::Rate => "rate",
            Family::ByteRate => "byte rate",
        }
    }
}

/// A unit as named in a log format (`unit.latency=ms`): what a bare number of that field
/// means.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Unit {
    pub family: Family,
    /// The base units in one of it.
    pub factor: f64,
    /// A binary size prefix (`KiB`...).
    pub binary: bool,
}

/// A value as read: in the base unit, its family, whether its prefix was binary, and the
/// number as written (`1.5` of `1.5s`; the base value of `1m30s`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub value: f64,
    pub family: Family,
    pub binary: bool,
    pub written: f64,
}

const TIME_UNITS: [(&str, f64); 15] = [
    ("ns", 1e-6),
    ("µs", 1e-3),
    ("μs", 1e-3),
    ("us", 1e-3),
    ("ms", 1.0),
    ("sec", 1e3),
    ("s", 1e3),
    ("min", 6e4),
    ("m", 6e4),
    ("hr", 3.6e6),
    ("h", 3.6e6),
    ("d", 8.64e7),
    // Spelled out, as some loggers print them.
    ("secs", 1e3),
    ("mins", 6e4),
    ("days", 8.64e7),
];

const SIZE_UNITS: [(&str, f64, bool); 18] = [
    ("B", 1.0, false),
    ("KB", 1e3, false),
    ("kB", 1e3, false),
    ("kb", 1e3, false),
    ("MB", 1e6, false),
    ("mb", 1e6, false),
    ("GB", 1e9, false),
    ("gb", 1e9, false),
    ("TB", 1e12, false),
    ("tb", 1e12, false),
    ("PB", 1e15, false),
    ("KiB", 1024.0, true),
    ("kiB", 1024.0, true),
    ("MiB", 1048576.0, true),
    ("GiB", 1073741824.0, true),
    ("TiB", 1099511627776.0, true),
    ("PiB", 1125899906842624.0, true),
    ("bytes", 1.0, false),
];

/// The rate denominators and the seconds in each.
const PER: [(&str, f64); 6] = [
    ("/s", 1.0),
    ("/sec", 1.0),
    ("ps", 1.0),
    ("/min", 60.0),
    ("/m", 60.0),
    ("/h", 3600.0),
];

/// A plain number: digits with an optional sign, point and exponent (`inf` and `NaN`
/// refused).
fn plain(text: &str) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() || !t.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// The time unit named `name`, as its milliseconds.
fn time_unit(name: &str) -> Option<f64> {
    TIME_UNITS.iter().find(|(u, _)| *u == name).map(|(_, f)| *f)
}

/// `text` as a duration written as several parts (`1h30m`, `2m30.5s`, `1h2m3s4ms`), in
/// milliseconds: Go's form, each part a number then a time unit.
fn compound_duration(text: &str) -> Option<f64> {
    let b = text.as_bytes();
    let (mut i, mut total, mut parts) = (0, 0.0, 0);
    let negative = b.first() == Some(&b'-');
    if negative {
        i = 1;
    }
    while i < b.len() {
        let start = i;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
        }
        let number = text[start..i].parse::<f64>().ok()?;
        let unit_start = i;
        while i < b.len() && !(b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
        }
        let factor = time_unit(&text[unit_start..i])?;
        total += number * factor;
        parts += 1;
    }
    (parts >= 2 && total.is_finite()).then_some(if negative { -total } else { total })
}

/// `text` as a number with its unit, in the family's base unit; a bare number is
/// `Plain`. `None` when it is not a number (`GET`, `ms`, `5 apples`).
pub fn read(text: &str) -> Option<Reading> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    // The digits end where the unit starts: the last digit or point.
    let end = t
        .bytes()
        .rposition(|b| b.is_ascii_digit() || b == b'.')
        .map(|i| i + 1)?;
    let (digits, unit) = t.split_at(end);
    let unit = unit.trim_start();
    if unit.is_empty() || unit == "%" {
        return plain(digits).map(|value| Reading {
            value,
            family: Family::Plain,
            binary: false,
            written: value,
        });
    }
    if let Some(value) = plain(digits) {
        if let Some(f) = time_unit(unit) {
            return Some(Reading {
                value: value * f,
                family: Family::Time,
                binary: false,
                written: value,
            });
        }
        if let Some(r) = size_or_rate(value, unit) {
            return Some(r);
        }
    }
    compound_duration(t).map(|value| Reading {
        value,
        family: Family::Time,
        binary: false,
        written: value,
    })
}

/// `value` followed by a size or rate unit (`MB`, `KiB/s`, `/s`, `rps`).
fn size_or_rate(value: f64, unit: &str) -> Option<Reading> {
    let size = |u: &str| SIZE_UNITS.iter().find(|(n, ..)| *n == u).copied();
    if let Some((_, f, binary)) = size(unit) {
        return Some(Reading {
            value: value * f,
            family: Family::Size,
            binary,
            written: value,
        });
    }
    if unit == "rps" {
        return Some(Reading {
            value,
            family: Family::Rate,
            binary: false,
            written: value,
        });
    }
    for (per, seconds) in PER {
        let Some(head) = unit.strip_suffix(per) else {
            continue;
        };
        let head = head.trim_end();
        if head.is_empty() {
            return Some(Reading {
                value: value / seconds,
                family: Family::Rate,
                binary: false,
                written: value,
            });
        }
        if let Some((_, f, binary)) = size(head) {
            return Some(Reading {
                value: value * f / seconds,
                family: Family::ByteRate,
                binary,
                written: value,
            });
        }
    }
    None
}

/// `text` read as `read` does, a bare number taking `bare` when the field has one (a log
/// format's `unit.<field>`).
pub fn read_as(text: &str, bare: Option<Unit>) -> Option<Reading> {
    let r = read(text)?;
    Some(match (r.family, bare) {
        (Family::Plain, Some(u)) => Reading {
            value: r.value * u.factor,
            family: u.family,
            binary: u.binary,
            written: r.written,
        },
        _ => r,
    })
}

/// The unit named `name` (`ms`, `s`, `KiB`, `MB/s`, `/s`), for `unit.<field>=`.
pub fn unit_named(name: &str) -> Option<Unit> {
    let r = read(&format!("1{}", name.trim())).filter(|r| r.family != Family::Plain)?;
    Some(Unit {
        family: r.family,
        factor: r.value,
        binary: r.binary,
    })
}

/// The scale a set of values is shown in: their family, and binary prefixes when most of
/// their sizes had them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Scale {
    pub family: Family,
    pub binary: bool,
}

/// How many values of each family were read, for the family a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tally {
    pub counts: [u64; FAMILIES],
    /// Values with a binary prefix.
    pub binary: u64,
}

impl Tally {
    pub fn add(&mut self, r: &Reading) {
        self.counts[r.family.index()] += 1;
        if r.binary {
            self.binary += 1;
        }
    }

    pub fn merge(&mut self, o: &Tally) {
        for (a, b) in self.counts.iter_mut().zip(o.counts) {
            *a += b;
        }
        self.binary += o.binary;
    }

    pub fn total(&self) -> u64 {
        self.counts.iter().sum()
    }

    /// The family most values have (plain numbers on a tie with a unit lose, so a few
    /// `0` among `12ms` do not make the field plain), and its count.
    pub fn dominant(&self) -> (Family, u64) {
        let mut best = (Family::Plain, self.counts[0]);
        for f in &Family::ALL[1..] {
            let n = self.counts[f.index()];
            if n > 0 && n >= best.1 {
                best = (*f, n);
            }
        }
        best
    }

    /// The scale the values are shown in.
    pub fn scale(&self) -> Scale {
        let (family, n) = self.dominant();
        Scale {
            family,
            binary: matches!(family, Family::Size | Family::ByteRate) && self.binary * 2 > n,
        }
    }
}

/// A number as the tables show it: whole numbers without decimals, otherwise up to three.
fn num(v: f64) -> String {
    let s = if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else if v.abs() >= 100.0 {
        format!("{v:.1}")
    } else if v.abs() >= 1.0 {
        format!("{v:.2}")
    } else {
        format!("{v:.3}")
    };
    // `1.50` reads as `1.5`.
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// `v` (in the base unit of `scale`) in the unit that reads best: `850 ms`, `1.2 s`,
/// `2.5 min`, `3.4 MB`, `12 KiB/s`. A plain number is shown as it is.
pub fn format(v: f64, scale: Scale) -> String {
    let a = v.abs();
    let sizes = |v: f64, suffix: &str| {
        let (step, names): (f64, [&str; 6]) = if scale.binary {
            (1024.0, ["B", "KiB", "MiB", "GiB", "TiB", "PiB"])
        } else {
            (1000.0, ["B", "KB", "MB", "GB", "TB", "PB"])
        };
        let mut x = v;
        let mut i = 0;
        while x.abs() >= step && i + 1 < names.len() {
            x /= step;
            i += 1;
        }
        format!("{} {}{suffix}", num(x), names[i])
    };
    match scale.family {
        Family::Plain => num(v),
        Family::Time if a == 0.0 => "0 ms".to_string(),
        Family::Time if a < 1e-3 => format!("{} ns", num(v * 1e6)),
        Family::Time if a < 1.0 => format!("{} µs", num(v * 1e3)),
        Family::Time if a < 1e3 => format!("{} ms", num(v)),
        Family::Time if a < 6e4 => format!("{} s", num(v / 1e3)),
        Family::Time if a < 3.6e6 => format!("{} min", num(v / 6e4)),
        Family::Time if a < 8.64e7 => format!("{} h", num(v / 3.6e6)),
        Family::Time => format!("{} d", num(v / 8.64e7)),
        Family::Size => sizes(v, ""),
        Family::Rate => format!("{}/s", num(v)),
        Family::ByteRate => sizes(v, "/s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(t: &str) -> Option<(f64, Family)> {
        read(t).map(|r| (r.value, r.family))
    }

    #[test]
    fn durations_read_in_milliseconds() {
        assert_eq!(base("900ms"), Some((900.0, Family::Time)));
        assert_eq!(base("1.2s"), Some((1200.0, Family::Time)));
        assert_eq!(base(" 250 ms "), Some((250.0, Family::Time)));
        assert_eq!(base("1500us"), Some((1.5, Family::Time)));
        assert_eq!(base("1500µs"), Some((1.5, Family::Time)));
        assert_eq!(base("2m"), Some((120_000.0, Family::Time)));
        assert_eq!(base("1h"), Some((3_600_000.0, Family::Time)));
        assert_eq!(base("1m30s"), Some((90_000.0, Family::Time)));
        assert_eq!(base("1h2m3.5s"), Some((3_723_500.0, Family::Time)));
        assert_eq!(base("-1m30s"), Some((-90_000.0, Family::Time)));
        assert!(read("900ms").unwrap().value < read("1.2s").unwrap().value);
    }

    #[test]
    fn sizes_keep_decimal_and_binary_apart() {
        assert_eq!(base("3KB"), Some((3000.0, Family::Size)));
        assert_eq!(base("3KiB"), Some((3072.0, Family::Size)));
        assert_eq!(base("3.4 MB"), Some((3_400_000.0, Family::Size)));
        assert_eq!(base("1GiB"), Some((1_073_741_824.0, Family::Size)));
        assert_eq!(base("512 bytes"), Some((512.0, Family::Size)));
        assert!(read("2KiB").unwrap().binary);
        assert!(!read("2KB").unwrap().binary);
    }

    #[test]
    fn rates_read_per_second() {
        assert_eq!(base("120/min"), Some((2.0, Family::Rate)));
        assert_eq!(base("50/s"), Some((50.0, Family::Rate)));
        assert_eq!(base("50 rps"), Some((50.0, Family::Rate)));
        assert_eq!(base("2MB/s"), Some((2e6, Family::ByteRate)));
    }

    #[test]
    fn plain_numbers_and_non_numbers() {
        assert_eq!(base("-4"), Some((-4.0, Family::Plain)));
        assert_eq!(base("12.5 %"), Some((12.5, Family::Plain)));
        assert_eq!(base("1e3"), Some((1000.0, Family::Plain)));
        for t in [
            "", "ms", "GET", "inf", "NaN", "5 apples", "v1.2", "1.2.3s", "3x",
        ] {
            assert_eq!(read(t), None, "{t:?}");
        }
    }

    #[test]
    fn a_format_unit_gives_bare_numbers_their_meaning() {
        let ms = unit_named("ms").unwrap();
        assert_eq!(read_as("250", Some(ms)).unwrap().value, 250.0);
        assert_eq!(read_as("2s", Some(ms)).unwrap().value, 2000.0);
        let s = unit_named("s").unwrap();
        assert_eq!(read_as("1.5", Some(s)).unwrap().family, Family::Time);
        assert_eq!(read_as("1.5", Some(s)).unwrap().value, 1500.0);
        assert_eq!(unit_named("KiB").unwrap().factor, 1024.0);
        assert!(unit_named("apples").is_none());
        assert!(unit_named("").is_none());
    }

    #[test]
    fn the_dominant_family_and_its_scale() {
        let mut t = Tally::default();
        for v in ["12ms", "1.2s", "0", "45ms"] {
            t.add(&read(v).unwrap());
        }
        assert_eq!(t.dominant(), (Family::Time, 3));
        let mut s = Tally::default();
        for v in ["1KiB", "2MiB", "3KB"] {
            s.add(&read(v).unwrap());
        }
        assert_eq!(
            s.scale(),
            Scale {
                family: Family::Size,
                binary: true
            }
        );
    }

    #[test]
    fn values_are_shown_in_the_unit_that_reads_best() {
        let time = Scale {
            family: Family::Time,
            binary: false,
        };
        assert_eq!(format(1200.0, time), "1.2 s");
        assert_eq!(format(850.0, time), "850 ms");
        assert_eq!(format(150_000.0, time), "2.5 min");
        assert_eq!(format(0.25, time), "250 µs");
        let dec = Scale {
            family: Family::Size,
            binary: false,
        };
        assert_eq!(format(3_400_000.0, dec), "3.4 MB");
        assert_eq!(format(512.0, dec), "512 B");
        let bin = Scale {
            family: Family::Size,
            binary: true,
        };
        assert_eq!(format(3.5 * 1048576.0, bin), "3.5 MiB");
        let rate = Scale {
            family: Family::ByteRate,
            binary: false,
        };
        assert_eq!(format(2e6, rate), "2 MB/s");
        assert_eq!(format(1234.0, Scale::default()), "1234");
    }
}
