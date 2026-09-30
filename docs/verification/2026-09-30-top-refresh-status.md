# Verification report — 顶部「最近更新」状态行

**Date:** 2026-09-30
**Host:** macOS (arm64), `cargo 1.96.0`, Node `v24.16.0`
**Revision under test:** working tree, diff hash `54646243a05b16a2cda702740a32181eedd0c8e6`
（23 个文件：`models.rs`、`service.rs`、`Dashboard.svelte`、`App.svelte`、`types.ts`、
`refreshStatus.ts` / `refreshStatus.test.ts`、16 个 locale 字典）

这一轮修的是同一行上的两个问题，第二个是用户看完第一次修复后的追加反馈。

## 用户报告

1. 首次登录 workbuddy 与 trae 之后，数据已经拿到，但顶部那一行仍显示「上次刷新失败」。
2. 清掉缓存重新登录后「还是一样，数据有了，但是还是更新失败的标志」——截图里看的是
   WorkBuddy（有数据），而同一行右侧仍写着「上次刷新失败」。

## 结论

**均已修复。**

- 问题 1 的根因：顶部那一行依赖的两个事实只有批次收尾会写；登录走的是单点刷新，
  不经过批次收尾，于是上一批「全失败」留下的标记一直挂着。
- 问题 2 的根因：那一行当时按**全局**结论显示。截图那一刻 `codex`（连不上）和
  `trae-cn`（没登录）确实在失败，于是「看 WorkBuddy」也被算成失败。

| # | 主张 | 判定 |
|---|------|------|
| 1 | 登录成功后顶部不再显示失败 | **VERIFIED**（`a_successful_single_provider_refresh_updates_the_top_row`） |
| 2 | 单点成功不会掩盖其他 provider 的故障 | **VERIFIED**（`a_single_provider_success_does_not_hide_another_providers_failure`） |
| 3 | 整批全失败不会推进「最近成功」时间 | **VERIFIED**（`a_batch_where_every_provider_failed_does_not_advance_the_last_successful_refresh`） |
| 4 | 空批次不推进时间、不改变失败结论 | **VERIFIED**（代码路径 + `completed > 0` 守卫） |
| 5 | 每个 provider 各带自己的失败标记 | **VERIFIED**（`each_provider_carries_its_own_failure_flag`） |
| 6 | 选中一个时只读它自己的时间与失败 | **VERIFIED**（`refreshStatus.test.ts` 的 `refreshStatusSource` 5 例） |
| 7 | 选中没数据的 provider 时不借别人的成功 | **VERIFIED**（同上「never borrows」用例） |
| 8 | 真实界面上的像素表现 | **NOT PROVEN** — 见文末 |

## 日志证据（`~/Library/Logs/Quota01/Quota01.log`）

问题 1：

```
10:11:03.511 [refresh] batch start (2 providers, force=false)
10:11:03.512 [refresh] batch end (1ms, 0 ok / 2 failed)      <- 上一批全失败
10:12:50.675 [plugin:trae-cn] refresh start (force=true)      <- 单点刷新，不经批次
10:12:51.018 [auth] WebView session saved for trae-cn
10:13:01.096 [plugin:workbuddy-cn] refresh start (force=true)
10:13:02.602 [auth] device-code session saved for workbuddy-cn
10:13:08.247 [refresh] batch start (3 providers, force=true)  <- 只有这一批清了标记
10:13:10.502 [refresh] batch end (2254ms, 3 ok / 0 failed)
```

两次登录成功后数据已经在屏幕上（`refresh end` 无错误），但失败标记直到 10:13:08
那次全量刷新才被清掉——正是用户看到的现象。

问题 2（清缓存后重登，截图那一刻的状态）：

```
15:00:15.945 [plugin:trae-cn] refresh failed (0ms, kind=Authentication): Sign in to TraeWork CN
15:00:17.326 [plugin:workbuddy-cn] refresh end (1382ms)          <- WorkBuddy 拿到了数据
15:00:23.949 [plugin:codex] refresh failed (8005ms, kind=Network): Could not connect to Codex
15:00:23.961 [refresh] batch end (8017ms, 1 ok / 2 failed)        <- 全局结论 = 失败
```

整批确实有一个是旧的，所以**全局**标记仍然是失败；但用户眼前那份 WorkBuddy 数据是新的。
那一行现在只描述眼前这份，所以不再亮标记。

## 修复内容

`src-tauri/src/service.rs`

- `run_refresh_flight` 落地处（`apply_refresh_result` 之后）调用
  `note_refresh_outcome`，单点刷新由此也会更新顶部那一行。
- `note_refresh_succeeded`：只在真的拿到数据时推进 `last_successful_refresh_at`。
- `note_refresh_failure_flag`：按各 provider **现况**重算 `last_refresh_failed`，
  而不是沿用「上一批有没有失败」。
- `provider_refresh_failed`（新增自由函数）：单条判定 = `error.is_some()` 或
  `last_failed_refresh` 仍含该 provider。两个信号都要看——刷新开始时会把 `error`
  清掉好让界面转圈，只看 `error` 会把正在重试的 provider 误判成已恢复。整批用的
  `any_enabled_provider_failed` 与每个 provider 的状态字段共用这一条规则。
- `state()` / `provider_state()`：给每个 provider 填上它自己的 `last_refresh_failed`。
  `state()` 里这把锁必须限定在块内——函数末尾的 `next_refresh_at()` 也要读它，而
  `std::sync::Mutex` 不可重入，持着再进就是自锁死（本轮就踩到并修掉了）。
- 两条合成错误的早退路径（未知 provider、刷新协调不可用）显式带上
  `last_refresh_failed: true`，免得出现「有 error 却说自己没失败」。
- `refresh_enabled_with_progress`：`succeeded > 0` 才推进时间；`completed > 0` 才重算
  标记（空批次保留上一次结论）。

`src-tauri/src/models.rs`

- `ProviderViewState` 增加 `last_refresh_failed`（`#[serde(default)]`），
  默认 `false`。

前端

- `src/lib/refreshStatus.ts`：新增纯函数 `refreshStatusSource(viewState, providerId)`——
  选中了某个 provider 就取它自己的 `snapshot.refreshedAt` + `lastRefreshFailed`，
  没选中才退回整体。抽出纯函数是为了能直接钉住「看 A 时不报 B」这条规则。
- `src/lib/Dashboard.svelte`：新增 `statusProviderId` 属性，顶部那一行改读
  `refreshStatusSource`。
- `src/App.svelte`：`dashboardProps` 里传 `statusProviderId: selectedProviderId`。
- `src/lib/types.ts`：`ProviderViewState.lastRefreshFailed?`。
- 16 个 locale 字典沿用上一轮加的键，没有新增文案。

## 门禁 — 真实输出

```
corepack pnpm verify   -> exit 0，四个阶段全绿

cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
[exit=0] clean

cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.15s

cargo test --manifest-path src-tauri/Cargo.toml --all-targets
test result: ok. 856 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.75s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

corepack pnpm test
Test Files  48 passed (48)
     Tests  404 passed (404)

corepack pnpm check
svelte-check found 0 errors and 0 warnings

corepack pnpm lint
All matched files use Prettier code style!

corepack pnpm verify:versions      -> Quota01 version 0.7.10 is consistent.
corepack pnpm verify:contracts     -> 42 frontend Tauri commands match registered Rust command handlers.
                                      27 Rust/TypeScript model field contracts match.
                                      16 provider consumers … 20 runtimes … 其余全通过
```

定点复跑新增用例：

```
test service::tests::a_batch_where_every_provider_failed_does_not_advance_the_last_successful_refresh ... ok
test service::tests::a_partially_successful_batch_advances_the_timestamp_and_still_reports_failure ... ok
test service::tests::a_fully_successful_batch_clears_the_failure_flag ... ok
test service::tests::a_successful_single_provider_refresh_updates_the_top_row ... ok
test service::tests::a_single_provider_success_does_not_hide_another_providers_failure ... ok
test service::tests::each_provider_carries_its_own_failure_flag ... ok
test result: ok. 6 passed; 0 failed

src/lib/refreshStatus.test.ts  (15 tests)  ok
```

## 未覆盖 / 残余风险

- 没有截到 popup 里那一行的像素：`screencapture` 抓到的是别的窗口在前台，popup 当时
  不在屏上；没有用辅助功能权限去点开它。后端与纯函数两侧都有定点用例，但「这一行在
  真机上确实变成不带失败的样式」只能由用户肉眼确认。
- `.dev` 数据目录本轮再次清空（含登录凭据），workbuddy / trae 需要重新登录；备份在
  `/tmp/quota01-dev-backup-20260930-220926`。
- 顶部那一行的点击动作仍然是「刷新全部」，而文字现在描述选中的那一个 provider。
  这是有意保留的：刷新全部是超集，按钮的 tooltip 也还是「刷新全部」。
- 本轮未提交；工作树里另有一批与本修复无关的改动，未触碰。

