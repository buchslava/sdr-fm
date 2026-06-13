const SPELLCHECK_KEYS: &[&str] = &[
    "WebContinuousSpellCheckingEnabled",
    "WebAutomaticSpellingCorrectionEnabled",
    "WebGrammarCheckingEnabled",
    "WebAutomaticTextReplacementEnabled",
    "WebSmartInsertDeleteEnabled",
    "NSAutomaticSpellingCorrectionEnabled",
    "NSAutomaticQuoteSubstitutionEnabled",
    "NSAutomaticDashSubstitutionEnabled",
    "NSSpellCheckerAutomaticallyIdentifiesLanguages",
];

#[cfg(target_os = "macos")]
pub fn disable_webview_spellcheck() {
    use objc2_foundation::{NSUserDefaults, NSString};

    let defaults = NSUserDefaults::standardUserDefaults();
    for key_str in SPELLCHECK_KEYS {
        let key = NSString::from_str(key_str);
        defaults.setBool_forKey(false, &key);
    }
    defaults.synchronize();

    // WebKit also reads app-scoped defaults for the bundle id.
    let _ = std::process::Command::new("defaults")
        .args([
            "write",
            "com.sdrfm.app",
            "WebContinuousSpellCheckingEnabled",
            "-bool",
            "false",
        ])
        .status();
}

#[cfg(not(target_os = "macos"))]
pub fn disable_webview_spellcheck() {}
