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

## 待发布前确认

### 2. 手动验收 Workbuddy CN 页面

使用本地 dev 启动应用后，确认以下行为：

- Workbuddy 显示名称为 `Workbuddy CN`。
- Popup 中可见“可用积分包”指标。
- 收缩状态最多显示最近 3 个有效积分包。
- 展开后显示全部有效积分包。
- 每个积分包的剩余/总量、到期时间和进度条对应实际数据。
- 拖拽“可用积分包”后，刷新或重新打开 Popup 仍保留排序。
- Star “近期到期的积分包”后，menubar 和 taskband 显示类似 `71` 的纯数字，不附加英文单位。
- 所有有效积分包余额累加后的总积分与服务端数据一致。

### 3. 发布打包

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

- `cargo test`：589 passed，0 failed
- `pnpm test`：245 passed，0 failed
- `pnpm build`：通过（存在既有的大 chunk 提示）
- `pnpm check`：0 errors，0 warnings
- `pnpm lint`：通过
- `pnpm verify:contracts`：通过
- `cargo fmt`：通过
