use std::collections::BTreeMap;
use std::ffi::OsString;
use std::sync::{OnceLock, RwLock};

use anyhow::{Result, anyhow};
use unic_langid::LanguageIdentifier;

include!(concat!(env!("OUT_DIR"), "/embedded_i18n.rs"));

static CATALOGS: OnceLock<BTreeMap<String, BTreeMap<String, String>>> = OnceLock::new();
static ACTIVE_LOCALE: OnceLock<RwLock<String>> = OnceLock::new();
static SUPPORTED_LOCALES: OnceLock<Vec<String>> = OnceLock::new();

pub fn init(locale: Option<String>) {
    let resolved = select_locale(locale, &supported_locales());
    let state = ACTIVE_LOCALE.get_or_init(|| RwLock::new("en".to_string()));
    if let Ok(mut slot) = state.write() {
        *slot = resolved;
    }
}

pub fn tr(key: &str) -> String {
    tr_for(&current_locale(), key)
}

pub fn tr_for(locale: &str, key: &str) -> String {
    let catalogs = catalogs();
    for candidate in locale_chain(locale) {
        if let Some(message) = catalogs
            .get(&candidate)
            .and_then(|catalog| catalog.get(key))
        {
            return message.clone();
        }
    }
    key.to_string()
}

pub fn trf(key: &str, args: &[(&str, &str)]) -> String {
    trf_for(&current_locale(), key, args)
}

pub fn trf_for(locale: &str, key: &str, args: &[(&str, &str)]) -> String {
    let mut rendered = tr_for(locale, key);
    for (name, value) in args {
        rendered = rendered.replace(&format!("{{{name}}}"), value);
    }
    rendered
}

pub fn current_locale() -> String {
    ACTIVE_LOCALE
        .get_or_init(|| RwLock::new("en".to_string()))
        .read()
        .map(|value| value.clone())
        .unwrap_or_else(|_| "en".to_string())
}

pub fn supported_locales() -> Vec<String> {
    SUPPORTED_LOCALES
        .get_or_init(|| {
            let mut locales = catalogs().keys().cloned().collect::<Vec<_>>();
            locales.sort();
            locales
        })
        .clone()
}

pub fn select_locale(cli_locale: Option<String>, supported: &[String]) -> String {
    fn resolve(candidate: &str, supported: &[String]) -> Option<String> {
        let normalized = normalize_locale(candidate)?;
        if supported.iter().any(|entry| entry == &normalized) {
            return Some(normalized);
        }
        let base = base_language(&normalized)?;
        if supported.iter().any(|entry| entry == &base) {
            return Some(base);
        }
        None
    }

    if let Some(cli) = cli_locale.as_deref()
        && let Some(found) = resolve(cli, supported)
    {
        return found;
    }

    if let Some(env_locale) = detect_env_locale()
        && let Some(found) = resolve(&env_locale, supported)
    {
        return found;
    }

    if let Some(system_locale) = detect_system_locale()
        && let Some(found) = resolve(&system_locale, supported)
    {
        return found;
    }

    "en".to_string()
}

pub fn cli_locale_from_argv(argv: &[OsString]) -> Option<String> {
    let mut iter = argv.iter().skip(1);
    while let Some(arg) = iter.next() {
        let text = arg.to_string_lossy();
        if let Some(value) = text.strip_prefix("--locale=") {
            return Some(value.to_string());
        }
        if text == "--locale" {
            return iter.next().map(|value| value.to_string_lossy().to_string());
        }
    }
    None
}

pub fn detect_env_locale() -> Option<String> {
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn detect_system_locale() -> Option<String> {
    sys_locale::get_locale()
}

pub fn normalize_locale(raw: &str) -> Option<String> {
    let mut cleaned = raw.trim();
    if cleaned.is_empty() {
        return None;
    }
    if let Some((head, _)) = cleaned.split_once('.') {
        cleaned = head;
    }
    if let Some((head, _)) = cleaned.split_once('@') {
        cleaned = head;
    }
    let cleaned = cleaned.replace('_', "-");
    cleaned
        .parse::<LanguageIdentifier>()
        .ok()
        .map(|locale| locale.to_string())
}

pub fn base_language(tag: &str) -> Option<String> {
    tag.split('-')
        .next()
        .map(|value| value.to_ascii_lowercase())
}

pub fn locale_chain(locale: &str) -> Vec<String> {
    let Some(normalized) = normalize_locale(locale) else {
        return vec!["en".to_string()];
    };

    let mut out = vec![normalized.clone()];
    if let Some(base) = base_language(&normalized)
        && base != normalized
    {
        out.push(base);
    }
    if !out.iter().any(|entry| entry == "en") {
        out.push("en".to_string());
    }
    out
}

fn catalogs() -> &'static BTreeMap<String, BTreeMap<String, String>> {
    CATALOGS.get_or_init(load_embedded_catalogs)
}

pub fn load_catalog(locale: &str) -> Result<BTreeMap<String, String>> {
    for candidate in locale_chain(locale) {
        if let Some(catalog) = catalogs().get(&candidate) {
            return Ok(catalog.clone());
        }
    }
    Err(anyhow!(
        "{}",
        trf("errors.i18n.missing_locale", &[("locale", locale)])
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        base_language, cli_locale_from_argv, locale_chain, normalize_locale, select_locale, tr_for,
    };

    #[test]
    fn normalizes_locale_tags() {
        assert_eq!(normalize_locale("EN_us.UTF-8"), Some("en-US".to_string()));
        assert_eq!(normalize_locale("de_DE@euro"), Some("de-DE".to_string()));
        assert_eq!(normalize_locale("nl"), Some("nl".to_string()));
    }

    #[test]
    fn falls_back_to_en_language() {
        assert_eq!(locale_chain("en-US"), vec!["en-US", "en"]);
        assert_eq!(locale_chain("zz-ZZ"), vec!["zz-ZZ", "zz", "en"]);
    }

    #[test]
    fn returns_english_translation_for_unknown_locale() {
        assert_eq!(
            tr_for("zz-ZZ", "cli.root.about"),
            "Greentic bundle authoring CLI scaffold"
        );
    }

    #[test]
    fn reads_locale_from_cli_args() {
        let argv = [
            "greentic-bundle".into(),
            "--locale".into(),
            "nl-NL".into(),
            "wizard".into(),
        ];
        assert_eq!(cli_locale_from_argv(&argv), Some("nl-NL".to_string()));
    }

    #[test]
    fn selects_locale_with_base_language_fallback() {
        let supported = vec!["en".to_string(), "en-GB".to_string(), "ar".to_string()];
        assert_eq!(
            select_locale(Some("ar-SA".to_string()), &supported),
            "ar".to_string()
        );
        assert_eq!(
            select_locale(Some("en-AU".to_string()), &supported),
            "en".to_string()
        );
    }

    #[test]
    fn extracts_base_language() {
        assert_eq!(base_language("en-GB"), Some("en".to_string()));
    }
}
