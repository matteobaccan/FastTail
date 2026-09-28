//! Automatic highlighting of tokens: IP addresses, UUIDs, URLs, durations and file paths
//! painted with a colour of the theme, with no rule to write (see `TailEngine::
//! match_highlight_spans_with`, where the spans rank after the user rules, the quick labels
//! and the ANSI colours).
//!
//! The scanner is a single left-to-right pass without regex: at each token start (the
//! line start, or a byte after one that cannot continue a word) it tries the kinds
//! cheapest first - URL, UUID, IPv4, IPv6, duration, path - and on a match jumps past the
//! token, so a URL's path is not also a path and an IP inside it is not also an IP.
//! Each matcher also checks the byte after the token, so `1.2.3.4.5`, `10.0.0.1abc` or
//! `5min` are not cut into a token and a tail.

/// A kind of automatic token; each has its own colour in every theme
/// (`CyberTheme::token_color`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// IPv4 (with an optional `:port`) or IPv6 (bracketed with a port too).
    Ip,
    Uuid,
    Url,
    Duration,
    Path,
}

impl TokenKind {
    pub const ALL: [TokenKind; 5] = [
        TokenKind::Ip,
        TokenKind::Uuid,
        TokenKind::Url,
        TokenKind::Duration,
        TokenKind::Path,
    ];

    /// The name in `auto_highlight_kinds` of `fasttail.ini`.
    pub fn key(self) -> &'static str {
        match self {
            TokenKind::Ip => "ip",
            TokenKind::Uuid => "uuid",
            TokenKind::Url => "url",
            TokenKind::Duration => "duration",
            TokenKind::Path => "path",
        }
    }

    /// i18n key of the Settings toggle.
    pub fn name_key(self) -> &'static str {
        match self {
            TokenKind::Ip => "auto_hl_ip",
            TokenKind::Uuid => "auto_hl_uuid",
            TokenKind::Url => "auto_hl_url",
            TokenKind::Duration => "auto_hl_duration",
            TokenKind::Path => "auto_hl_path",
        }
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// A set of token kinds (the empty set turns the scanner off).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub struct TokenKinds(u8);

impl TokenKinds {
    pub const NONE: TokenKinds = TokenKinds(0);
    pub const ALL: TokenKinds = TokenKinds(0b1_1111);

    pub fn contains(self, kind: TokenKind) -> bool {
        self.0 & kind.bit() != 0
    }

    pub fn set(&mut self, kind: TokenKind, on: bool) {
        if on {
            self.0 |= kind.bit();
        } else {
            self.0 &= !kind.bit();
        }
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// `ip,uuid,url,duration,path` as written in `fasttail.ini` (only the kinds in the set).
    pub fn to_config(self) -> String {
        TokenKind::ALL
            .iter()
            .filter(|k| self.contains(**k))
            .map(|k| k.key())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Reads `auto_highlight_kinds`: comma-separated names, unknown ones ignored.
    pub fn from_config(text: &str) -> TokenKinds {
        let mut kinds = TokenKinds::NONE;
        for name in text.split(',') {
            let name = name.trim();
            if let Some(kind) = TokenKind::ALL
                .iter()
                .find(|k| k.key().eq_ignore_ascii_case(name))
            {
                kinds.set(*kind, true);
            }
        }
        kinds
    }
}

/// Calls `found(start, end, kind)` for every token of the kinds in `kinds` in `line`, left
/// to right, without overlap; `found` returns `false` to stop (the span budget is full).
pub fn scan(line: &str, kinds: TokenKinds, mut found: impl FnMut(usize, usize, TokenKind) -> bool) {
    if kinds.is_empty() {
        return;
    }
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let starts = i == 0 || !continues_word(b[i - 1]);
        if !starts || !may_start(c) {
            i += 1;
            continue;
        }
        match match_at(b, i, kinds) {
            Match::Token(end, kind) => {
                if !found(i, end, kind) {
                    return;
                }
                i = end;
            }
            Match::SkipTo(end) => i = end.max(i + 1),
            Match::None => i += 1,
        }
    }
}

/// Collects `scan` into a vector (tests, benchmarks).
pub fn tokens(line: &str, kinds: TokenKinds) -> Vec<(usize, usize, TokenKind)> {
    let mut out = Vec::new();
    scan(line, kinds, |s, e, k| {
        out.push((s, e, k));
        true
    });
    out
}

/// A byte that belongs to the word before it: a token cannot start right after it.
fn continues_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'-' | b'/' | b'\\') || c >= 0x80
}

/// A byte some token can start with.
fn may_start(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'/' | b'~' | b'\\' | b'[' | b':')
}

/// What the matchers found at a token start.
enum Match {
    Token(usize, TokenKind),
    None,
    /// Nothing starts before this index (a run no token can hide in).
    SkipTo(usize),
}

/// The token at `at`, cheapest kinds first.
fn match_at(b: &[u8], at: usize, kinds: TokenKinds) -> Match {
    let c = b[at];
    if kinds.contains(TokenKind::Url) && c.is_ascii_alphabetic() {
        if let Some(end) = url(b, at) {
            return Match::Token(end, TokenKind::Url);
        }
    }
    if kinds.contains(TokenKind::Uuid) && c.is_ascii_hexdigit() {
        if let Some(end) = uuid(b, at) {
            return Match::Token(end, TokenKind::Uuid);
        }
    }
    if kinds.contains(TokenKind::Ip) {
        if c.is_ascii_digit() {
            if let Some(end) = ipv4(b, at) {
                return Match::Token(end, TokenKind::Ip);
            }
        }
        if c.is_ascii_hexdigit() || c == b':' || c == b'[' {
            match ipv6(b, at) {
                Ipv6::Found(end) => return Match::Token(end, TokenKind::Ip),
                // A duration or a path cannot start a run of hex digits and colons
                // longer than an address either (a duration next to a colon is none).
                Ipv6::LongRun(end) => return Match::SkipTo(end),
                Ipv6::No => {}
            }
        }
    }
    if kinds.contains(TokenKind::Duration) && c.is_ascii_digit() {
        if let Some(end) = duration(b, at) {
            return Match::Token(end, TokenKind::Duration);
        }
    }
    if kinds.contains(TokenKind::Path) {
        if let Some(end) = path(b, at) {
            return Match::Token(end, TokenKind::Path);
        }
    }
    Match::None
}

/// Whether a token may end before `end`: at the line end, or before a byte that cannot
/// continue it (a dot counts only when a word follows it: `10.0.0.1.` ends a sentence).
fn ends_token(b: &[u8], end: usize) -> bool {
    match b.get(end) {
        None => true,
        Some(c) if c.is_ascii_alphanumeric() || *c == b'_' || *c >= 0x80 => false,
        Some(b'.') | Some(b'-') => !b.get(end + 1).is_some_and(|n| n.is_ascii_alphanumeric()),
        Some(_) => true,
    }
}

fn digits(b: &[u8], at: usize, max: usize) -> usize {
    b[at.min(b.len())..]
        .iter()
        .take(max)
        .take_while(|c| c.is_ascii_digit())
        .count()
}

fn hex_digits(b: &[u8], at: usize, max: usize) -> usize {
    b[at.min(b.len())..]
        .iter()
        .take(max + 1)
        .take_while(|c| c.is_ascii_hexdigit())
        .count()
}

/// `scheme://` then everything up to a space, a quote or an angle bracket, with trailing
/// sentence punctuation left out.
fn url(b: &[u8], at: usize) -> Option<usize> {
    const SCHEMES: [&[u8]; 6] = [b"https", b"http", b"ftp", b"wss", b"ws", b"file"];
    let scheme = SCHEMES.iter().find(|s| {
        b.len() >= at + s.len() + 3
            && b[at..at + s.len()].eq_ignore_ascii_case(s)
            && &b[at + s.len()..at + s.len() + 3] == b"://"
    })?;
    let body = at + scheme.len() + 3;
    let mut end = body;
    while end < b.len()
        && !b[end].is_ascii_whitespace()
        && !matches!(b[end], b'"' | b'\'' | b'<' | b'>' | b'`')
    {
        end += 1;
    }
    while end > body
        && matches!(
            b[end - 1],
            b'.' | b',' | b';' | b':' | b'!' | b'?' | b')' | b']' | b'}'
        )
    {
        end -= 1;
    }
    (end > body).then_some(end)
}

/// `8-4-4-4-12` hex digits.
fn uuid(b: &[u8], at: usize) -> Option<usize> {
    let mut pos = at;
    for (n, len) in [8usize, 4, 4, 4, 12].iter().enumerate() {
        if n > 0 {
            if b.get(pos) != Some(&b'-') {
                return None;
            }
            pos += 1;
        }
        if hex_digits(b, pos, *len) != *len {
            return None;
        }
        pos += len;
    }
    ends_token(b, pos).then_some(pos)
}

/// Four octets 0-255 and an optional `:port` (1-65535).
fn ipv4(b: &[u8], at: usize) -> Option<usize> {
    let mut pos = at;
    let mut octets = [0u32; 4];
    for (n, octet) in octets.iter_mut().enumerate() {
        if n > 0 {
            if b.get(pos) != Some(&b'.') {
                return None;
            }
            pos += 1;
        }
        let len = digits(b, pos, 4);
        if len == 0 || len > 3 {
            return None;
        }
        let value: u32 = std::str::from_utf8(&b[pos..pos + len]).ok()?.parse().ok()?;
        if value > 255 {
            return None;
        }
        *octet = value;
        pos += len;
    }
    // `118.0.0.0` (a browser build) or `2.1.0.0` (a version) look like addresses; a
    // network address ending in `.0.0` is rare enough in logs to leave it as text.
    // `0.0.0.0`, the "every interface" of a listener, stays an address.
    if octets[2] == 0 && octets[3] == 0 && octets != [0; 4] {
        return None;
    }
    if b.get(pos) == Some(&b':') {
        let len = digits(b, pos + 1, 6);
        if (1..=5).contains(&len) {
            let port: u32 = std::str::from_utf8(&b[pos + 1..pos + 1 + len])
                .ok()?
                .parse()
                .ok()?;
            if (1..=65535).contains(&port) && ends_token(b, pos + 1 + len) {
                return Some(pos + 1 + len);
            }
        }
    }
    ends_token(b, pos).then_some(pos)
}

/// Longest text an IPv6 address can be: 8 groups of 4 hex digits and 7 colons, or 39
/// bytes, and an embedded `::` form is shorter.
const MAX_IPV6_BYTES: usize = 39;

/// What `ipv6` found at a position.
enum Ipv6 {
    Found(usize),
    No,
    /// A run of hex digits and colons too long to be an address, ending at this index:
    /// nothing inside it is an address either, and the scan skips it whole.
    LongRun(usize),
}

/// Hex groups of 1-4 digits: the full form (8 groups, 7 colons) or a compressed one with
/// a single `::` that has a digit in some group or at least three groups (so `dead::beef`
/// and `Foo::bar` stay text). A clock (`14:02:05`), a MAC address or `12:34:56:78` has
/// neither form. `[addr]:port` is one token. At most `MAX_IPV6_BYTES + 2` bytes are read
/// before giving up, so a long run of `00:01:02:…` costs a constant per position.
fn ipv6(b: &[u8], at: usize) -> Ipv6 {
    let is_run = |c: u8| c.is_ascii_hexdigit() || c == b':';
    let bracketed = b[at] == b'[';
    let start = at + usize::from(bracketed);
    let mut end = start;
    while end < b.len() && is_run(b[end]) {
        end += 1;
        if end - start > MAX_IPV6_BYTES + 1 {
            while end < b.len() && is_run(b[end]) {
                end += 1;
            }
            return Ipv6::LongRun(end);
        }
    }
    // A single colon after the address belongs to the text (`addr: ...`).
    if !bracketed && end > start + 1 && b[end - 1] == b':' && b[end - 2] != b':' {
        end -= 1;
    }
    let text = &b[start..end];
    if text.len() < 2 || text.len() > MAX_IPV6_BYTES {
        return Ipv6::No;
    }
    let colons = text.iter().filter(|c| **c == b':').count();
    if !(2..=7).contains(&colons) {
        return Ipv6::No;
    }
    let compressed = text.windows(2).filter(|w| w == b"::").count();
    if compressed > 1 || text.windows(3).any(|w| w == b":::") {
        return Ipv6::No;
    }
    let mut filled = 0;
    for group in text.split(|c| *c == b':') {
        if group.len() > 4 {
            return Ipv6::No;
        }
        filled += usize::from(!group.is_empty());
    }
    let has_digit = text.iter().any(u8::is_ascii_digit);
    let valid = if compressed == 1 {
        filled <= 7 && (has_digit || filled >= 3)
    } else {
        colons == 7 && filled == 8
    };
    // `::` alone, or a lone compressed group of letters (`a::b` in prose), stays text.
    if !valid || filled == 0 || (compressed == 1 && filled == 1 && text.len() < 3) {
        return Ipv6::No;
    }
    if bracketed {
        if b.get(end) != Some(&b']') {
            return Ipv6::No;
        }
        end += 1;
        if b.get(end) == Some(&b':') {
            let len = digits(b, end + 1, 6);
            if (1..=5).contains(&len) {
                end += 1 + len;
            }
        }
    }
    if ends_token(b, end) {
        Ipv6::Found(end)
    } else {
        Ipv6::No
    }
}

/// A number and a unit, or a chain of them (`2m30s`, `1h5m`): `ns`, `µs`, `us`, `ms`,
/// `s`, `m`, `h`, `d`. The number may have a fraction (`1.25s`).
fn duration(b: &[u8], at: usize) -> Option<usize> {
    let mut pos = at;
    let mut parts = 0;
    loop {
        let int = digits(b, pos, usize::MAX);
        if int == 0 {
            break;
        }
        let mut num_end = pos + int;
        if b.get(num_end) == Some(&b'.') {
            let frac = digits(b, num_end + 1, usize::MAX);
            if frac > 0 {
                num_end += 1 + frac;
            }
        }
        let Some(unit) = unit_len(b, num_end) else {
            break;
        };
        pos = num_end + unit;
        parts += 1;
    }
    // A group of a clock or a MAC address (`:4d:`) is no duration.
    let in_colons = (at > 0 && b[at - 1] == b':') || b.get(pos) == Some(&b':');
    (parts > 0 && ends_token(b, pos) && !in_colons).then_some(pos)
}

fn unit_len(b: &[u8], at: usize) -> Option<usize> {
    let rest = &b[at.min(b.len())..];
    for unit in [&b"ns"[..], b"us", "\u{b5}s".as_bytes(), b"ms"] {
        if rest.starts_with(unit) {
            return Some(unit.len());
        }
    }
    match rest.first() {
        Some(b's' | b'm' | b'h' | b'd') => Some(1),
        _ => None,
    }
}

/// A byte a path can hold.
fn path_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'_' | b'.' | b'-' | b'/' | b'\\' | b'~' | b'+' | b'@' | b'%' | b'$' | b'=' | b','
        )
        || c >= 0x80
}

/// `/a/b`, `/a.log`, `~/x`, `C:\x`, `\\server\share`, at the start of the line or after a
/// space, a quote, a bracket or `=`. A slash path needs a second slash or an extension,
/// so `/api` and `1/2` stay text.
fn path(b: &[u8], at: usize) -> Option<usize> {
    let after_gap = at == 0
        || matches!(
            b[at - 1],
            b' ' | b'\t' | b'"' | b'\'' | b'(' | b'[' | b'<' | b'=' | b',' | b':' | b'`'
        );
    if !after_gap {
        return None;
    }
    let c = b[at];
    let next = b.get(at + 1).copied();
    let body = match c {
        b'/' if next.is_some_and(|n| {
            n.is_ascii_alphanumeric() || matches!(n, b'.' | b'_') || n >= 0x80
        }) =>
        {
            at + 1
        }
        b'~' if next == Some(b'/') => at + 2,
        b'\\'
            if next == Some(b'\\') && b.get(at + 2).is_some_and(|n| n.is_ascii_alphanumeric()) =>
        {
            at + 2
        }
        c if c.is_ascii_alphabetic() && next == Some(b':') && b.get(at + 2) == Some(&b'\\') => {
            at + 3
        }
        _ => return None,
    };
    let mut end = body;
    while end < b.len() && path_byte(b[end]) {
        end += 1;
    }
    while end > body && matches!(b[end - 1], b'.' | b',' | b'=') {
        end -= 1;
    }
    if c == b'/' {
        let tail = &b[body..end];
        let last = tail.iter().rposition(|x| *x == b'/');
        let second_slash = last.is_some();
        let name = &tail[last.map_or(0, |i| i + 1)..];
        let extension = name
            .iter()
            .rposition(|x| *x == b'.')
            .is_some_and(|i| i > 0 && name.get(i + 1).is_some_and(|n| n.is_ascii_alphanumeric()));
        if !second_slash && !extension {
            return None;
        }
    }
    if c == b'\\' && !b[body..end].contains(&b'\\') {
        return None;
    }
    (end > body || c != b'/').then_some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(line: &str) -> Vec<(&str, TokenKind)> {
        tokens(line, TokenKinds::ALL)
            .into_iter()
            .map(|(s, e, k)| (&line[s..e], k))
            .collect()
    }

    #[test]
    fn access_log_row() {
        assert_eq!(
            found("10.0.4.17 GET https://api.example.com/v1/orders 503 in 1.25s"),
            vec![
                ("10.0.4.17", TokenKind::Ip),
                ("https://api.example.com/v1/orders", TokenKind::Url),
                ("1.25s", TokenKind::Duration),
            ]
        );
    }

    #[test]
    fn not_everything_is_a_token() {
        assert!(found("version 1.2.3 took 1/2 of the budget at 14:02:05").is_empty());
        assert!(found("build 2026-09-28 id 42 port 8080 v1.2.3.4.5").is_empty());
        assert!(found("5min 10days x86_64 abc5s 1.2.3.4.5").is_empty());
        assert!(found("std::vector Foo::bar crate::x").is_empty());
        assert!(found("mac 00:1a:2b:3c:4d:5e clock 12:34:56:78").is_empty());
        assert!(found("the /api endpoint").is_empty());
    }

    #[test]
    fn ipv4_with_port_and_punctuation() {
        assert_eq!(
            found("connecting to 10.0.4.17:8443."),
            vec![("10.0.4.17:8443", TokenKind::Ip)]
        );
        assert_eq!(found("(192.168.1.1)"), vec![("192.168.1.1", TokenKind::Ip)]);
        assert!(found("256.1.1.1").is_empty());
        // Versions with four parts are not addresses; the bind-all address is.
        assert!(found("Chrome 118.0.0.0 build 2.1.0.0").is_empty());
        assert_eq!(
            found("listening on 0.0.0.0:8080"),
            vec![("0.0.0.0:8080", TokenKind::Ip)]
        );
        assert!(found("10.0.0.1abc").is_empty());
    }

    #[test]
    fn ipv6_forms() {
        assert_eq!(found("from ::1 ok"), vec![("::1", TokenKind::Ip)]);
        assert_eq!(
            found("fe80::1ff:fe23:4567:890a"),
            vec![("fe80::1ff:fe23:4567:890a", TokenKind::Ip)]
        );
        assert_eq!(
            found("peer 2001:0db8:85a3:0000:0000:8a2e:0370:7334 up"),
            vec![("2001:0db8:85a3:0000:0000:8a2e:0370:7334", TokenKind::Ip)]
        );
        assert_eq!(
            found("at [2001:db8::1]:8080 now"),
            vec![("[2001:db8::1]:8080", TokenKind::Ip)]
        );
        assert_eq!(
            found("host fe80::1: down"),
            vec![("fe80::1", TokenKind::Ip)]
        );
        // Hex-only words around `::` are text; three groups or a digit make an address.
        assert!(found("dead::beef abc::def").is_empty());
        assert_eq!(found("fe::ab::cd").len(), 0);
        assert_eq!(found("a::b:c ok"), vec![("a::b:c", TokenKind::Ip)]);
    }

    /// A long run of hex groups and colons (a hex dump, a MAC table) costs a constant
    /// per byte: every position is a token start, but an address is at most 39 bytes.
    #[test]
    fn long_hex_colon_runs_scan_in_linear_time() {
        let unit = "00:01:02:03:";
        let line = unit.repeat(1_048_576 / unit.len());
        let start = std::time::Instant::now();
        assert!(tokens(&line, TokenKinds::ALL).is_empty());
        let big = start.elapsed();
        let small_line = unit.repeat(1_024 / unit.len());
        let start = std::time::Instant::now();
        for _ in 0..1024 {
            assert!(tokens(std::hint::black_box(&small_line), TokenKinds::ALL).is_empty());
        }
        let small = start.elapsed();
        // Linear: 1 MB once costs about what 1 KB costs 1024 times (quadratic would be
        // ~1000x more); the bound is loose for noisy machines and debug builds.
        assert!(
            big < small * 8 + std::time::Duration::from_millis(50),
            "{big:?} vs {small:?}"
        );
        let limit = if cfg!(debug_assertions) { 3_000 } else { 200 };
        assert!(big < std::time::Duration::from_millis(limit), "{big:?}");
        // Mixed with spaces the same: each run is skipped whole.
        let spaced = format!("{} 10.0.0.1", "ab:cd:ef:".repeat(10_000));
        assert_eq!(tokens(&spaced, TokenKinds::ALL).len(), 1);
    }

    #[test]
    fn uuids() {
        assert_eq!(
            found("req=550e8400-e29b-41d4-a716-446655440000 done"),
            vec![("550e8400-e29b-41d4-a716-446655440000", TokenKind::Uuid)]
        );
        assert!(found("550e8400-e29b-41d4-a716-44665544000").is_empty());
    }

    #[test]
    fn urls() {
        assert_eq!(
            found("see <http://x.org/a?b=1>, then"),
            vec![("http://x.org/a?b=1", TokenKind::Url)]
        );
        assert_eq!(
            found("WSS://h:9/s and file:///var/log/a.log."),
            vec![
                ("WSS://h:9/s", TokenKind::Url),
                ("file:///var/log/a.log", TokenKind::Url)
            ]
        );
        assert!(found("gopher://x").is_empty());
    }

    #[test]
    fn durations() {
        assert_eq!(
            found("took 250ms, 1.5s, 2m30s, 3µs, 10ns, 7us, 1h, 2d."),
            vec![
                ("250ms", TokenKind::Duration),
                ("1.5s", TokenKind::Duration),
                ("2m30s", TokenKind::Duration),
                ("3µs", TokenKind::Duration),
                ("10ns", TokenKind::Duration),
                ("7us", TokenKind::Duration),
                ("1h", TokenKind::Duration),
                ("2d", TokenKind::Duration),
            ]
        );
        assert_eq!(found("duration=12ms"), vec![("12ms", TokenKind::Duration)]);
    }

    #[test]
    fn paths() {
        assert_eq!(
            found("open /var/log/app.log failed"),
            vec![("/var/log/app.log", TokenKind::Path)]
        );
        assert_eq!(found("wrote /tmp/x."), vec![("/tmp/x", TokenKind::Path)]);
        assert_eq!(found("file=/app.log"), vec![("/app.log", TokenKind::Path)]);
        assert_eq!(found("~/notes.txt"), vec![("~/notes.txt", TokenKind::Path)]);
        assert_eq!(
            found(r"C:\Logs\app.log and \\srv\share\x"),
            vec![
                (r"C:\Logs\app.log", TokenKind::Path),
                (r"\\srv\share\x", TokenKind::Path)
            ]
        );
        assert!(found("a/b/c 1/2 and/or").is_empty());
    }

    #[test]
    fn kinds_are_switchable() {
        let line = "10.0.0.1 https://h/x 5s /a/b 550e8400-e29b-41d4-a716-446655440000";
        let mut kinds = TokenKinds::ALL;
        kinds.set(TokenKind::Url, false);
        kinds.set(TokenKind::Ip, false);
        let got: Vec<TokenKind> = tokens(line, kinds).into_iter().map(|t| t.2).collect();
        assert_eq!(
            got,
            vec![TokenKind::Duration, TokenKind::Path, TokenKind::Uuid]
        );
        assert!(tokens(line, TokenKinds::NONE).is_empty());
    }

    #[test]
    fn config_round_trip() {
        assert_eq!(TokenKinds::ALL.to_config(), "ip,uuid,url,duration,path");
        assert_eq!(
            TokenKinds::from_config("ip,uuid,url,duration,path"),
            TokenKinds::ALL
        );
        let mut some = TokenKinds::NONE;
        some.set(TokenKind::Url, true);
        some.set(TokenKind::Path, true);
        assert_eq!(TokenKinds::from_config(&some.to_config()), some);
        assert_eq!(TokenKinds::from_config(" URL , bogus"), {
            let mut k = TokenKinds::NONE;
            k.set(TokenKind::Url, true);
            k
        });
        assert_eq!(TokenKinds::from_config(""), TokenKinds::NONE);
    }

    #[test]
    fn stops_when_asked() {
        let mut n = 0;
        scan("1s 2s 3s 4s", TokenKinds::ALL, |_, _, _| {
            n += 1;
            n < 2
        });
        assert_eq!(n, 2);
    }

    #[test]
    fn multibyte_text_is_safe() {
        let line = "é/ü 10.0.0.1 àè /ü/ö.txt ☃ 5ms";
        for (s, e, _) in tokens(line, TokenKinds::ALL) {
            assert!(line.is_char_boundary(s) && line.is_char_boundary(e));
        }
        assert_eq!(found(line).len(), 3);
    }

    /// Micro-benchmark: the scan of a 200-byte row stays well under the 2 µs target in a
    /// release build; a debug build is only checked against a loose bound.
    #[test]
    fn scan_is_cheap() {
        let line = "2026-09-28T14:02:05.123Z INFO 10.0.4.17 GET https://api.example.com/v1/orders?id=550e8400-e29b-41d4-a716-446655440000 503 in 1.25s path=/var/log/app.log user=alice agent=curl/8.1 retry=2";
        assert!((180..=240).contains(&line.len()), "{}", line.len());
        let rounds = 20_000;
        let start = std::time::Instant::now();
        let mut total = 0;
        for _ in 0..rounds {
            scan(std::hint::black_box(line), TokenKinds::ALL, |_, _, _| {
                total += 1;
                true
            });
        }
        let per_row = start.elapsed() / rounds;
        assert!(total > 0);
        let bound = if cfg!(debug_assertions) { 100 } else { 2 };
        assert!(
            per_row < std::time::Duration::from_micros(bound),
            "{per_row:?} per row"
        );
    }
}
