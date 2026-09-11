# Workbuddy CN 积分包后续事项

## 当前状态

Workbuddy CN 的积分包查询、汇总、Popup 展示、分享卡片、拖拽排序以及近期到期积分展示已经完成，并通过当前自动化测试和静态检查。

当前 worktree：`/Users/lingsmbp/Documents/aiwork/OpenQuota-workbuddy`

当前分支：`codex/workbuddy-provider`

## 已完成的功能

### 1. 分享卡片支持“可用积分包”

`src/lib/shareCard.ts` 的 `buildProviderShareRows` 已支持 `creditPackages` 指标来源:

- 只导出 `remaining > 0` 的有效积分包。
- 收缩状态最多导出最近 3 个积分包,展开时导出全部。
- 每个积分包使用现有分享卡片的横向进度行,显示包名、剩余/总量、进度条和各自到期时间。
- 无有效积分包时不生成空行。

已补充收缩、展开和无有效积分包三个场景的测试。

### 2. Popup 与 menubar 的积分口径统一

问题：Popup 里显示 `1K`，但 menubar 显示 `2.2K`。

根因：同一份积分被拆成了两个口径相反的指标。

- `workbuddy.credits` 是“已用 / 总量”的额度指标（已用 `2295.58`、总量 `3315`）。
- `workbuddy.balance` 是“剩余积分”的值指标（剩余 `1019.42`）。
- menubar 默认固定前者，所以按 `usageDisplay = used` 显示已用 `2296`；Popup 里最醒目的却是后者的 `1K`。

修复：统一为剩余积分口径。

- `workbuddy.credits` 改为引用 `balance` 值指标，表示可用（剩余）积分，并保持默认固定。
- 移除重复的 `workbuddy.balance` 指标。
- 旧设置里的 `workbuddy.balance` 条目会在启动归一化时被安全丢弃，已固定的 `workbuddy.credits` 自动改为显示剩余积分。
- 数值不再附带 `credits` 单位词：Popup 标题已经写明 Credits，menubar/taskband 只显示 `1.0K` 这样的纯数字。
- 快照不再产出 `credits` quota。快照契约是双向的：快照里出现的每个 quota/value/status 都必须被 provider 指标定义引用，否则刷新会以 `Provider data does not match its registered metric contract.` 失败。已用/总量改由「可用积分包」逐包展示。

规则：WorkBuddy 的 “Credits” 始终表示可用（剩余）积分，等于所有有效积分包剩余积分之和；已用/总量的进度只体现在每个积分包自身的进度条里。

### 3. Popup 中积分包的展开规则

- 选择单个提供商 Tab，或通过任务栏/总 App 图标 focus 某个提供商时，始终展示全部有效积分包，不受该提供商的展开/收缩状态影响。
- 停留在“全部”Tab，或通过总 App 图标打开总览时，提供商的展开/收缩状态继续生效：收缩最多显示最近 3 个，展开后显示全部。

## 待发布前确认

### 4. 手动验收 Workbuddy CN 页面

使用本地 dev 启动应用后，确认以下行为：

- Workbuddy 显示名称为 `Workbuddy CN`。
- Popup 中可见“可用积分包”指标。
- 选择 Workbuddy CN Tab 后，无论原展开状态如何，都显示全部有效积分包。
- 在“全部”Tab 中，收缩状态最多显示最近 3 个有效积分包，展开后显示全部。
- 每个积分包的剩余/总量、到期时间和进度条对应实际数据。
- 拖拽“可用积分包”后，刷新或重新打开 Popup 仍保留排序。
- Star “近期到期的积分包”后，menubar 和 taskband 显示类似 `71` 的纯数字，不附加英文单位。
- 所有有效积分包余额累加后的总积分与服务端数据一致。

### 5. 发布打包

当前没有进行新的 macOS 打包。后续打包时需要使用不会与其他版本混淆的特殊版本号，并遵循以下约定：

- 打包完成后不要自动打开 DMG。
- 只向用户提供生成文件的位置或下载入口。
- 打包前先完成 dev 手动验收，尤其检查真实积分包数量、余额和到期时间。

## 已完成但需保留的规则

- 总积分是所有有效积分包剩余积分之和。
- “近期到期的积分包”只取最近到期的一个积分包，不是未来固定天数内的全部积分包。
- 有效积分包不包含已过期、余额为 0 或历史包。
- 积分包的到期明细使用各自的到期时间，不能用总积分的最近到期时间替代所有积分包的到期时间。

## 当前验证记录

- `pnpm verify`：通过（versions、contracts、frontend、rust 全链路）
- `cargo test --all-targets`：593 passed，0 failed
- `pnpm test`：245 passed，0 failed
- 已补充 `mapped_metrics_are_all_exposed_by_the_definition` 回归测试，覆盖上面的快照契约
