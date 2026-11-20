use std::{collections::HashMap, sync::LazyLock};

// The gallery uses the same build table in model-version.ts.
static BUILDS: LazyLock<HashMap<&str, &str>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../../src/lib/utils/model-builds.json"))
        .expect("valid bundled model build table")
});
static BUILD_SUFFIX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?-u:\b)([0-9a-f]{8})$").unwrap());

pub(super) fn badge_label(model: Option<&str>) -> Option<&'static str> {
    let name = model?.trim().to_ascii_lowercase();
    if name.contains("furry") && name.contains("v3") {
        return Some("v3 FR");
    }
    for (version, full, curated) in [
        ("v5", "v5 F", "v5 C"),
        ("v4.5", "v4.5 F", "v4.5 C"),
        ("v4", "v4 F", "v4 C"),
    ] {
        if name.contains(version) {
            return match (name.contains("full"), name.contains("curated")) {
                (true, false) => Some(full),
                (false, true) => Some(curated),
                _ => Some(
                    BUILD_SUFFIX
                        .captures(&name)
                        .and_then(|parts| BUILDS.get(&parts[1]).copied())
                        .unwrap_or(version),
                ),
            };
        }
    }
    name.contains("v3").then_some("v3")
}

pub(super) fn badge_key(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase()
        .trim_start_matches('v')
        .to_owned()
}
