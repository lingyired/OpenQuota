# Windows 任务栏 Taskband 集成计划

## 一、摘要

目标：优化 Windows 版体验，引入 [`tauri-plugin-multiline-taskband`](https://github.com/lingyired/tauri-plugin-multiline-taskband)（Windows 任务栏左右边缘两行文本标签插件，TAURI v2，Win10/11 均支持）。

- 每个已启用的监控（provider）在任务栏上生成一个 taskband 实例，两行显示：
  - 第一行：图标（文字缩写，如 "OC"）+ 槽位 1 的值（默认 session）
  - 第二行：槽位 2 + 槽位 3 的值（默认 weekly、monthly，用 `·` 连接；监控不支持则自动省略）
- 除左上角图标固定外，其余三个内容槽位可自由选择该监控支持的指标（如 codex 没有 monthly 就不显示）。
- 点击 taskband 实例 → 复用现有主窗口 popup，自动滚动到该监控；系统托盘行为保持不变。
- 增加设置项：全局（主开关、默认位置、间距）+ 每监控（位置 left/right、颜色、加粗、字号、对齐、padding、内容槽位）。

核心架构决策：**所有 taskband 创建/更新/删除/点击处理都在 Rust 侧完成**（新模块 `taskband.rs`），前端只负责编辑设置（AppSettings）和监听 `taskband-open` 事件滚动。原因：
1. 数据（快照/指标值）在 Rust 侧，刷新后 Rust 直接更新 taskband，不依赖 webview 是否活跃（webview 休眠时任务栏数据仍实时）。
2. 插件仓库未提交 `dist-js` 构建产物，前端 npm 无法直接以 git 依赖引入 guest-js；Rust 侧调用插件原生 API（`app.multiline_taskband()`）无此问题。
3. 前端无需新增插件权限，仅需既有 `core:default`（事件监听）。

## 二、现状分析

- **插件能力**（已克隆源码核实）：
  - Rust API：`MultilineTaskbandExt::multiline_taskband()`，方法有 `create(id, side)`、`set_text`、`set_colors`、`set_font_sizes`、`set_font_family`、`set_bold`、`set_alignment`、`set_padding`、`set_line_visible`、`set_visible`、`set_side`、`set_order`、`set_margin`、`set_edge_margins`、`set_auto_popup`、`remove` 等。
  - 事件：`multiline-taskband://{id}//ready`、`//click`（payload `{id, button, buttonState, position, rect}`）、`//popup-open`、`//popup-close`。
  - 每实例两行文本，每行独立颜色（`default` 跟随系统 / `solid #rrggbb`）、字号（pt）、加粗、对齐（0/1/2）；每实例独立 padding、位置（left=开始按钮旁 / right=托盘旁）、顺序。
  - 内置 popup 机制（`setPopupWindow` + `setAutoPopup`），但我们不复用（会尝试打开未注册的 popup 窗口），将调用 `set_auto_popup(false)` 关闭。
  - 插件仅支持文本渲染、不支持图片 → 图标用文字缩写（用户已确认"先显示文字缩写"）。
- **OpenQuota 现状**：
  - [`tray_presentation.rs`](file:///Users/lingsmbp/Documents/aiwork/OpenQuota/src-tauri/src/tray_presentation.rs) 的 `update()` 在每次刷新完成（`notifications::finish_refresh`）、设置保存（`commands/settings.rs`）、启动时被调用 —— 是 taskband 更新的理想挂载点。内部 `tray_metric()` 已能把指标解析为短值（如 "75%"）。
  - [`models.rs`](file:///Users/lingsmbp/Documents/aiwork/OpenQuota/src-tauri/src/models.rs)：`AppSettings`（schema_version=7）、`ProviderLayout`、`MetricDefinition`（含 `tray.short_label`，如 "S"/"W"/"M"）。
  - 指标示例：opencode 有 `session/weekly/monthly` 三个 quota（全 enable、AlwaysVisible）；codex 有 `session/weekly/spark/sparkWeekly` + `credits/rateLimitResets` 等，无 monthly —— 正好验证"不支持就不显示"。
  - [`window.rs`](file:///Users/lingsmbp/Documents/aiwork/OpenQuota/src-tauri/src/window.rs)：`show_main_window()`、`open_screen()` 已存在；App.svelte 已有 `open-screen` 事件监听模式可复用。
  - 主窗口为 `main`（320px 宽 popup），`[data-provider-id]` 标记每个监控区块，可作滚动锚点。
  - CI（`.github/workflows/ci.yml`）含 `windows-latest` 矩阵，可验证 Windows 构建。

## 三、变更方案

### 1. 引入插件依赖（仅 Windows）

**文件：`src-tauri/Cargo.toml`**
- 在 `[target.'cfg(target_os = "windows")'.dependencies]` 下新增：
  ```toml
  tauri-plugin-multiline-taskband = { git = "https://github.com/lingyired/tauri-plugin-multiline-taskband", branch = "main" }
  ```
- 说明：非 Windows 目标不编译插件，macOS/Linux 构建零影响。CI 需联网拉取 git 依赖（CI 本来就访问 GitHub，无碍）。后续可考虑锁定 commit 提高可复现性。

**文件：`src-tauri/src/lib.rs`**
- 新增 `#[cfg(target_os = "windows")] mod taskband;`
- Builder 链中加 `.plugin(tauri_plugin_multiline_taskband::init())`（`#[cfg(target_os = "windows")]` 包裹）。

### 2. 设置模型扩展

**文件：`src-tauri/src/models.rs`**

新增类型（serde camelCase/lowercase，与现有风格一致）：

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskbandSide { Left, Right }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TaskbandColorStyle { Default, Solid { value: String } }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskbandLayout {
    pub enabled: bool,                 // 该监控是否显示在任务栏（默认 true）
    pub side: Option<TaskbandSide>,    // None = 跟随全局默认位置
    pub slot_top: Option<String>,      // 第一行内容指标 id（None = 只显示图标）
    pub slot_bottom: Option<String>,   // 第二行左
    pub slot_bottom_2: Option<String>, // 第二行右
    pub show_labels: bool,             // 第二行是否显示短标签（默认 true）
    pub top_color: Option<TaskbandColorStyle>,   // None = 自动（有品牌色用品牌色，否则系统色）
    pub bottom_color: Option<TaskbandColorStyle>,// None = 系统色
    pub top_bold: bool,
    pub bottom_bold: bool,
    pub top_size: f64,                 // pt，默认 9
    pub bottom_size: f64,
    pub top_align: i32,                // 0/1/2，默认 0
    pub bottom_align: i32,
    pub padding_left: i32,             // 物理像素，默认 4
    pub padding_right: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskbandPreferences {
    pub enabled: bool,              // Windows 主开关（默认 true）
    pub default_side: TaskbandSide, // 默认 Right
    pub margin: i32,                // 实例间距，默认 4
    pub edge_margin_left: i32,      // 默认 0
    pub edge_margin_right: i32,     // 默认 0
}
```

`AppSettings` 增加（均 `#[serde(default)]`，向后兼容旧存档）：
- `pub taskband: TaskbandPreferences`
- `pub taskband_providers: BTreeMap<String, TaskbandLayout>`
- `schema_version` 默认 7 → 8；`impl Default` 补齐默认值。

**文件：`src-tauri/src/settings.rs`**
- `normalize_with_persisted_accounts` 中 `settings.schema_version = 7;` → `8;`。
- 更新测试断言（约 L2026/L2034 处 `schema_version == 7` → `8`）。
- `default_settings` 无需改（走 `..AppSettings::default()`），`taskband_providers` 空 map 即全部走默认槽位逻辑。

**文件：`src/lib/types.ts`**
- 镜像新增：`TaskbandSide`、`TaskbandColorStyle`、`TaskbandLayout`、`TaskbandPreferences`，并在 `AppSettings` 增加 `taskband`、`taskbandProviders`。

### 3. 新增 Rust 模块 `src-tauri/src/taskband.rs`（`#[cfg(target_os = "windows")]`）

**职责**：根据 AppSettings + UsageViewState 对账（reconcile）taskband 实例。

- **托管状态** `TaskbandState`（`app.manage`）：
  - `created: Mutex<HashMap<String, AppliedConfig>>`（已创建实例及其上次应用的配置摘要，用于 diff，避免每次刷新重复调用 set_*）
  - `click_registered: Mutex<HashSet<String>>`（已注册 click 监听的实例 id）
  - `plugin_configured: AtomicBool`（全局配置是否已应用）
- **入口** `pub(crate) fn update(app, state, settings, registry)`：
  1. 主开关 `settings.taskband.enabled == false` → 移除全部实例、清空记录，返回。
  2. 首次（或 margin/edge_margins 变化时）：`set_auto_popup(false)`、`set_margin`、`set_edge_margins`。
  3. 遍历 `settings.providers`（enabled 且 taskband 启用）：
     - 解析该 provider 的槽位值与文本（见下）。
     - 无快照 → `set_visible(false)`（保留实例避免闪烁）。
     - 有快照 → 未创建则 `create(id, side)`；随后按 diff 调用 `set_text`、`set_side`、`set_colors`、`set_bold`、`set_font_sizes`、`set_alignment`、`set_padding`、`set_line_visible`、`set_visible(true)`。
  4. 清理：已禁用/移除 provider 的实例 → `remove(id)`。
  5. 为每个实例注册一次 click 监听（事件名 `multiline-taskband://{id}//click`）：
     - `button == "left" && buttonState == "up"` → `window::show_main_window(&main_window)` + `app.emit("taskband-open", provider_id)`。

- **文本组装规则**（实例 id = provider layout id，如 "opencode"）：
  - 槽位指标候选 = 该 provider 的 `metrics` 中 `enabled && tray.is_some()`，按 layout 顺序。
  - 未在 `taskband_providers` 显式配置时（或槽位为 None）的默认：
    - `slot_top` = 第 1 个候选（opencode=session）
    - `slot_bottom` = 第 2 个候选（opencode=weekly）
    - `slot_bottom_2` = 第 3 个候选（opencode=monthly；codex 只有 session/weekly/spark，无 monthly 就不显示）
  - 第一行文本 = `{short_name} {槽位1值}`（如 "OC 75%"；槽位为空则仅 "OC"）。
  - 第二行文本 = `{短标签} {值}` 组合，`show_labels` 为 true 时带 `tray.short_label` 前缀，多个用 ` · ` 连接（如 "W 80% · M 40%"）；两个槽位都为空则 `set_line_visible(bottom=false)`。
  - 指标值解析：复用 `tray_presentation` 的解析逻辑（需把 `tray_metric` 提升为 `pub(crate)`，并新增 `pub(crate) fn resolved_provider_metrics(state, provider, settings, registry) -> Vec<(metric_id, TrayMetric)>` 返回该 provider 全部启用的 tray 指标值，供 taskband 按 id 取用）。该函数同时供 taskband 与现有 `resolved_groups` 使用，避免重复实现。
- **图标颜色**：`top_color` 为 None 时，若 `providerIconColor` 有品牌色（如 antigravity #4285F4、kimi #1783FF）则用 `Solid(品牌色)`，否则 `Default`（跟随系统任务栏文字色）。

**挂载点**：在 `tray_presentation::update()` 末尾追加（`#[cfg(target_os = "windows")]` 下调用 `taskband::update(app, state, settings, registry)`）。这样启动、每次刷新、每次设置保存都自动对账。

### 4. 点击 → popup 打开并滚动到该监控

- 插件侧：`set_auto_popup(false)`（见模块 3），不使用插件的 popup 窗口机制。
- Rust 侧：taskband.rs 的 click 监听处理（见模块 3）：`show_main_window` + `emit("taskband-open", provider_id)`。
- 前端侧：

**文件：`src/App.svelte`**
- 新增监听：`onTaskbandOpen`（复用现有 `onOpenScreen` 监听模式，监听 `taskband-open` 事件）。
- 处理：`screen = 'dashboard'` → `await tick()` → 在 DOM 中查找 `.provider-section[data-provider-id="{providerId}"]` 并 `scrollIntoView({ block: 'start' })`。
- 若需先确保窗口可见再滚动，可在收到事件后先调用现有显示逻辑（Rust 侧已 `show_main_window`，webview 激活后事件可达）。

**文件：`src/lib/Dashboard.svelte`**（如需要）
- 若 App.svelte 直接操作 DOM 不便，可给 Dashboard 增加 `scrollToProvider(providerId)` 方法（props 回调）在渲染树内定位滚动。实现时二选一，倾向 App.svelte 直接 DOM 查询（最小改动）。

### 5. 前端设置 UI

**文件：`src/lib/SettingsScreen.svelte`**
- 新增 "Taskbar" 设置区块（用 `desktopPlatform()` 判断仅 Windows 显示），包含：
  - 主开关：启用任务栏监控显示（`taskband.enabled`）
  - 默认位置：左 / 右（`taskband.defaultSide`）
  - 实例间距（`taskband.margin`）、左侧边距、右侧边距（数字输入）
- 走既有 `onChange` 设置保存流。

**文件：`src/lib/CustomizeProviderDetail.svelte`**
- 每个监控新增 "Taskbar" 折叠区块（仅 Windows 显示）：
  - 启用开关（`taskband_providers[id].enabled`）
  - 位置：跟随默认 / 左 / 右
  - 三个内容槽位下拉框：选项 = 该监控 enabled 且 `tray != null` 的指标 + "None"（opencode 显示 Session/Weekly/Monthly；codex 无 monthly 自然没有该选项）
  - 第二行显示短标签开关（`showLabels`）
  - 样式：第一行/第二行颜色（默认/自定义 hex）、加粗、字号、对齐、padding
  - "恢复默认"按钮（从 `taskband_providers` 删除该 key）
- 交互细节：首次修改某监控设置时物化完整 `TaskbandLayout` 写入 map；槽位默认跟随候选顺序。

### 6. 版本与文档

- `src-tauri/tauri.conf.json` 与 `src-tauri/Cargo.toml`：`0.5.0` → `0.6.0`（用户可见新功能，符合项目版本规则）。
- `docs/releasing.md` 如含变更清单需补充；若后续有功能文档则同步。

### 7. 测试更新

- `src-tauri/src/tray_presentation.rs` 的既有测试因 `tray_metric` 提升可见性无需改动；新增 taskband 文本组装单测（opencode 三槽位、codex 缺 monthly、空快照隐藏等）——放 `taskband.rs` 内 `#[cfg(test)]`。
- 前端：`settingsController.test.ts` 等若断言完整 `AppSettings` 默认值需同步新增字段；`App.test.ts` 若受影响同步更新。

## 四、假设与决策

1. **Rust 侧驱动**：所有 taskband 调用在 Rust 完成，前端仅编辑设置与监听事件；不引入插件 guest-js npm 依赖、不新增 capability 权限（事件监听属 `core:default`）。
2. **图标用文字缩写**：插件不支持图片，用 `ProviderDefinition.short_name`（如 "OC"）+ 品牌色/系统色。
3. **两行四元素布局**：受插件"每实例仅两行文本"限制，实际渲染为 第一行 `图标 + 槽位1值`，第二行 `槽位2值 · 槽位3值`（可带短标签）；三个内容槽位自由选择，缺失则跳过/隐藏行。不追求逐像素四角布局。
4. **默认开启**：主开关 `taskband.enabled` 默认 true、每监控 `enabled` 默认 true（Windows 用户更新后启用监控即出现 taskband；符合"开启监控即显示"诉求）。
5. **默认槽位**：自动取该监控 enabled 的 tray 指标前三个（session/weekly/monthly 顺序）；codex 无 monthly 则不显示第三槽。
6. **点击行为**：左键 → 复用现有 main popup 窗口并滚动到该监控；托盘（系统 tray）行为不变。插件自动 popup 关闭。
7. **非 Windows**：插件依赖、taskband 模块、设置 UI 区块全部 `cfg(target_os = "windows")` 或按平台隐藏，macOS/Linux 零影响。
8. **schema_version 7→8**：新增字段均 `#[serde(default)]`，旧存档无损加载。

## 五、验证步骤

1. macOS 本地：`cargo check`（taskband 模块被 cfg 排除，确认无影响）+ `pnpm test`（前端测试通过）。
2. Windows 构建：CI `windows-latest` 任务 `cargo check/test` 通过；本地有 Windows 机器则 `pnpm tauri dev` 验证。
3. Windows 手动验证清单：
   - 启用 opencode → 任务栏右缘出现 "OC 75%" / "W 80% · M 40%" 两行标签。
   - 启用 codex → 第二行无 monthly，仅显示存在的槽位。
   - 设置中修改槽位/颜色/字号/加粗/对齐/padding → taskband 实时更新。
   - 修改某监控位置为 left → 实例移到开始按钮旁。
   - 点击 taskband 实例 → 主窗口 popup 打开并滚动到该监控；托盘点击仍打开完整仪表盘。
   - 禁用监控/关闭主开关 → 对应实例移除。
   - 数据刷新后文本自动更新；explorer 重启、任务栏移动后实例仍正常。
   - 深浅色切换时 `default` 颜色跟随系统。
4. 发布流程：按 `docs/releasing.md` 走 `pnpm build` 出包（版本 0.6.0）。
