use serde::{Deserialize, Serialize};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SoundAlertPreset {
    #[default]
    None,
    Beep,
    Chime,
    Warning,
    Critical,
}

impl SoundAlertPreset {
    pub fn name(&self) -> &'static str {
        match self {
            SoundAlertPreset::None => "None",
            SoundAlertPreset::Beep => "Beep",
            SoundAlertPreset::Chime => "Chime",
            SoundAlertPreset::Warning => "Warning",
            SoundAlertPreset::Critical => "Critical",
        }
    }

    pub fn from_name(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "beep" => SoundAlertPreset::Beep,
            "chime" => SoundAlertPreset::Chime,
            "warning" => SoundAlertPreset::Warning,
            "critical" => SoundAlertPreset::Critical,
            _ => SoundAlertPreset::None,
        }
    }

    pub fn all() -> &'static [SoundAlertPreset] {
        &[
            SoundAlertPreset::None,
            SoundAlertPreset::Beep,
            SoundAlertPreset::Chime,
            SoundAlertPreset::Warning,
            SoundAlertPreset::Critical,
        ]
    }

    pub fn play(&self) {
        if *self == SoundAlertPreset::None {
            return;
        }
        let preset = *self;
        thread::spawn(move || {
            #[cfg(windows)]
            unsafe {
                match preset {
                    SoundAlertPreset::None => {}
                    SoundAlertPreset::Beep => {
                        MessageBeep(0x00000000); // MB_OK standard beep
                    }
                    SoundAlertPreset::Chime => {
                        MessageBeep(0x00000040); // MB_ICONASTERISK info chime
                    }
                    SoundAlertPreset::Warning => {
                        MessageBeep(0x00000030); // MB_ICONEXCLAMATION warning sound
                    }
                    SoundAlertPreset::Critical => {
                        MessageBeep(0x00000010); // MB_ICONHAND critical stop sound
                    }
                }
            }
            #[cfg(not(windows))]
            {
                print!("\x07");
            }
        });
    }
}

pub enum CyberSound {
    BeepError,
    BlipAttach,
    ChirpClick,
}

#[cfg(windows)]
extern "system" {
    fn MessageBeep(u_type: u32) -> i32;
}

pub fn play_sound(sound: CyberSound, enabled: bool) {
    if !enabled {
        return;
    }

    thread::spawn(move || {
        match sound {
            CyberSound::BeepError => {
                #[cfg(windows)]
                unsafe {
                    MessageBeep(0x00000010); // MB_ICONHAND / Stop
                }
                #[cfg(not(windows))]
                {
                    print!("\x07");
                }
            }
            CyberSound::BlipAttach => {
                #[cfg(windows)]
                unsafe {
                    MessageBeep(0x00000040); // MB_ICONASTERISK / Information
                }
                #[cfg(not(windows))]
                {
                    print!("\x07");
                }
            }
            CyberSound::ChirpClick => {
                #[cfg(windows)]
                unsafe {
                    MessageBeep(0x00000000); // MB_OK
                }
                #[cfg(not(windows))]
                {
                    print!("\x07");
                }
            }
        }
    });
}
