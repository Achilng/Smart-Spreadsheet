//! 画风签名：把正向提示词归一化后哈希，作为“相同画风（相同提示词）”
//! 对比口径的唯一依据。只比较正向提示词；角色与负向提示词一律不参与。
//!
//! 质量词总表来自 NovelAI 官方文档（docs.novelai.net/en/image/qualitytags）
//! 与真实资料库的公共尾部 tag 实证，取**各版本并集**：多收的词两侧同样
//! 剥离、不破坏匹配公平性，漏收则跨版本匹配直接失效。
//! 修订总表或归一化算法时必须递增 `STYLE_SIGNATURE_VERSION`，
//! 启动回填会据此对全库重算。

use sha2::{Digest, Sha256};

/// 画风签名算法版本。质量词表或归一化规则变更时 +1，触发全量重算。
pub const STYLE_SIGNATURE_VERSION: u32 = 1;

/// NovelAI 官方在各版本默认追加的质量词并集（小写、内部空白已折叠）。
/// 比对前会先剥离权重外壳，因此 `{best quality}`、`1.5::very aesthetic::`
/// 等加权写法同样命中。
const QUALITY_TAGS: &[&str] = &[
    // V4.5 Full / V4.5 Curated / V5 实证尾部
    "location",
    "very aesthetic",
    "masterpiece",
    "no text",
    // V4 Full / V4 Curated / Anime V3 / Furry V3
    "best quality",
    "amazing quality",
    "absurdres",
    // V4.5 Curated / V4 Curated 追加项
    "rating:general",
];

/// 必须整串精确命中才剥离的官方默认项：V4.5 Curated 会追加
/// `-0.8::feet::`。刻意不把裸 `feet` 放进总表——它是常见语义 tag，
/// 按裸词剥离会把 genuinely 不同的提示词误判为相同画风。
const WEIGHTED_QUALITY_TAGS: &[&str] = &["-0.8::feet::"];

/// 计算一行正向提示词的画风签名。
///
/// `None`（无提示词、空提示词、剥离质量词后为空）表示该行不参与
/// “相同画风”类匹配；有内容时返回归一化结果的 SHA-256 十六进制。
pub fn style_signature_of(positive_prompt: Option<&str>) -> Option<String> {
    let prompt = positive_prompt?;
    let tags = normalized_style_tags(prompt);
    if tags.is_empty() {
        return None;
    }
    Some(format!("{:x}", Sha256::digest(tags.join(",").as_bytes())))
}

/// 归一化正向提示词为 tag 序列：
/// 按半角逗号与换行切分（全角逗号不是分隔符，与提示词字段现状一致）、
/// 首尾去空白、内部连续空白折叠为单个空格、剥离官方质量词、转小写、
/// 保持原有顺序（顺序不同视为不同提示词）。
pub fn normalized_style_tags(positive_prompt: &str) -> Vec<String> {
    positive_prompt
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split([',', '\n'])
        .filter(|raw| !raw.trim().is_empty())
        .map(|raw| raw.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase())
        .filter(|tag| !is_quality_tag(tag))
        .collect()
}

fn is_quality_tag(collapsed_lowercase: &str) -> bool {
    WEIGHTED_QUALITY_TAGS.contains(&collapsed_lowercase)
        || QUALITY_TAGS.contains(&strip_weight_shell(collapsed_lowercase))
}

/// 剥离 NovelAI 权重外壳：嵌套的 `{...}` / `[...]` 逐层剥，
/// 数值权重 `N::内容::`（允许负数与小数）剥前后缀。
fn strip_weight_shell(mut tag: &str) -> &str {
    loop {
        let mut changed = false;
        while let Some((open, close)) = tag.chars().next().zip(tag.chars().next_back())
            && matches!((open, close), ('{', '}') | ('[', ']'))
        {
            tag = tag[open.len_utf8()..tag.len() - close.len_utf8()].trim();
            changed = true;
        }
        if let Some(index) = tag.find("::")
            && tag.len() >= index + 4
            && tag.ends_with("::")
        {
            let prefix = tag[..index].trim();
            if !prefix.is_empty() && prefix.parse::<f32>().is_ok() {
                tag = tag[index + 2..tag.len() - 2].trim();
                changed = true;
            }
        }
        if !changed {
            return tag;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_stable_and_order_sensitive() {
        let a = style_signature_of(Some("artist:a, blue hair, smile"));
        let b = style_signature_of(Some("artist:a, blue hair, smile"));
        let c = style_signature_of(Some("blue hair, artist:a, smile"));
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.is_some_and(|value| value.len() == 64));
    }

    #[test]
    fn official_quality_words_are_stripped_across_versions() {
        // Anime V3 / V4 默认尾部
        let base = style_signature_of(Some("artist:a, blue hair"));
        let v3 = style_signature_of(Some(
            "artist:a, blue hair, best quality, amazing quality, very aesthetic, absurdres",
        ));
        // V4.5 Full 默认尾部（含 location）
        let v45 = style_signature_of(Some(
            "artist:a, blue hair, location, very aesthetic, masterpiece, no text",
        ));
        // Furry V3 花括号形式
        let furry = style_signature_of(Some("artist:a, blue hair, {best quality}, {amazing quality}"));
        // V4.5 Curated 尾部（含 rating:general 与加权 feet）
        let curated = style_signature_of(Some(
            "artist:a, blue hair, location, masterpiece, no text, -0.8::feet::, rating:general",
        ));
        assert_eq!(base, v3);
        assert_eq!(base, v45);
        assert_eq!(base, furry);
        assert_eq!(base, curated);
    }

    #[test]
    fn weighted_quality_words_are_stripped() {
        let base = style_signature_of(Some("artist:a, blue hair"));
        let weighted = style_signature_of(Some("artist:a, 1.5::very aesthetic::, blue hair"));
        let nested = style_signature_of(Some("artist:a, {{masterpiece}}, blue hair"));
        assert_eq!(base, weighted);
        assert_eq!(base, nested);
    }

    #[test]
    fn genuine_feet_tag_is_kept_but_curated_default_form_is_stripped() {
        let with_feet = normalized_style_tags("1girl, feet");
        assert_eq!(with_feet, vec!["1girl", "feet"]);
        let curated = normalized_style_tags("1girl, -0.8::feet::");
        assert_eq!(curated, vec!["1girl"]);
    }

    #[test]
    fn full_width_comma_is_not_a_separator_but_newlines_are() {
        let tags = normalized_style_tags("red，blue\ngreen");
        assert_eq!(tags, vec!["red，blue", "green"]);
        let crlf = normalized_style_tags("red\r\nblue\rgreen");
        assert_eq!(crlf, vec!["red", "blue", "green"]);
    }

    #[test]
    fn whitespace_case_and_empty_tags_are_normalized() {
        let tags = normalized_style_tags("  Red   Hair , , BLUE\tEyes ,, smile ");
        assert_eq!(tags, vec!["red hair", "blue eyes", "smile"]);
    }

    #[test]
    fn empty_and_pure_quality_prompts_have_no_signature() {
        assert_eq!(style_signature_of(None), None);
        assert_eq!(style_signature_of(Some("")), None);
        assert_eq!(style_signature_of(Some("   ")), None);
        assert_eq!(
            style_signature_of(Some(
                ", best quality, amazing quality, very aesthetic, absurdres"
            )),
            None
        );
    }

    #[test]
    fn non_quality_semantic_tags_are_never_stripped() {
        let tags = normalized_style_tags("highres, ultra-detailed, location scouting, text message");
        assert_eq!(tags, vec![
            "highres",
            "ultra-detailed",
            "location scouting",
            "text message"
        ]);
    }
}
