use std::{collections::HashMap, sync::OnceLock};

pub fn text(language: &str, source: &str) -> String {
    static ENGLISH: OnceLock<HashMap<String, String>> = OnceLock::new();
    if language != "en-US" {
        return source.into();
    }
    ENGLISH
        .get_or_init(|| {
            serde_json::from_str(include_str!("../ui/locales/en.json")).expect("valid locale")
        })
        .get(source)
        .map(String::as_str)
        .unwrap_or(source)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_dialogs_follow_language_and_unknown_errors_stay_readable() {
        assert_eq!(text("en-US", "Закрыть"), "Close");
        assert_eq!(text("ru-RU", "Закрыть"), "Закрыть");
        assert_eq!(text("en-US", "HRESULT 0x1234"), "HRESULT 0x1234");
    }
}
