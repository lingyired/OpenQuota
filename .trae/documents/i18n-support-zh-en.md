# 国际化（i18n）支持计划 — 中文 + 英文

## 摘要

为 OpenQuota01 增加国际化能力，首期支持简体中文（zh-CN）与英文（en），后续可扩展更多语言。采用用户选定的 **svelte-i18n** 库；覆盖范围：前端界面全部文案、Rust 端系统通知（pacing 提醒）与 macOS 原生托盘菜单。语言偏好（跟随系统 / 英文 / 中文）作为新设置项持久化到 `AppSettings`。

## 现状分析

- 技术栈：Tauri 2 + Svelte 5（runes）+ TypeScript，运行时依赖极少（`@tauri-apps/api`、`svelte`）。
- 所有 UI 文案硬编码英文，分布在约 20 个 `.svelte` 组件、`App.svelte`，以及 TS 辅助模块（[updateController.svelte.ts](file:///Users/lingsmbp/Documents/aiwork/OpenQuota/src/lib/updateController.svelte.ts) 的 `nextUpdateLabel`、[metricFormat.ts](file:///Users/lingsmbp/Documents/aiwork/OpenQuota/src/lib/metricFormat.ts) 的单位、[shareCard.ts](file:///Users/lingsmbp/Documents/aiwork/OpenQuota/src/lib/shareCard.ts)、[totalSpend.ts](file:///Users/lingsmbp/Documents/aiwork/OpenQuota/src/lib/totalSpend.ts)）。
- 后端下发到前端的展示字符串也是英文：provider catalog 的 metric label（"Usage Today"/"Credits"/"Session"）、quota label、status text、notice title/message、warning、usage source note。
- Rust 端用户可见文案：
  - 系统通知：`pacing.rs` 的 `PaceMilestone::title()/body()`（3 组文案）+ `notifications.rs` 组装。
  - 原生菜单：`lib.rs::install_tray()`（macOS："Settings"/"Quit OpenQuota01"；其他平台："Open OpenQuota01"/"Customize…"/"Settings…"/"Quit OpenQuota01"）。`install_tray` 在 `app.manage(settings)` 之后调用，可直接读取语言偏好。
- 设置持久化：Rust [models.rs](file:///Users/lingsmbp/Documents/aiwork/OpenQuota/src-tauri/src/models.rs) 的 `AppSettings`（`#[serde(rename_all="camelCase", default)]`，新字段缺失自动取默认，无需迁移）+ TS [types.ts](file:///Users/lingsmbp/Documents/aiwork/OpenQuota/src/lib/types.ts)。`scripts/verify/verify-model-contract.js` 强制两端 `AppSettings` 字段一致。
- 测试影响：现有测试大多断言**渲染后**英文文本（默认 locale=en 时继续通过）；但 [uiLanguage.test.ts](file:///Users/lingsmbp/Documents/aiwork/OpenQuota/src/lib/uiLanguage.test.ts) 直接断言 svelte **源码**包含英文字面量，改造后必然失败，需要重写。
- 版本：当前 0.6.0（`package.json` / `Cargo.toml` / `tauri.conf.json`）。新功能 → 按项目约定升到 **0.7.0**。

## 关键设计决策

1. **库**：`svelte-i18n@^4`（支持 Svelte 5；其已知 SSR hydration 问题与本 Tauri 桌面 SPA 无关）。Markup 中用 `$_('key', params)`（ICU 占位符 `{name}`）；script 中 `_('key')` 在组件内是响应式的。
2. **语言偏好**：`LanguagePreference = 'system' | 'en' | 'zh-CN'`，默认 `system`。前端 `resolveLocale`：`system` 且 `navigator.language` 以 `zh` 开头 → `zh-CN`，否则 `en`。Rust 端用 `sys-locale` crate 做同样的系统语言探测（通知/菜单用）。
3. **后端展示字符串的本地化**：前端维护一份「英文原文 → 消息 key」词典 `backendGlossary` + `tBackend(raw)`：命中则走 i18n 翻译，未命中回退原文。这样**不改任何 Rust provider mapper**，即可覆盖 catalog/quota/status/notice/warning/source note 等全部动态文案，且新增字符串只需补词典。
4. **en 词典值 = 现有英文字面量**（保证渲染型测试不变绿、glossary 原文能对上）。
5. **原生菜单在启动时按初始语言构建**，语言切换后不热更新（记录为已知限制）。
6. **Windows taskband 短标签（"S" 等）本阶段保持英文**，列为后续阶段。
7. **品牌名**（Claude/Codex/Cursor 等 provider displayName）与用户自定义重命名不翻译。
8. **数字/货币格式**：保持现有 `Intl.NumberFormat('en-US')`（USD 本位），不随语言变化。

## 改动明细

### A. 前端 i18n 基础设施（新增）

- `package.json`：`dependencies` 增加 `svelte-i18n@^4`（`corepack pnpm add svelte-i18n`）。
- 新增 `src/lib/i18n/index.ts`：
  - `import { addMessages, init, locale, _ } from 'svelte-i18n'`；`addMessages('en', en)`、`addMessages('zh-CN', zhCn)`；`init({ initialLocale: 'en', fallbackLocale: 'en' })`（模块顶层执行，测试与运行时一致）。
  - 导出 `_`（组件直接用）、`setLanguage(pref: LanguagePreference)`（解析后 `locale.set(...)`）、`resolveLocale(pref)`、`tBackend(raw)`。
- 新增 `src/lib/i18n/messages/en.ts`：命名空间字典，**值必须与现有英文字面量逐字一致**。建议命名空间：`app` / `dashboard` / `settings` / `customize` / `provider` / `metric` / `share` / `update` / `time` / `units` / `error` / `notification`。
- 新增 `src/lib/i18n/messages/zh-CN.ts`：与 en 同结构（TS 类型强制 key 一致，svelte-check 兜底）。
- 新增 `src/lib/i18n/backendGlossary.ts`：`Record<string, keyof 消息结构>`，收录后端产出的全部已知英文字符串（从 provider mappers/registry 中收集，如 "Usage Today"、"Weekly"、"Session"、"Credits"、status/notice 文案、source note 等）。

### B. 语言设置 + 切换

- `src/lib/types.ts`：新增 `export type LanguagePreference = 'system' | 'en' | 'zh-CN';`，`AppSettings` 增加 `language: LanguagePreference;`（与 Rust 字段保持一致，契约脚本自动校验）。
- `src-tauri/src/models.rs`：新增枚举 `LanguagePreference { System, En, #[serde(rename="zh-CN")] ZhCn }`（serde 值 "system" | "en" | "zh-CN"）；`AppSettings` 增加 `#[serde(default)] pub language: LanguagePreference`；`Default` 为 `System`（`reset_all_settings` 自动回到默认）。
- `src/App.svelte`：新增 `$effect` 监听 `settingsState?.settings.language`，调用 `setLanguage(...)`；`setLanguage` 内部 `$state`-级 `locale.set()` 触发全界面响应式更新。
- `src/lib/SettingsScreen.svelte`：在 **General** 分区顶部新增 `Language` 行（`SelectMenu`）：选项 `System`（label 用 `$_('settings.language.system')`，显示 "Auto"/"自动"）、`English`、`简体中文`（语言名用本族语，不翻译）。

### C. 替换前端硬编码文案（范围）

逐个组件把模板中的英文字面量替换为 `$_('...')`，script 中需响应式的用 `_('...')`（组件内响应式），事件回调/工具函数直接 `_('...')`。涉及：

- `App.svelte`：footer、options 菜单、share 菜单、两个 ConfirmationSheet、About、错误消息（含 `{provider}` 插值）、"Reset {name}"、sr-only 文案、aria-label/tooltip。
- `Dashboard.svelte`：空态、onboarding、context 菜单、metric 菜单、错误、update 卡片。
- `SettingsScreen.svelte`：全部分区标题/行标签/选项 label/提示 tooltip。
- `CustomizeProviderList.svelte`、`CustomizeProviderDetail.svelte`、`ProviderNameSection.svelte`、`RenameProviderSheet.svelte`、`ProviderApiKeySection.svelte`、`ResetCreditsDetail.svelte`、`ModelUsageDetail.svelte`、`ConfirmationSheet.svelte`（props 由调用方传 key 后翻译，或组件内 `_()`）。
- 指标组件：`QuotaMetric.svelte`（pacing 文案、reset 文案）、`ValueMetric.svelte`、`UsageMetric.svelte`、`UsageTrend.svelte`、`TotalSpend.svelte`、`StatusMetric.svelte`、`MetricRenderer.svelte`（"No data"/"Reset unavailable"）、`ProviderLinks.svelte`、`ProviderNoticeRow.svelte`。
- TS 模块：`updateController.svelte.ts`（`nextUpdateLabel` 相对时间、"Waiting for first update" 等）、`metricFormat.ts`（units：thousand/million/billion/tokens/MTok）、`shareCard.ts`、`totalSpend.ts`、`customizationHistory.ts`（若有可见文案）。
- 后端动态字符串在**渲染点**套 `tBackend(raw)`：`MetricRenderer` 的 `definition.label`、`QuotaMetric` 的 `quota.label`、`StatusMetric` 的 `text/subtitle`、`ProviderNoticeRow` 的 `title/message`、`UsageTrend`/`MetricRenderer` 的 `sourceNote`、snapshot `warnings`、`SettingsScreen` 的 `platformSummary/integrationError`（已知固定值）。

### D. Rust 端本地化（通知 + 原生菜单）

- `src-tauri/Cargo.toml`：增加 `sys-locale`（极小、无传递依赖）用于系统语言探测。
- 新增 `src-tauri/src/i18n.rs`：
  - `pub enum Locale { En, ZhCn }` + `pub fn resolve(language: LanguagePreference) -> Locale`（System 时 `sys_locale::get_locale()` 以 `zh` 开头 → ZhCn）。
  - 小字典 + `tr(locale, key) -> &'static str` / 带参 `tr(locale, key, params)`：覆盖 `pacing.rs` 的 3 组 milestone 文案、`lib.rs` 菜单 5 条文案。
  - `lib.rs` 注册 `pub mod i18n;`。
- `src-tauri/src/notifications.rs`：`finish_refresh` 内由 `settings.get().language` 解析 locale，`deliver` 时用 `tr` 生成 title/body（`"{} · {}\n{}"` 组装逻辑保留）。
- `src-tauri/src/lib.rs::install_tray`：函数内 `let locale = i18n::resolve(app.state::<Arc<SettingsService>>().get().language);` 后按 locale 生成菜单 label（macOS 与非 macOS 两处）。

### E. 测试 + 校验 + 版本

- 新增 `src/lib/i18n.test.ts`：
  - `resolveLocale`：system+zh 浏览器 → zh-CN；system+en → en；显式偏好直接命中。
  - `tBackend`：已知串命中翻译、未知串回退原文。
  - zh-CN 字典与 en 字典 key 结构一致（类型层面已保证，可再断言嵌套 key 集合相等）。
  - 响应式切换：渲染一个含 `$_` 与 `tBackend` 的组件 → `setLanguage('zh-CN')` → 断言文本即时变化（对齐用户「切换语言无需刷新」的偏好）。
- 重写 `src/lib/uiLanguage.test.ts`：把「源码含英文字面量」断言改为「en 字典包含该文案」+ 保留视觉/样式契约断言（CSS 部分不动）。
- 既有渲染型测试（App.test.ts 等）：默认 locale=en 应继续通过；若个别测试因文案走 `_()` 后仍渲染英文而不受影响，无需改动；如有断言源码的其它用例一并调整为字典断言。`src/test/setup.ts` 若需在组件测试前保证 i18n 已 init，可 import `src/lib/i18n`（模块级 init 已覆盖），并在 `afterEach` 复位 locale 防止测试间泄漏。
- 版本号升至 **0.7.0**：`package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`（`verify:versions` 校验三者一致）。
- 契约脚本无需手改：`verify-model-contract.js` 自动比对新增 `language` 字段两端一致。

## 假设与已知限制

- `system` 语言仅支持 zh/非 zh 二元判定；更多语言后续阶段再加（结构已预留）。
- 原生菜单语言在启动时固定，切换语言后需重启才更新菜单栏文案（应用内界面实时生效）。
- Windows taskband 短标签暂不翻译。
- 不翻译 provider 品牌名、用户自定义名称、URL、数字/货币格式。

## 验证步骤

1. `corepack pnpm lint && corepack pnpm check && corepack pnpm test && corepack pnpm build`（eslint/prettier/svelte-check/vitest/build）。
2. `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --all-targets`。
3. `corepack pnpm verify:contracts`（model contract 含新字段）。
4. `corepack pnpm verify:versions`。
5. 手动：语言设为中文 → 全部界面（Dashboard/Settings/Customize/各弹层/错误提示）即时变中文；macOS 托盘菜单与系统通知为中文；切回英文恢复正常；`system` 在英文系统下显示英文。
