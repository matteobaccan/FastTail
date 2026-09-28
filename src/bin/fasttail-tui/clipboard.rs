//! Copying the selected rows: the system clipboard through `arboard` (already in the
//! dependency tree through egui), else an OSC 52 escape that asks the terminal to set
//! the clipboard (Windows Terminal, xterm, kitty, WezTerm, tmux with `set-clipboard`;
//! it also works over SSH, where no local clipboard is reachable).

use std::io::Write;

/// How the text reached the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Copied {
    System,
    Osc52,
}

/// Holds the system clipboard open: on X11 the copied text lives only as long as its
/// owner, so the handle is kept for the whole session.
#[derive(Default)]
pub struct Clipboard {
    system: Option<arboard::Clipboard>,
    tried: bool,
}

impl Clipboard {
    pub fn copy(&mut self, text: &str) -> std::io::Result<Copied> {
        if !self.tried {
            self.tried = true;
            self.system = arboard::Clipboard::new().ok();
        }
        if let Some(cb) = self.system.as_mut() {
            if cb.set_text(text.to_string()).is_ok() {
                return Ok(Copied::System);
            }
        }
        let mut out = std::io::stdout();
        out.write_all(osc52(text).as_bytes())?;
        out.flush()?;
        Ok(Copied::Osc52)
    }
}

/// The OSC 52 sequence that sets the clipboard to `text`.
pub fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// Standard base64 with padding (the only encoding OSC 52 takes).
pub fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
    }
}
