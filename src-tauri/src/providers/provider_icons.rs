//! Provider brand icon sources (bundled SVG), shared by any surface that
//! renders a provider mark (Windows taskband `set_icon`, menus, …).
//!
//! The assets are monochrome `currentColor` templates, so callers typically
//! pass them to the taskband plugin with `tint: true` to paint them in the
//! current foreground colour (see `tint` in the plugin's `IconSpec`).

macro_rules! brand_icon {
    ($name:literal) => {
        include_str!(concat!("../../../src/assets/provider-icons/", $name, ".svg"))
    };
}

const CLAUDE: &str = brand_icon!("claude");
const CODEX: &str = brand_icon!("codex");
const COPILOT: &str = brand_icon!("copilot");
const CURSOR: &str = brand_icon!("cursor");
const DEVIN: &str = brand_icon!("devin");
const ANTIGRAVITY: &str = brand_icon!("antigravity");
const GROK: &str = brand_icon!("grok");
const OPENCODE: &str = brand_icon!("opencode");
const OPENROUTER: &str = brand_icon!("openrouter");
const ZAI: &str = brand_icon!("zai");
const KIMI: &str = brand_icon!("kimi");
const MINIMAX: &str = brand_icon!("minimax");

/// Raw SVG source of a provider's brand icon, or `None` if none is bundled.
/// Keyed by provider family, so account ids (`id@…`) share the family's mark.
pub(crate) fn provider_icon_svg(provider_id: &str) -> Option<&'static str> {
    match super::provider_family(provider_id) {
        "claude" => Some(CLAUDE),
        "codex" => Some(CODEX),
        "copilot" => Some(COPILOT),
        "cursor" => Some(CURSOR),
        "devin" => Some(DEVIN),
        "antigravity" => Some(ANTIGRAVITY),
        "grok" => Some(GROK),
        "opencode" => Some(OPENCODE),
        "openrouter" => Some(OPENROUTER),
        "zai" => Some(ZAI),
        "kimi" => Some(KIMI),
        "minimax" => Some(MINIMAX),
        _ => None,
    }
}