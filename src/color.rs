// FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
// Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
// SPDX-License-Identifier: MIT

//! A colour type free of any GUI crate, used by the engine, the themes and every front
//! end. The bytes are sRGB with premultiplied alpha, the layout egui's `Color32` uses, so
//! the GUI converts without loss and draws exactly what it drew before; a terminal reads
//! `r`, `g`, `b` of an opaque colour directly.

/// An sRGB colour with premultiplied alpha (`a == 255` is opaque, `TRANSPARENT` means
/// "keep what is behind").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Rgba = Rgba::from_rgba_premultiplied(0, 0, 0, 0);
    pub const BLACK: Rgba = Rgba::from_rgb(0, 0, 0);
    pub const WHITE: Rgba = Rgba::from_rgb(255, 255, 255);
    pub const RED: Rgba = Rgba::from_rgb(255, 0, 0);

    /// An opaque colour.
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn from_rgba_premultiplied(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// An opaque grey.
    pub const fn from_gray(l: u8) -> Self {
        Self::from_rgb(l, l, l)
    }

    /// Black with the given opacity.
    pub const fn from_black_alpha(a: u8) -> Self {
        Self::from_rgba_premultiplied(0, 0, 0, a)
    }

    pub const fn is_opaque(&self) -> bool {
        self.a == 255
    }

    /// Fades the colour: every channel, alpha included, times `factor` (0 is transparent,
    /// 1 unchanged), rounded as egui's `Color32::gamma_multiply` rounds.
    pub fn gamma_multiply(self, factor: f32) -> Self {
        debug_assert!(
            0.0 <= factor && factor.is_finite(),
            "factor should be finite, but was {factor}"
        );
        let mul = |v: u8| (v as f32 * factor + 0.5) as u8;
        Self::from_rgba_premultiplied(mul(self.r), mul(self.g), mul(self.b), mul(self.a))
    }

    /// The colour as `[r, g, b]`, alpha dropped.
    pub const fn rgb(&self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_and_constants() {
        assert_eq!(Rgba::from_rgb(1, 2, 3).rgb(), [1, 2, 3]);
        assert!(Rgba::from_rgb(1, 2, 3).is_opaque());
        assert!(!Rgba::TRANSPARENT.is_opaque());
        assert_eq!(Rgba::from_gray(7), Rgba::from_rgb(7, 7, 7));
        assert_eq!(
            Rgba::from_black_alpha(9),
            Rgba::from_rgba_premultiplied(0, 0, 0, 9)
        );
        assert_eq!(Rgba::default(), Rgba::TRANSPARENT);
    }

    #[test]
    fn gamma_multiply_fades_every_channel_with_rounding() {
        let c = Rgba::from_rgb(100, 200, 255).gamma_multiply(0.5);
        assert_eq!(c, Rgba::from_rgba_premultiplied(50, 100, 128, 128));
        assert_eq!(Rgba::WHITE.gamma_multiply(1.0), Rgba::WHITE);
        assert_eq!(Rgba::WHITE.gamma_multiply(0.0), Rgba::TRANSPARENT);
    }
}
