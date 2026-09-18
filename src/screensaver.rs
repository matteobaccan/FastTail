use egui::{Color32, FontId, Pos2, Rect};
use std::time::{Duration, Instant};

/// Frame interval of the animation: 30 fps is fluid enough for falling glyphs, and the
/// cost of the animation is proportional to the frame rate (measured on wgpu: about 5% of
/// a core at 5 fps, 18% at 30 fps, for a 1400x900 window).
pub const FRAME_INTERVAL: Duration = Duration::from_millis(33);

struct MatrixColumn {
    x: f32,
    y: f32,
    speed: f32,
    length: usize,
    chars: Vec<char>,
}

pub struct MatrixScreensaver {
    pub is_active: bool,
    pub last_input_time: Instant,
    columns: Vec<MatrixColumn>,
    last_frame: Instant,
}

const GLYPHS: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F', 'X', 'Y', 'Z',
    'ｦ', 'ｱ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｹ', 'ｺ', 'ｻ', 'ｼ', 'ｽ', 'ｾ', 'ｿ', 'ﾀ', 'ﾂ', 'ﾃ', 'ﾅ', 'ﾆ',
    'ﾇ', 'ﾈ', '#', '@', '%', '&', '*', '+', '<', '>', '=', ':',
];

impl Default for MatrixScreensaver {
    fn default() -> Self {
        Self {
            is_active: false,
            last_input_time: Instant::now(),
            columns: Vec::new(),
            last_frame: Instant::now(),
        }
    }
}

impl MatrixScreensaver {
    pub fn on_user_input(&mut self) {
        self.last_input_time = Instant::now();
        self.is_active = false;
    }

    /// Activates the screensaver after `timeout_mins` without input. A timeout of 0
    /// disables it. The window must have the focus: a window that sits behind another
    /// one or on another desktop receives no input and would otherwise start the
    /// animation unseen, burning CPU in the background; losing the focus while the
    /// animation runs stops it for the same reason.
    pub fn check_inactivity(&mut self, timeout_mins: u32, enabled: bool, window_focused: bool) {
        if !enabled || timeout_mins == 0 || !window_focused {
            self.is_active = false;
            return;
        }

        let idle_duration = self.last_input_time.elapsed().as_secs();
        let timeout_secs = (timeout_mins as u64) * 60;

        if idle_duration >= timeout_secs {
            self.is_active = true;
        }
    }

    pub fn render(&mut self, ctx: &egui::Context, viewport: Rect) {
        if !self.is_active {
            return;
        }

        // Keep the animation going at a bounded frame rate: without vsync on the OpenGL
        // path an unconditional request_repaint would spin as fast as the GPU allows.
        ctx.request_repaint_after(FRAME_INTERVAL);

        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("matrix_screensaver"),
        ));

        // Draw deep pure black semi-transparent backdrop to clear prior frame trails
        painter.rect_filled(viewport, 0.0, Color32::from_black_alpha(245));

        let col_width = 18.0;
        let num_cols = (viewport.width() / col_width).ceil() as usize;

        if self.columns.len() != num_cols {
            self.columns = (0..num_cols)
                .map(|i| {
                    let len = 12 + (i * 7) % 18;
                    let chars = (0..len)
                        .map(|j| GLYPHS[(i * 13 + j * 5) % GLYPHS.len()])
                        .collect();
                    MatrixColumn {
                        x: viewport.min.x + (i as f32 * col_width),
                        y: ((i * 47) as f32) % viewport.height(),
                        speed: 120.0 + ((i * 31) % 160) as f32,
                        length: len,
                        chars,
                    }
                })
                .collect();
        }

        let dt = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.last_frame = Instant::now();

        let font_id = FontId::monospace(14.0);

        for col in &mut self.columns {
            col.y += col.speed * dt;
            if col.y - ((col.length as f32) * 16.0) > viewport.max.y {
                col.y = viewport.min.y - 20.0;
            }

            for j in 0..col.length {
                let char_y = col.y - (j as f32 * 16.0);
                if char_y < viewport.min.y || char_y > viewport.max.y {
                    continue;
                }

                // Randomly mutate glyphs
                if (j + col.chars.len()) % 17 == 0 {
                    col.chars[j] = GLYPHS[(col.chars[j] as usize + 3) % GLYPHS.len()];
                }

                let color = if j == 0 {
                    // Head: glowing white / bright light green
                    Color32::from_rgb(220, 255, 230)
                } else {
                    let alpha = (255.0 * (1.0 - (j as f32 / col.length as f32))) as u8;
                    Color32::from_rgba_premultiplied(0, 255, 65, alpha)
                };

                painter.text(
                    Pos2::new(col.x, char_y),
                    egui::Align2::LEFT_TOP,
                    col.chars[j].to_string(),
                    font_id.clone(),
                    color,
                );
            }
        }

        // Subtitle banner at bottom center
        painter.text(
            Pos2::new(viewport.center().x, viewport.max.y - 30.0),
            egui::Align2::CENTER_CENTER,
            "FASTTAIL - MATRIX DIGITAL RAIN • PRESS ANY KEY TO RESUME",
            FontId::monospace(12.0),
            Color32::from_rgb(0, 255, 65),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle_for(secs: u64) -> MatrixScreensaver {
        MatrixScreensaver {
            last_input_time: Instant::now() - Duration::from_secs(secs),
            ..Default::default()
        }
    }

    #[test]
    fn activates_after_timeout_when_focused() {
        let mut s = idle_for(120);
        s.check_inactivity(1, true, true);
        assert!(s.is_active);
    }

    #[test]
    fn stays_off_before_timeout() {
        let mut s = idle_for(30);
        s.check_inactivity(1, true, true);
        assert!(!s.is_active);
    }

    #[test]
    fn timeout_zero_means_off() {
        let mut s = idle_for(3600);
        s.check_inactivity(0, true, true);
        assert!(!s.is_active);
    }

    #[test]
    fn does_not_start_without_focus_and_stops_on_focus_loss() {
        let mut s = idle_for(3600);
        s.check_inactivity(1, true, false);
        assert!(
            !s.is_active,
            "an unfocused window must not start the animation"
        );

        s.check_inactivity(1, true, true);
        assert!(s.is_active);
        s.check_inactivity(1, true, false);
        assert!(!s.is_active, "losing the focus stops the animation");
    }

    #[test]
    fn user_input_stops_it() {
        let mut s = idle_for(3600);
        s.check_inactivity(1, true, true);
        assert!(s.is_active);
        s.on_user_input();
        assert!(!s.is_active);
    }
}
