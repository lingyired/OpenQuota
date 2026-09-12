# Usage01 Provider 默认固定指标设计

## 背景

Usage01 的 provider 指标可以独立固定在托盘或 macOS 菜单栏。当前每个 provider 通过 `MetricDefinition::default_pinned` 自行声明默认固定项，规则并不一致：

- Claude、Codex、Antigravity、Kimi、MiniMax 和 Z.ai 默认固定前两个指标。
- Cursor 默认固定第二、第三个指标。
- Copilot 和 OpenRouter 只默认固定第一个指标。
- Devin、Grok 和 OpenCode 默认不固定任何指标。
- 动态发现的额外 Claude 账号 provider 被统一设置为不固定，即使主 Claude provider 默认固定前两个。

这会导致首次安装时，即使某个 provider 被检测到并自动启用，它的菜单栏也可能没有任何默认内容。新的产品规则是：每个 provider 都默认固定它的前两个可选指标，使 provider 自动启用后即可直接在菜单栏看到最关键的数据。

## 目标

- 所有 provider 在全新安装时默认固定前两个可固定且默认启用的指标。
- 规则覆盖静态 provider、未来新增 provider，以及动态发现的 `claude@xxxx` provider。
- 已有用户保存的 pinned 状态保持不变，不做一次性迁移。
- 保持每个 provider 最多固定两个指标的限制。

## 非目标

- 不改变用户手动选择固定指标的能力。
- 不把已有用户升级到新默认值。
- 不改变菜单栏指标顺序、展示格式、刷新行为或 provider 自动检测逻辑。
- 不新增第三种固定状态或修改设置 schema。

## 规范化规则

在 `ProviderRegistry::new` 接收 provider 定义并准备写入 catalog 时统一规范化：

1. 将定义中所有指标的 `default_pinned` 先设为 `false`。
2. 按 provider 指标数组顺序遍历。
3. 对满足以下条件的指标计为可选固定项：
   - `pinnable == true`
   - `default_enabled == true`
   - `tray.is_some()`
4. 将遇到的前两个可选固定项设为 `default_pinned = true`。
5. 如果可选固定项不足两个，只固定实际存在的项。

该规则成为 provider catalog 的统一不变量。provider 定义中继续存在的 `default_pinned` 字段由 registry 覆盖，因此不需要逐个 provider 维护默认策略。

## 数据流

1. provider runtime 返回原始 `ProviderDefinition`。
2. `ProviderRegistry::new` 过滤 links，并规范化 `default_pinned`。
3. 规范化后的定义通过 provider catalog 暴露给前端，并作为设置默认值的唯一来源。
4. 全新安装时，`default_provider` 根据规范化后的定义创建 `MetricLayout`。
5. 已有设置归一化时，已有指标继续保留用户保存的 `pinned` 值。
6. 缺失的新指标和完全新增的 provider 使用规范化后的默认值。
7. “重置 provider”和“重置所有设置”使用同一套规范化默认值。

## 边界行为

- 动态 Claude 额外账号不再被特殊清除固定项；它们和主 Claude provider 一样固定前两个可选指标。
- Cursor 从“第二、第三个指标”改为严格固定“第一、第二个指标”。
- OpenCode、Devin、Grok 首次安装时固定前两个指标。
- Copilot、OpenRouter 默认增加第二个固定指标。
- 已经保存的 provider 布局不执行批量改写，因此规则切换本身不会新增或取消已有用户的固定项。
- 旧安装中已经存在但缺失某个指标的情况，新增指标仍会按新的 provider 默认值补齐。

## 测试

- 增加 provider catalog 不变量测试：每个 provider 只有按顺序遇到的前两个合格指标为 `default_pinned = true`。
- 增加动态 Claude 账号 provider 测试，确认 `claude@xxxx` 的前两个指标默认固定。
- 更新依赖旧规则的 provider 测试：
  - OpenCode 不再断言全部不固定。
  - Cursor 改为断言前两个固定、第三个不固定。
  - Claude 额外账号断言前两个固定，其余不固定。
  - Copilot、OpenRouter、Devin、Grok 的默认固定预期与统一规则一致。
- 增加全新安装测试，确认启用 provider 时前两个指标为 pinned。
- 增加已有配置回归测试，确认载入和保存不会改写用户原有 pinned 状态。
- 运行 Rust 测试、格式检查和 clippy；如前端类型或 fixture 受影响，同时运行对应前端检查。

## 成功标准

全新安装中，任何 provider 被检测并启用后，只要前两个可选位置中至少有一个合格指标，菜单栏就会默认显示这些指标，最多两个。已有用户的固定选择保持不变，registry、settings、reset 和动态 Claude 账号路径对默认固定规则保持一致。
