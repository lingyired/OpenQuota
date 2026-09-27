# 设置、Popup 与提供商实例实施计划

> **实施时：**使用 `executing-plans` skill，逐项执行并勾选 `- [ ]`。遵循用户的最新 Luna 模型偏好，目前为 `gpt-6-luna`。

**目标：**实现三列设置、两列 popup、独立提供商实例控制，以及分档自动刷新。

**架构：**popup 与 Settings 使用独立的 Tauri 窗口；`App.svelte` 按 WebView 身份渲染数据 popup 或设置工作区，并抽取单提供商数据组件供 popup 与设置预览共用。Rust 负责窗口切换、创建并统计实际原生实例；自动刷新按提供商调度，手动刷新保留现有命令。

**技术栈：**Svelte 5、TypeScript、Tauri 2、Rust、Vitest、Cargo tests。

**设计说明：**[设置、Popup 与提供商实例设计说明](../specs/2026-09-26-settings-popup-provider-presence-design.md)

## 全局约束

- 沿用现有技术栈和视觉变量，不增加 UI 依赖。
- 保留所有语言、明暗主题、紧凑密度、RTL、键盘操作及减少动画支持。
- 保留带版本校验的设置保存路径和单提供商单飞刷新。
- 明确保存的 `taskbandProviders[id].enabled = false` 始终保持关闭；缺少布局记录仅对已启用提供商表示默认开启。
- 最少一个实例只计算提供商实例。移除 macOS/Windows 的 Quota01 应用级图标、相关设置及强制兜底。Linux 现有托盘逻辑不变。
- 应用不能在没有原生入口或可见窗口时不可见地运行。

## 文件职责

| 职责                 | 文件                                                                                                                                                                                                                                                                            |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 共用数据详情与 popup | `src/lib/Dashboard.svelte`、新增 `src/lib/ProviderDataView.svelte`、`src/lib/ProviderRail.svelte`、`src/lib/providerTabSummary.ts`、`src/App.svelte`                                                                                                                            |
| 三列设置             | 新增 `src/lib/SettingsWorkspace.svelte`、`src/lib/CustomizeProviderList.svelte`、`src/lib/CustomizeProviderDetail.svelte`、`src/lib/SettingsScreen.svelte`                                                                                                                      |
| 窗口尺寸与关闭       | `src-tauri/src/window.rs`、`src-tauri/tauri.conf.json`、`src/lib/windowController.ts`、`src/lib/panelSizing.ts`、`src/lib/backend.ts`                                                                                                                                           |
| 原生实例             | `src-tauri/src/settings.rs`、`src-tauri/src/models.rs`、`src-tauri/src/menubar.rs`、`src-tauri/src/taskband.rs`、`src-tauri/src/tray_presentation.rs`、`src-tauri/src/desktop_integration.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/commands/settings.rs`、`src/lib/types.ts` |
| 刷新策略与文案       | `src-tauri/src/refresh_loop.rs`、`src-tauri/src/policy.rs`、`src-tauri/src/service.rs`、`src-tauri/src/notifications.rs`、`src-tauri/src/commands/usage.rs`、`src/lib/i18n/messages/*.ts`、`README.md`                                                                          |

## 重点风险

1. 已启用但没有原生实例的提供商仍必须出现在 popup 和设置预览中，由任务 1、4 测试。
2. 读数暂缺或实例创建失败时不能失去恢复入口，由任务 4 测试。
3. 旧用户缺少布局记录与明确保存 `enabled: false` 不能混淆，由任务 4 测试。
4. 15 分钟后台档不能在旧的 10 分钟阈值被误判为过期，由任务 5 测试。
5. 快速切换、保存和关闭不能选中已删除提供商或绕过退出确认，由任务 1、3、4 测试。

---

### 任务 1：共用数据视图与 popup 选择

**文件：**`src/lib/Dashboard.svelte`、新增 `src/lib/ProviderDataView.svelte`、`src/lib/ProviderRail.svelte`、`src/lib/providerTabSummary.ts`、`src/App.svelte`、`src/lib/ProviderRail.test.ts`、`src/App.update.test.ts`。

**接口：**`ProviderDataView` 接收单个 `providerId`、`UsageViewState`、`AppSettings`、`ProviderCatalogIndex`、操作回调和 `readOnlyPreview`。存在已启用提供商时，`ProviderRail` 接收非空的选中 ID。

- [x] 先写失败测试：三个已启用提供商（一个关闭原生实例）显示三行摘要；切换只改变右侧；`taskband-open` 选中对应提供商；移除当前选择后自动选中剩余第一个；零已启用提供商显示可操作的空状态。
- [x] 从 `Dashboard` 抽出提供商标题、指标、错误、重试和空状态。总览支出及更新提示保留在组件外。两个摘要读数继续使用 `providerTabReadings`，缺值时保持占位和无障碍名称。
- [x] 取消“All/单个筛选”模式，以提供商 ID 表示选择；左侧始终可见。保留顺序和方向键、Home/End。快捷键或直接打开应用时恢复有效选择，否则选第一个已启用提供商；点击原生实例优先选中该提供商。
- [x] 运行 `corepack pnpm test -- src/lib/ProviderRail.test.ts src/App.update.test.ts` 和 `corepack pnpm check`。检查错误状态及快速切换后左右两列保持一致。
- [x] 检查通过后提交此任务。

### 任务 2：三列设置工作区

**文件：**新增 `src/lib/SettingsWorkspace.svelte`、`src/lib/CustomizeProviderList.svelte`、`src/lib/CustomizeProviderDetail.svelte`、`src/lib/SettingsScreen.svelte`、`src/App.svelte`、新增 `src/lib/SettingsWorkspace.test.ts`。

**接口：**`SettingsWorkspace` 接收设置、使用状态、目录及现有保存、排序、刷新、账号和重置回调；只在本地保存当前选择。右侧以 `readOnlyPreview=true` 渲染 `ProviderDataView`。

- [x] 先写失败测试：三列显示、选择时不跳页、通用设置入口、已关闭提供商可选、移除后的选择后备、使用数据事件更新预览、键盘焦点顺序。
- [x] 将现有列表、提供商设置及数据视图组合到工作区。左侧增加“通用设置”。沿用现有保存回调，保留登录、命名、指标固定、排序、任务栏样式和重置。
- [x] 为各列设置标题、独立滚动及选中状态。窄工作区改成“列表 + 设置/预览切换面板”，并检查明暗主题及 RTL。
- [x] 运行定向测试、`corepack pnpm check` 和 `corepack pnpm lint`。查看启用、停用、无数据、加载和错误状态。
- [x] 检查通过后提交此任务。

### 任务 3：原生窗口宽度与界面切换

**文件：**`src-tauri/src/window.rs`、`src-tauri/tauri.conf.json`、`src/lib/windowController.ts`、`src/lib/panelSizing.ts`、`src/App.svelte`、`src/lib/panelSizing.test.ts`、Rust 窗口测试。

**接口：**主窗口只承载 popup 数据并固定为 440px；设置使用独立的普通原生窗口，目标约 1000px，受显示器工作区约束；过窄时启动两列后备布局。打开设置时收起 popup，关闭设置时按提供商实例状态恢复 popup 或保留退出确认窗口。

- [x] 先写几何失败测试：标准桌面、窄工作区、多显示器位置及缩放、从设置返回 popup、高度变化时反复切换。
- [x] 修改固定 `PANEL_WIDTH` 的相关尺寸及定位路径。设置窗口限制在当前工作区，返回时恢复 popup 的 440px 和锚点；高度适配不能套用旧界面尺寸。
- [x] 对照窗口代码与几何单测检查 popup/浮窗模式、手动高度、刷新中切换及系统缩放；运行定向 Rust 窗口测试和 `corepack pnpm test -- src/lib/panelSizing.test.ts`。
- [ ] 在原生 UI 手测标题栏拖动、设置独立窗口、popup 固定宽度，以及关闭 Settings 后 popup 恢复。Settings 不提供页面返回按钮。隔离 app 已启动，但 CUA 只能读到原生菜单，截图为空白面板，当前无法确认 WebView 结果。
- [ ] 原生 UI 核验后提交此任务。

### 任务 4：实例默认值、有效入口及退出确认

**文件：**`src-tauri/src/settings.rs`、`src-tauri/src/models.rs`、`src-tauri/src/menubar.rs`、`src-tauri/src/taskband.rs`、`src-tauri/src/tray_presentation.rs`、`src-tauri/src/desktop_integration.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/commands/settings.rs`、`src-tauri/src/window.rs`、`src/lib/CustomizeProviderDetail.svelte`、`src/lib/SettingsWorkspace.svelte`、`src/lib/SettingsScreen.svelte`、`src/lib/ConfirmationSheet.svelte`、`src/App.svelte`、`src/lib/types.ts` 及相关测试。

**接口：**纯 Rust 解析器 `requested_provider_entries(settings, registry, platform)` 根据固定指标配置列出应创建的实例，与快照数据解耦。原生对账结果向 `SettingsViewState` 报告实际创建成功的数量和失败原因。统一 `requestLeaveSettings` 处理 UI 与系统关闭。

- [x] 先写失败的 Rust 测试：开启后默认显示、明确关闭仍保持、未启用或无固定指标不计数、无快照时占位、创建失败、最后实例移除、零实例启动打开设置、提供商菜单仍能进入设置和退出、没有 Quota01 应用级图标。前端测试覆盖开关、确认/取消和所有离开路径。
- [x] 在 macOS/Windows 提供商设置中展示实例开关。移除 `showAppMenubar` UI 及 `show_app_menubar`/`app_menubar_forced` 契约，旧设置文件仍能读取。启用与显示实例分离，保留明确关闭；无固定指标时说明原因。
- [x] 移除 macOS 应用菜单栏实例、强制兜底及监听器，并移除 Windows 应用托盘图标及安装路径；Linux 托盘不变。按期望实例对账，读数缺失时显示 `--`。零实例启动直接打开设置，且原生提供商菜单的“设置”选中对应提供商。
- [x] 零实际实例时保持设置窗口可见。返回、Escape、标题栏关闭、系统关闭、点击窗口外和跳转 popup 统一弹出退出确认；取消保留窗口及焦点，确认等待保存队列完成后退出；明确“退出应用”直接执行。
- [x] 运行定向 Rust 与前端测试并检查代码路径；尝试用隔离 HOME 启动 macOS debug app bundle。CUA 能识别原生窗口，但窗口截图与 AX 树未显示 WebView 内容，未完成原生菜单栏与独立窗口视觉手测。Windows 原生 UI 也需在 Windows 设备验证。

### 任务 5：分档刷新、本地化与最终验证

**文件：**`src-tauri/src/refresh_loop.rs`、`src-tauri/src/policy.rs`、`src-tauri/src/service.rs`、`src-tauri/src/notifications.rs`、`src-tauri/src/commands/usage.rs`、`src/lib/i18n/messages/*.ts`、`README.md`、Rust 刷新测试及前端文案测试。

**接口：**`refresh_interval_for_provider` 对有实例或启用通知的提供商返回 5 分钟，对无实例且无通知的已启用提供商返回 15 分钟，对未启用提供商返回不调度。调度器按 ID 记录到期时间；`ProviderService` 使用同一间隔判断过期。

- [x] 以可控 `Instant` 输入覆盖快/慢/停用档、启用项首次立即刷新、启停和间隔变化、失败退避；策略测试覆盖后台提供商 5 分钟阈值及 10/30 分钟过期阈值。配置与实例状态每 5 秒重读，因此不用等到最长刷新周期才应用变化。
- [x] 将每 5 分钟整批刷新改为按提供商到期调度。相关间隔、尝试或失败状态变化时重算，复用 service 单飞及现有 `usage-state` 事件。选中后台提供商超过 5 分钟后通过新命令补刷；手动刷新仍强制执行。
- [x] 过期阈值与通知所用 service 快/慢策略统一。沿用前几项新增的多语言列标题、实例说明、更新时间、空状态和退出确认文案；清理旧应用图标文案并更新 README。
- [x] 运行前端、Rust、契约验证，以及 macOS 和 Windows GNU Rust 目标编译。隔离 HOME 下的 macOS debug app bundle 已启动；CUA 只读到原生菜单，窗口截图呈空白面板，无法确认 WebView 的最终视觉结果。Windows 原生 UI 无可用设备。普通/紧凑、主题、RTL、键盘和减少动画的原生端到端手测仍需相应系统设备完成。
- [x] 全套自动验证通过。
- [ ] 完成任务 3 的原生窗口手测后，提交最终补丁。

## 实施交接

初始实现已完成。用户确认 popup 与设置必须使用独立原生窗口；本轮继续审核并补齐窗口隔离和 popup 固定宽度。

## 实施记录

- Task 4 将 `SettingsViewState` 扩展为返回实际成功实例数和逐实例失败原因，供设置工作区显示；该字段是计划中“向 SettingsViewState 报告实际创建成功的数量和失败原因”的具体实现。
- Task 4 修复 `src-tauri/src/providers/workbuddy/auth.rs` 中 Windows 专用 `auth_file_path()` 末尾多余分号。Windows GNU 目标检查显示该分号令路径表达式返回 `()`，阻止 Windows 编译；这是验证发现的必要编译修复，不是格式化改动。
- 原生 UI 手测限制：隔离 HOME 下可启动并由 CUA 绑定 macOS debug app bundle，但 WebView AX 内容不可见、截图为空白面板，因此本轮不能确认最终窗口外观；Windows 原生 UI 也没有可用主机。自动化前端、Rust 和 macOS/Windows 目标编译均单独验证通过，系统 UI 手测仍需在相应设备完成。
- Task 5 将刷新时间状态从单个固定 5 分钟计时改为每个启用提供商独立调度；快档用于实际提供商原生实例或启用通知的提供商，慢档用于无实例且无通知者。macOS/Windows 实例 ID 来自原生创建/更新成功结果，Linux ID 来自既有托盘实际解析出的提供商组。自动分批不更新 `lastFullRefreshAt`，真实界面使用 `nextRefreshAt`；旧字段只保留前端兼容回退。
- Task 5 按可注入的单调时间参数测试调度到期，不另建可变假时钟；失败后 60 秒重试，实例/通知/设置变化由 5 秒轮询收敛，新启用提供商立即刷新。选中后台提供商的检查由 `refresh_selected_provider_if_due` 明确触发，沿用手动按钮强制刷新的独立路径。
- 窗口隔离审核确认：主 popup 原先会在同一个 WebView 内切入 `SettingsWorkspace`，随后把窗口宽度扩到约 1000px；修复以单独的 `settings` Tauri 窗口承载设置，并将 popup 最大宽度锁定为 440px。
- 窗口路由再次审核：主窗口固定渲染 popup 数据页，`settings` 窗口固定渲染三列工作区；提供商设置请求通过 `settings-workspace-selection` 在工作区内选中，不再更换页面；移除了 Settings 页面的返回按钮。前端回归覆盖 popup 仍只呈现数据，以及 Settings 选中提供商时仍留在单页。
- 最终复核重新运行 `corepack pnpm verify:frontend`（47 个文件、367 项测试通过；格式、Svelte 检查和构建通过，构建提示现有 JS chunk 超过 500 kB）、`corepack pnpm verify:rust`（格式、Clippy 与 767 项测试通过）、`corepack pnpm verify:contracts`、`corepack pnpm verify:versions`、macOS 与 Windows GNU all-targets `cargo check`，均通过；`git diff --check` 通过。隔离 HOME 原生启动探测到已安装的 Trae CN 提供商但无登录态，随后停止测试进程；未能通过空白 CUA 面板完成原生 WebView 视觉确认。
