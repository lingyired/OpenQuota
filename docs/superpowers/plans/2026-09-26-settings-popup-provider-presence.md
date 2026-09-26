# 设置、Popup 与提供商实例实施计划

> **实施时：**使用 `executing-plans` skill，逐项执行并勾选 `- [ ]`。遵循用户的最新 Luna 模型偏好，目前为 `gpt-6-luna`。

**目标：**实现三列设置、两列 popup、独立提供商实例控制，以及分档自动刷新。

**架构：**`App.svelte` 管理选择和导航；抽取单提供商数据组件供 popup 与设置预览共用。Rust 负责创建并统计实际原生实例；自动刷新按提供商调度，手动刷新保留现有命令。

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

**接口：**增加按 `dashboard` 或 `settings` 调整窗口尺寸的原生操作，返回实际逻辑尺寸。Popup 保持 440px；设置目标约 1000px，受显示器工作区约束；过窄时启动两列后备布局。

- [ ] 先写几何失败测试：标准桌面、窄工作区、多显示器位置及缩放、从设置返回 popup、高度变化时反复切换。
- [ ] 修改固定 `PANEL_WIDTH` 的相关尺寸及定位路径。设置窗口限制在当前工作区，返回时恢复 popup 的 440px 和锚点；高度适配不能套用旧界面尺寸。
- [ ] 检查 popup/浮窗模式、标题栏拖动、手动高度、刷新中切换及系统缩放。运行定向 Rust 窗口测试和 `corepack pnpm test -- src/lib/panelSizing.test.ts`。
- [ ] 检查通过后提交此任务。

### 任务 4：实例默认值、有效入口及退出确认

**文件：**`src-tauri/src/settings.rs`、`src-tauri/src/models.rs`、`src-tauri/src/menubar.rs`、`src-tauri/src/taskband.rs`、`src-tauri/src/tray_presentation.rs`、`src-tauri/src/desktop_integration.rs`、`src-tauri/src/lib.rs`、`src-tauri/src/commands/settings.rs`、`src-tauri/src/window.rs`、`src/lib/CustomizeProviderDetail.svelte`、`src/lib/SettingsWorkspace.svelte`、`src/lib/SettingsScreen.svelte`、`src/lib/ConfirmationSheet.svelte`、`src/App.svelte`、`src/lib/types.ts` 及相关测试。

**接口：**纯 Rust 解析器 `requested_provider_entries(settings, registry, platform)` 根据固定指标配置列出应创建的实例，与快照数据解耦。原生对账结果向 `SettingsViewState` 报告实际创建成功的数量和失败原因。统一 `requestLeaveSettings` 处理 UI 与系统关闭。

- [ ] 先写失败的 Rust 测试：开启后默认显示、明确关闭仍保持、未启用或无固定指标不计数、无快照时占位、创建失败、最后实例移除、零实例启动打开设置、提供商菜单仍能进入设置和退出、没有 Quota01 应用级图标。前端测试覆盖开关、确认/取消和所有离开路径。
- [ ] 在 macOS/Windows 提供商设置中展示实例开关。移除 `showAppMenubar` UI 及 `show_app_menubar`/`app_menubar_forced` 契约，旧设置文件仍能读取。启用与显示实例分离，保留明确关闭；无固定指标时说明原因。
- [ ] 移除 macOS 应用菜单栏实例、强制兜底及监听器，并移除 Windows 应用托盘图标及安装路径；Linux 托盘不变。按期望实例对账，读数缺失时显示 `--`。零实例启动直接打开设置，且原生提供商菜单的“设置”选中对应提供商。
- [ ] 零实际实例时保持设置窗口可见。返回、Escape、标题栏关闭、系统关闭、点击窗口外和跳转 popup 统一弹出退出确认；取消保留窗口及焦点，确认等待保存队列完成后退出；明确“退出应用”直接执行。
- [ ] 运行定向 Rust 与前端测试。手动移除最后一个 macOS 实例、关闭最后一个 Windows 实例，检查应用图标不出现、设置仍可见、取消不会退出；通过后提交。

### 任务 5：分档刷新、本地化与最终验证

**文件：**`src-tauri/src/refresh_loop.rs`、`src-tauri/src/policy.rs`、`src-tauri/src/service.rs`、`src-tauri/src/notifications.rs`、`src-tauri/src/commands/usage.rs`、`src/lib/i18n/messages/*.ts`、`README.md`、Rust 刷新测试及前端文案测试。

**接口：**`refresh_interval_for_provider` 对有实例或启用通知的提供商返回 5 分钟，对无实例且无通知的已启用提供商返回 15 分钟，对未启用提供商返回不调度。调度器按 ID 记录到期时间；`ProviderService` 使用同一间隔判断过期。

- [ ] 先写模拟时钟失败测试：快/慢/停用档、启停及实例变化、通知和凭据变化、失败退避、后台提供商选中时上次尝试小于/大于 5 分钟、10/30 分钟过期阈值。
- [ ] 将每 5 分钟整批刷新改为按提供商到期调度。相关状态变化时重算，保留单飞和现有 `usage-state` 事件。选中后台提供商超过 5 分钟才补刷，手动刷新始终强制执行。
- [ ] 统一过期及通知评估策略。为全部现有语言补充列标题、实例说明、更新时间、空状态和退出确认文案，清理过时的应用图标文案。更新 README。
- [ ] 运行 `corepack pnpm verify:frontend`、`corepack pnpm verify:rust`、`corepack pnpm verify:contracts`。实测 macOS 菜单栏和 Windows 任务栏及 popup，检查普通/紧凑密度、明暗主题、RTL、纯键盘和减少动画。
- [ ] 全套验证通过后提交此任务。

## 实施交接

实施前阅读本计划与设计说明。用户已确认只计算提供商实例，也不再需要 Quota01 应用图标。当前请求只要求计划，此阶段不修改产品代码。
