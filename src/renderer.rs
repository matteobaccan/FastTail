//! Rendering backend selection (OpenGL via `glow`, or `wgpu`) and the description of
//! the backend the running window ended up on.
//!
//! Resolution order: `FASTTAIL_RENDERER` environment variable, then the `renderer` key in
//! `fasttail.ini`, then `auto`. `auto` tries OpenGL first and retries with wgpu when eframe
//! fails to start the OpenGL backend.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// What the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RendererChoice {
    #[default]
    Auto,
    Glow,
    Wgpu,
}

impl RendererChoice {
    pub const ALL: [RendererChoice; 3] = [
        RendererChoice::Auto,
        RendererChoice::Glow,
        RendererChoice::Wgpu,
    ];

    /// Parses `auto`, `glow`/`gl`/`opengl` or `wgpu` (case-insensitive); anything else is `None`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "glow" | "gl" | "opengl" => Some(Self::Glow),
            "wgpu" => Some(Self::Wgpu),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Glow => "glow",
            Self::Wgpu => "wgpu",
        }
    }

    /// Environment variable first, then the configured value, then `auto`.
    pub fn resolve(env_value: Option<&str>, configured: RendererChoice) -> RendererChoice {
        env_value.and_then(Self::parse).unwrap_or(configured)
    }

    pub fn from_env(configured: RendererChoice) -> RendererChoice {
        let env = std::env::var("FASTTAIL_RENDERER").ok();
        Self::resolve(env.as_deref(), configured)
    }
}

/// The backend a window is actually running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererKind {
    Glow,
    Wgpu,
    Unknown,
}

/// Set by `main` before the window starts, read by the app when it is created:
/// `true` when OpenGL failed and wgpu was started instead.
static FALLBACK_USED: AtomicBool = AtomicBool::new(false);
/// 0 = unknown, 1 = glow, 2 = wgpu: which backend `main` is starting right now.
static STARTING: AtomicU8 = AtomicU8::new(0);

pub fn mark_starting(kind: RendererKind, fallback: bool) {
    STARTING.store(
        match kind {
            RendererKind::Glow => 1,
            RendererKind::Wgpu => 2,
            RendererKind::Unknown => 0,
        },
        Ordering::SeqCst,
    );
    FALLBACK_USED.store(fallback, Ordering::SeqCst);
}

pub fn fallback_used() -> bool {
    FALLBACK_USED.load(Ordering::SeqCst)
}

/// Set once the application object exists: an error returned by eframe after this point
/// comes from the running window, not from backend creation, so no fallback applies.
static APP_CREATED: AtomicBool = AtomicBool::new(false);

pub fn mark_app_created() {
    APP_CREATED.store(true, Ordering::SeqCst);
}

pub fn app_created() -> bool {
    APP_CREATED.load(Ordering::SeqCst)
}

/// Description of the active backend, built once when the app is created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRenderer {
    pub kind: RendererKind,
    /// Graphics API: `Dx12`, `Vulkan`, `Metal`, `Gl` for wgpu; the GL version string for glow.
    pub api: String,
    /// Adapter / GPU name.
    pub adapter: String,
    /// Driver description when the backend reports one.
    pub driver: String,
    /// True when OpenGL failed and this backend was started instead.
    pub fallback: bool,
}

impl ActiveRenderer {
    pub fn unknown() -> Self {
        Self {
            kind: RendererKind::Unknown,
            api: String::new(),
            adapter: String::new(),
            driver: String::new(),
            fallback: false,
        }
    }

    pub fn new(kind: RendererKind, api: &str, adapter: &str, driver: &str, fallback: bool) -> Self {
        Self {
            kind,
            api: api.trim().to_string(),
            adapter: adapter.trim().to_string(),
            driver: driver.trim().to_string(),
            fallback,
        }
    }

    /// Reads the backend from the eframe creation context.
    pub fn from_creation_context(cc: &eframe::CreationContext<'_>) -> Self {
        let fallback = fallback_used();
        if let Some(state) = &cc.wgpu_render_state {
            let info = state.adapter.get_info();
            return Self::new(
                RendererKind::Wgpu,
                &format!("{:?}", info.backend),
                &info.name,
                &info.driver_info,
                fallback,
            );
        }
        if let Some(gl) = &cc.gl {
            use eframe::glow::HasContext as _;
            // SAFETY: plain parameter queries on the live context egui created for us.
            let (renderer, version) = unsafe {
                (
                    gl.get_parameter_string(eframe::glow::RENDERER),
                    gl.get_parameter_string(eframe::glow::VERSION),
                )
            };
            return Self::new(RendererKind::Glow, &version, &renderer, "", fallback);
        }
        let mut r = Self::unknown();
        r.fallback = fallback;
        r
    }

    /// Short chip text for the status bar: `GL`, `WGPU`, `WGPU fallback`.
    pub fn chip(&self) -> String {
        let base = match self.kind {
            RendererKind::Glow => "GL",
            RendererKind::Wgpu => "WGPU",
            RendererKind::Unknown => "?",
        };
        if self.fallback {
            format!("{base} fallback")
        } else {
            base.to_string()
        }
    }

    /// One-line detail: `Dx12 · NVIDIA GeForce RTX 3060 · 31.0.15.3667`.
    pub fn details(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if !self.api.is_empty() {
            parts.push(&self.api);
        }
        if !self.adapter.is_empty() {
            parts.push(&self.adapter);
        }
        if !self.driver.is_empty() {
            parts.push(&self.driver);
        }
        if parts.is_empty() {
            "unknown".to_string()
        } else {
            parts.join(" · ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_choices_case_insensitively() {
        assert_eq!(RendererChoice::parse("AUTO"), Some(RendererChoice::Auto));
        assert_eq!(RendererChoice::parse(" gl "), Some(RendererChoice::Glow));
        assert_eq!(RendererChoice::parse("OpenGL"), Some(RendererChoice::Glow));
        assert_eq!(RendererChoice::parse("wgpu"), Some(RendererChoice::Wgpu));
        assert_eq!(RendererChoice::parse("vulkan"), None);
    }

    #[test]
    fn env_wins_over_config_and_bad_env_is_ignored() {
        assert_eq!(
            RendererChoice::resolve(Some("wgpu"), RendererChoice::Glow),
            RendererChoice::Wgpu
        );
        assert_eq!(
            RendererChoice::resolve(Some("nonsense"), RendererChoice::Glow),
            RendererChoice::Glow
        );
        assert_eq!(
            RendererChoice::resolve(None, RendererChoice::Auto),
            RendererChoice::Auto
        );
    }

    #[test]
    fn chip_and_details_formatting() {
        let r = ActiveRenderer::new(
            RendererKind::Wgpu,
            "Dx12",
            "Microsoft Basic Render Driver",
            "",
            true,
        );
        assert_eq!(r.chip(), "WGPU fallback");
        assert_eq!(r.details(), "Dx12 · Microsoft Basic Render Driver");

        let g = ActiveRenderer::new(RendererKind::Glow, "4.6.0 NVIDIA", "GeForce", "", false);
        assert_eq!(g.chip(), "GL");
        assert_eq!(g.details(), "4.6.0 NVIDIA · GeForce");
        assert_eq!(ActiveRenderer::unknown().details(), "unknown");
    }
}
