pub fn render_key(key: &str, locale: &str) -> String {
    crate::i18n::tr_for(locale, key)
}
