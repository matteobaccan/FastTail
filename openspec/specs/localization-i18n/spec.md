# Localization and i18n Specification

## Purpose
Provides a fully localized interface in sixteen languages, with OS language detection, English fallback, per-script font loading for the CJK languages and a runtime language switcher.

## Requirements

### Requirement: Multi-language interface support
The application SHALL support 16 languages for all UI labels, menus, settings, migration prompts, and tooltips: English (`en`), German (`de`), Spanish (`es`), French (`fr`), Italian (`it`), Dutch (`nl`), Polish (`pl`), Portuguese – Brazil (`pt-BR`), Turkish (`tr`), Russian (`ru`), Ukrainian (`uk`), Japanese (`ja`), Korean (`ko`), Chinese – simplified (`zh`), Chinese – traditional (`zh-TW`) and Friulian (`fur`). Every language SHALL carry a translation for every interface key; a key that falls back to English in a non-English language is a defect.

#### Scenario: Displaying UI in Italian
- **WHEN** the language is set to Italian
- **THEN** navigation menus, buttons, status indicators, and settings are displayed in Italian.

#### Scenario: Displaying UI in a CJK language
- **WHEN** the language is set to Chinese (simplified or traditional), Japanese or Korean
- **THEN** all UI labels are rendered with the glyphs of that script, using the system font loaded for it rather than empty replacement boxes.

#### Scenario: Traditional and simplified Chinese are distinct languages
- **WHEN** the user selects 中文 (繁體)
- **THEN** the interface uses traditional characters and Taiwanese terminology, independently of the simplified Chinese translation.

### Requirement: Language detection and fallback
The application SHALL automatically detect the host operating system language on first launch, and SHALL fall back gracefully to English whenever a translation key is missing or an unsupported locale is encountered. Detection SHALL read `LC_ALL`, `LC_MESSAGES` and `LANG` (ignoring any codeset or modifier suffix such as `.UTF-8`) and, on Windows, the user default UI language; language tags SHALL be matched as BCP-47 tags, so a region subtag never prevents a match.

#### Scenario: First launch on Italian OS
- **WHEN** FastTail launches for the first time on a machine with Italian locale (`it-IT`, `it_IT.UTF-8` or `it`)
- **THEN** the interface initializes automatically in Italian.

#### Scenario: Traditional Chinese locales resolve before the generic Chinese prefix
- **WHEN** the detected locale is `zh-TW`, `zh-HK`, `zh-MO` or `zh-Hant` (or the Windows UI language is `0x0404`, `0x0c04` or `0x1404`)
- **THEN** the interface initializes in traditional Chinese, while any other `zh` locale initializes in simplified Chinese.

#### Scenario: Missing translation or unknown locale
- **WHEN** FastTail runs on a system with an unsupported locale or encounters an undefined translation key
- **THEN** it displays the standard English text without crashing or showing blank labels.

### Requirement: Per-script font loading
The application SHALL load a fallback font for each CJK script the host provides — simplified Chinese, traditional Chinese, Japanese and Korean — and append them all to the proportional and monospace families, because no single CJK face covers the other scripts.

#### Scenario: Japanese and Korean on a machine that also has a Chinese font
- **WHEN** the host provides Microsoft YaHei, a Japanese face and Malgun Gothic
- **THEN** kana and hangul are rendered by their own faces instead of falling back to replacement boxes, whatever the interface language is.

### Requirement: Runtime language switcher
The application SHALL provide an explicit language selector in the settings, allowing users to switch languages instantly and persisting their choice in `fasttail.ini`. The selector SHALL be a drop-down list, so the settings row keeps its height as languages are added, and its first entry SHALL be "system language", which keeps the interface following the operating system (`language_auto`) and names the language currently detected. A fresh installation SHALL start in that mode; a configuration written before the setting existed SHALL keep the language it stored.

#### Scenario: Following the system language
- **WHEN** the user selects the "system language" entry and later the operating system language changes
- **THEN** the interface follows it at the next start, without the user touching the setting again.

#### Scenario: Switching language at runtime
- **WHEN** the user selects "Español" in the language drop-down
- **THEN** the entire UI re-renders immediately in Spanish without requiring an application restart.

### Requirement: Translation coverage is enforced by tests
The test suite SHALL fail when a language is missing an interface key, and SHALL fail when a language repeats the English text for a significant share of the keys, which is what an untranslated block looks like once the English fallback hides it.

#### Scenario: A language block is added without its translations
- **WHEN** a new language is declared but its strings still resolve to the English fallback
- **THEN** the i18n coverage test fails and names the language.
