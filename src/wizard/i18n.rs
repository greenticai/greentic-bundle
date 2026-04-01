pub fn render_key(key: &str, locale: &str) -> String {
    crate::i18n::tr_for(locale, key)
}

#[cfg(test)]
mod tests {
    use super::render_key;

    #[test]
    fn renders_known_wizard_key_for_requested_locale() {
        assert_eq!(render_key("wizard.menu.title", "en-US"), "Bundle Wizard");
    }

    #[test]
    fn falls_back_to_english_for_unknown_locale() {
        assert_eq!(render_key("wizard.menu.title", "zz-ZZ"), "Bundle Wizard");
    }

    #[test]
    fn returns_key_when_translation_is_missing() {
        assert_eq!(
            render_key("wizard.missing.translation.key", "en"),
            "wizard.missing.translation.key"
        );
    }
}
