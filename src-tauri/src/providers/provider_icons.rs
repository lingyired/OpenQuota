//! Provider brand icon sources (bundled SVG), shared by any surface that
//! renders a provider mark (Windows taskband `set_icon`, menus, …).
//!
//! The assets are monochrome `currentColor` templates, so callers typically
//! pass them to the taskband plugin with `tint: true` to paint them in the
//! current foreground colour (see `tint` in the plugin's `IconSpec`).

macro_rules! brand_icon {
    ($name:literal) => {
        include_str!(concat!(
            "../../../src/assets/provider-icons/",
            $name,
            ".svg"
        ))
    };
}

const CLAUDE: &str = brand_icon!("claude");
const CODEX: &str = brand_icon!("codex");
const COPILOT: &str = brand_icon!("copilot");
const CURSOR: &str = brand_icon!("cursor");
const DEEPSEEK: &str = brand_icon!("deepseek");
const ANTIGRAVITY: &str = brand_icon!("antigravity");
const GROK: &str = brand_icon!("grok");
const INFINI: &str = brand_icon!("infini");
const OPENCODE: &str = brand_icon!("opencode");
const OPENROUTER: &str = brand_icon!("openrouter");
const SILICONFLOW: &str = brand_icon!("siliconflow");
const ZAI: &str = brand_icon!("zai");
const KIMI: &str = brand_icon!("kimi");
const MINIMAX: &str = brand_icon!("minimax");
const TRAE: &str = brand_icon!("trae");
const WORKBUDDY: &str = brand_icon!("workbuddy");
const COMMANDCODE: &str = brand_icon!("commandcode");

/// Raw SVG source of a provider's brand icon, or `None` if none is bundled.
/// Keyed by provider family, so account ids (`id@…`) share the family's mark.
pub(crate) fn provider_icon_svg(provider_id: &str) -> Option<&'static str> {
    match super::provider_family(provider_id) {
        "claude" => Some(CLAUDE),
        "codex" => Some(CODEX),
        "copilot" => Some(COPILOT),
        "cursor" => Some(CURSOR),
        "deepseek" => Some(DEEPSEEK),
        "antigravity" => Some(ANTIGRAVITY),
        "grok" => Some(GROK),
        "infini" => Some(INFINI),
        "opencode" => Some(OPENCODE),
        "openrouter" => Some(OPENROUTER),
        "commandcode" => Some(COMMANDCODE),
        // `provider_family` only strips the `@account` suffix, so mainland-China
        // variants (`-cn`) keep their own id here and need explicit arms. They share
        // the brand mark of their parent provider.
        "zai" | "zai-cn" => Some(ZAI),
        "kimi" | "kimi-cn" => Some(KIMI),
        "minimax" | "minimax-cn" => Some(MINIMAX),
        "trae-cn" => Some(TRAE),
        "workbuddy" | "workbuddy-cn" => Some(WORKBUDDY),
        "siliconflow" | "siliconflow-cn" => Some(SILICONFLOW),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::provider_icon_svg;

    /// Regression test for the reported bug: CommandCode's menu-bar icon never
    /// appeared. Its provider id is exactly `commandcode` (see
    /// `providers::commandcode::definition`) and `provider_family` only strips an
    /// `@account` suffix, so `provider_icon_svg` must resolve a bundled mark for
    /// it. Before the `brand_icon!("commandcode")` const and its match arm were
    /// added this returned `None`, so no `IconSpec` ever reached the menu-bar
    /// plugin's `set_leading_icon` and the item rendered with no icon.
    #[test]
    fn commandcode_provider_resolves_a_bundled_icon() {
        let icon = provider_icon_svg("commandcode")
            .expect("commandcode must resolve a bundled brand icon");
        assert!(
            icon.contains("<svg"),
            "commandcode icon should be inline SVG source the plugin can decode"
        );
        assert!(
            icon.contains("currentColor"),
            "commandcode icon should be a currentColor template so `tint: true` paints it"
        );
    }

    /// The menu-bar plugin sizes an icon to the line's cell height and preserves
    /// the declared `viewBox` aspect. CommandCode's mark is drawn flush to a
    /// 24-unit square, so with a `0 0 24 24` viewBox it filled the entire cell and
    /// rendered visibly larger than every peer (which all carry 10-20% built-in
    /// padding). The artwork is inset by widening its viewBox; this guards that
    /// padding so the icon cannot silently go edge-to-edge again.
    #[test]
    fn commandcode_icon_keeps_the_shared_viewbox_padding() {
        let icon = provider_icon_svg("commandcode").expect("commandcode icon must resolve");
        let view_box = icon
            .split("viewBox=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("commandcode icon must declare a viewBox");
        let side: f64 = view_box
            .split_whitespace()
            .nth(3)
            .and_then(|value| value.parse().ok())
            .expect("commandcode viewBox must end in a numeric side");

        // The art spans 24 user units; a viewBox at least 10% wider than that
        // guarantees the mark no longer touches every edge.
        assert!(
            side >= 24.0 * 1.10,
            "commandcode viewBox side {side} leaves no optical padding around the 24-unit art, \
             so the icon will render larger than its peers in the menu bar"
        );
    }

    /// Account-scoped ids share the family's mark, so the new arm must cover the
    /// `id@account` form the same way every other family does.
    #[test]
    fn commandcode_account_scoped_id_shares_the_family_icon() {
        assert_eq!(
            provider_icon_svg("commandcode@deadbeef"),
            provider_icon_svg("commandcode")
        );
    }
}
