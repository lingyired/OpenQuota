# OpenQuota01 多语言支持设计

## 背景

OpenQuota01 当前仅支持 English 和简体中文。前端使用 `svelte-i18n`，英文词典是类型结构的源，简体中文词典与其保持同构；Rust 侧则单独为托盘菜单、系统通知和 Windows 任务栏维护少量文案。用户希望依据 OpenCode 的公开本地化信号，扩展到更适合 AI 编程工具用户的语言范围。

OpenCode 没有公开按国家/地区拆分的完整用户统计，因此语言选择采用其官方 App 已维护的 locale 覆盖范围和开发者工具的主要区域作为需求代理，而不把它宣称为精确的用户排名。

## 目标语言

最终支持以下 16 种语言，保留现有语言并新增 14 种：

| Locale | 显示名称 |
| --- | --- |
| `en` | English |
| `zh-CN` | 简体中文 |
| `zh-TW` | 繁體中文 |
| `es` | Español |
| `pt-BR` | Português (Brasil) |
| `ja` | 日本語 |
| `ko` | 한국어 |
| `de` | Deutsch |
| `fr` | Français |
| `ru` | Русский |
| `hi` | हिन्दी |
| `ar` | العربية |
| `it` | Italiano |
| `pl` | Polski |
| `tr` | Türkçe |
| `vi` | Tiếng Việt |

语言选择器使用本地名称，避免用户在当前语言未知时难以识别选项。`system` 仍然是独立选项，不算一种语言。

## 架构

### 前端词典

- 继续以 `src/lib/i18n/messages/en.ts` 作为完整消息结构和英文 fallback。
- 为每个新增 locale 创建同构消息文件，并在 `src/lib/i18n/index.ts` 注册。
- 保留现有 `t`、`tStore`、`tBackend` 和 `tBackendStore` API，不让组件感知词典实现。
- 所有新增词典必须保留英文词典中的参数占位符，例如 `{provider}`、`{percent}` 和 `{version}`。
- backend glossary 仍使用消息 key；一次补齐后端产生的所有已知英文短语，使前端和 Rust 侧用词保持一致。

### 语言偏好与系统解析

- 将 `LanguagePreference` 扩展为 `system` 加上 16 个显式 locale。
- `resolveLocale` 返回注册过的 locale；显式选择始终优先于系统语言。
- `system` 使用 navigator/OS locale 的语言前缀和区域别名匹配：例如 `zh-TW`/`zh-HK` 到繁体中文，`pt`/`pt-BR` 到巴西葡萄牙语，其余目标语言按语言前缀匹配。
- 未命中或无法读取系统语言时回退 `en`。
- Rust 的 `LanguagePreference`、locale 枚举和解析逻辑与前端保持相同的 locale 集合和 fallback 规则。

### Rust 原生文案

扩展 `src-tauri/src/i18n.rs`，覆盖现有 key 的全部目标语言：

- 托盘菜单：打开、自定义、设置、退出。
- 系统通知：打开、即将用尽、接近上限、将要用尽及通知正文。
- Windows 任务栏上下文菜单：隐藏、刷新、设置和退出。

Rust 中未知 key 或未覆盖 action 继续安全返回 key/action，避免影响启动和更新流程。

### RTL 与布局

- 阿拉伯语 locale 激活时，在应用根节点设置 `dir="rtl"`；其他 locale 使用 `ltr`。
- 只对确实依赖方向的布局做 RTL 调整，保留数字、百分比、快捷键、provider 名称和代码样式的自然方向。
- 检查设置页、下拉菜单、指标卡片、错误提示、确认弹窗和任务栏相关标签的长文本换行与溢出。
- 不为当前目标语言引入新的字体依赖，优先使用系统字体栈。

## 数据流

1. 设置服务读取 `LanguagePreference`。
2. App 启动时调用前端 `setLanguage`，按显式偏好或系统 locale 得到有效 locale。
3. `svelte-i18n` 切换当前字典，组件通过现有 reactive helper 更新。
4. Rust 启动菜单、通知和任务栏交互通过同一偏好解析有效 locale。
5. 无匹配 locale、未知消息 key 或未翻译字符串回退英文/原始字符串，不阻塞 UI。

## 测试与验收

- TypeScript 类型检查确认所有词典与英文消息结构同构。
- 词典完整性测试确认每个 locale 没有缺失 key、额外 key 或占位符不一致。
- 语言解析测试覆盖显式语言、系统语言、区域别名、未识别语言和 fallback。
- reactive switching 测试覆盖英文切换到至少一种拉丁文字语言、一种 CJK 语言和阿拉伯语。
- Rust 单元测试覆盖 locale 解析、菜单/通知/任务栏文案以及未知 key fallback。
- 运行现有前端测试、`svelte-check`、构建和 Rust 测试。
- 手动检查语言选择器、长文案、阿拉伯语 RTL 和关键弹窗布局。

## 非目标

- 不新增按地区统计、遥测或用户行为采集。
- 不把 provider 名称、模型名称、代码、URL、金额货币格式或用户自定义 provider 名称翻译成固定语言。
- 不在本次工作中扩展到 OpenCode 的全部 59 个 locale；后续可以在同一词典和完整性测试约束下由社区贡献。
- 不改变已有设置存储的语义；旧版本的 `en`、`zh-CN` 和 `system` 值必须继续可读取。

## 成功标准

用户可以在设置中选择 16 种语言中的任意一种；前端、托盘、通知和 Windows 任务栏使用对应语言；系统语言能自动命中已支持 locale；阿拉伯语界面方向正确；未翻译内容安全回退；所有现有测试和构建检查通过。
