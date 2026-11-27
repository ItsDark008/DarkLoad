//! Interface language detection and the few strings the backend shows on its own
//! (desktop notifications). Everything else is translated by the frontend.

use crate::settings::Language;

/// The operating system interface language, limited to the languages DarkLoad ships.
pub fn system_language() -> Language {
    os_language_tag()
        .as_deref()
        .map(language_from_tag)
        .unwrap_or(Language::En)
}

/// Maps a locale identifier such as `it_IT.UTF-8`, `it-CH` or `en-US` to a supported language.
pub fn language_from_tag(tag: &str) -> Language {
    let base = tag
        .split(['-', '_', '.', '@'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match base.as_str() {
        "it" => Language::It,
        _ => Language::En,
    }
}

#[cfg(windows)]
fn os_language_tag() -> Option<String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // SAFETY: takes no arguments and only returns the user's UI language identifier.
    let lang_id = unsafe { GetUserDefaultUILanguage() };
    // The low 10 bits hold the primary language; 0x10 is LANG_ITALIAN.
    Some(if lang_id & 0x3ff == 0x10 { "it" } else { "en" }.to_string())
}

#[cfg(not(windows))]
fn os_language_tag() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.is_empty() && value != "C" && value != "POSIX")
}

/// Title of the notification shown when a download finishes.
pub fn download_complete(language: Language) -> &'static str {
    match language.resolve() {
        Language::It => "Download completato",
        _ => "Download complete",
    }
}

/// Title of the notification shown when a download fails.
pub fn download_failed(language: Language) -> &'static str {
    match language.resolve() {
        Language::It => "Download non riuscito",
        _ => "Download failed",
    }
}

/// Labels of the selection controls injected into the YouTube browser, as a JSON object.
pub fn browser_text(language: Language) -> String {
    let (select, deselect, selected) = match language.resolve() {
        Language::It => (
            "Seleziona per DarkLoad",
            "Rimuovi dalla selezione di DarkLoad",
            "Selezionato in DarkLoad",
        ),
        _ => (
            "Select for DarkLoad",
            "Remove from DarkLoad selection",
            "Selected in DarkLoad",
        ),
    };
    serde_json::json!({ "select": select, "deselect": deselect, "selected": selected }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_locale_tags() {
        assert_eq!(language_from_tag("it_IT.UTF-8"), Language::It);
        assert_eq!(language_from_tag("it-CH"), Language::It);
        assert_eq!(language_from_tag("IT"), Language::It);
        assert_eq!(language_from_tag("en_US.UTF-8"), Language::En);
        assert_eq!(language_from_tag("de-DE"), Language::En);
        assert_eq!(language_from_tag(""), Language::En);
    }

    #[test]
    fn notification_titles_follow_the_language() {
        assert_eq!(download_complete(Language::It), "Download completato");
        assert_eq!(download_failed(Language::En), "Download failed");
    }

    #[test]
    fn browser_text_is_a_json_object() {
        let text: serde_json::Value = serde_json::from_str(&browser_text(Language::It)).unwrap();
        assert_eq!(text["select"], "Seleziona per DarkLoad");
        assert!(text["deselect"].is_string() && text["selected"].is_string());
    }
}
