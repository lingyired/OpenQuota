# 第三方登录窗口问题记录

## 背景

TraeWork CN 走 WebView 登录：点 **Open Sign-In** 打开一个加载 `www.trae.cn` 的窗口，用户点「登录」后由抖音（Douyin）完成第三方授权。同一条建窗代码也被 DeepSeek 的 WebView 登录复用。

用户报障：**登录时无法打开新窗口，抖音授权窗口出不来，登录按钮一直转圈，界面像卡住**。

一次完整的抖音登录尝试同时暴露了四个互相独立的缺陷。前三个已修复并提交（`f180e36`），第四个是框架能力缺口，暂不处理。

涉及文件只有一个：`src-tauri/src/commands/provider.rs`。

## 已修复的缺陷

### 1. `window.open()` 返回 `null`，授权窗口不出现

**现象**：登录按钮永久转圈，抖音授权窗口完全不出现。

**根因**：建登录窗口时没有安装新窗口处理器。wry 默认 `new_window_req_handler: None`（`wry-0.55.1/src/lib.rs`），其 macOS 事件委托在 `webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:` 末尾直接返回 `None`：

```rust
      } else {
        None      // new_window_req_handler 为 None
      }
```

WebKit 收到 `nil` 后，页面里 `window.open()` 的返回值就是 `null`，且不会有任何窗口出现。抖音登录页依赖该句柄轮询授权结果，句柄为 `null` 时回调永不触发——按钮就永远停在 loading。

**修复**：建窗时声明处理器，交由 wry 默认实现弹出子窗口。

```rust
.on_new_window(|_url, _features| tauri::webview::NewWindowResponse::Allow)
```

`Allow` 分支创建的是裸 `NSWindow`，通过 `makeKeyAndOrderFront` 直接显示，并保存在 wry 自己的 `new_windows` 列表里，**不经过 Tauri/tao 的窗口注册表**，因此不会与 `trae-cn-login` 的 label 去重逻辑冲突。

### 2. 登录窗口可以被重复创建

**现象**：一次会话中出现三个同 label 的登录窗口（实测编号 `91758` / `91861` / `91863`，尺寸位置完全相同、彼此重叠，视觉上只看得到一个）。

**根因**：三段逻辑叠加出的竞态。

- `open_provider_webview_login` 当时是同步命令，Tauri 把它调度到 threadpool 执行，多个调用可以真正并发。
- Tauri 的窗口存在性校验是 check-then-act：`prepare_window` 先查 label 是否存在，真正插入发生在稍后的 `attach_window`。
- 窗口注册表只在 `Destroyed` 事件里清理（`tauri/src/app.rs` 的 `on_window_event`），而 `CloseRequested` 上装了 `api.prevent_close()`，把这段不一致的窗口期进一步拉长。

于是两个并发调用可以都通过存在性检查，各建一个同 label 窗口。

**修复**：把「查找 → 创建 → 安装事件钩子」整段投递到主线程执行，由事件循环串行化；命令相应改为 `async fn`，通过 channel 把结果回传。

```rust
app.run_on_main_thread(move || {
    let result = (|| -> Result<(), String> {
        if let Some(window) = event_app.get_webview_window(&auth.window_label) {
            let _ = window.show();
            let _ = window.set_focus();
            return Ok(());
        }
        /* build + register_provider_login_window */
    })();
    let _ = sender.send(result);
})?;
receiver.recv().await
```

重复调用现在只会命中已存在的窗口并把它带到前台。

### 3. 抓取读到了错误的窗口

**现象**：登录后抓取必然失败，日志里反复出现：

```
[auth] provider sign-in storage diagnostic: "{\"page\":\"taur[PATH]",\"queryKeys\":[],\"sessionKeys\":[],\"localKeys\":[\"quota01.lastSelectedProviderId\"]}"
```

**根因**：`page` 字段是 `tauri://localhost`，`localKeys` 只有 `quota01.lastSelectedProviderId`——这是 **Quota01 自己的 Main 窗口**，不是登录窗口。

抓取时的窗口选择逻辑给 Cookie 类 provider 留了一个静默回退：

```rust
WebviewCredentialSource::Cookie { .. } => login_window
    .clone()
    .or_else(|| app.get_webview_window(crate::window::MAIN_WINDOW)),
```

而「断开连接」会先 `close()` 掉登录窗口。窗口一旦关闭，`get_webview_window("trae-cn-login")` 返回 `None`，随后的抓取就落到 Main 窗口上。

这里不只是「抓不到」的问题：`read_cookie_session` 是按 cookie **名字**在该窗口内枚举匹配的，读错窗口意味着可能把无关 cookie 当成 provider 凭据写进 vault。

**修复**：Cookie 类 provider 也要求登录窗口本身存在，不再回退。

```rust
let session_window = login_window
    .clone()
    .ok_or_else(|| "Open the provider sign-in window first.".to_owned())?;
```

### 4. 同一次登录触发并发抓取

**现象**：两条抓取诊断落在同一毫秒（`12:28:16.207` 与 `.208`），两边都去读存储。

**根因**：多个登录窗口各自触发一次抓取，前端还会在 `provider-session-window-closed` 事件上自动 capture。重复的那次不仅白做功，其失败结果还可能覆盖先到的成功结果。

**修复**：`ProviderSessionCloseGuard` 增加「正在抓取」集合，每个 provider 同时只允许一次抓取；用 RAII 句柄持有，保证任何提前返回或报错路径都能释放，不会把 provider 永久锁死。

```rust
let Some(_capture_guard) = guard.begin_capture(&provider_id) else {
    return Err("The provider sign-in is already being read.".to_owned());
};
```

配套三个单元测试：拒绝重入、句柄释放后可再次取得、不同 provider 互不影响。

## 未处理：抖音子窗口无法自我关闭

**现象**：抖音授权完成后，那个 `window.open` 弹出的子窗口停在屏幕上，需要手动关闭。

**根因**：页面在授权完成后会调用 `window.close()` 自行关闭（标准 OAuth popup 回调页行为）。WebKit 处理该调用时，走的是 UI 委托的 `webViewDidClose:`；**这个方法在 wry、tao、Tauri 三层源码里都没有实现**（全仓检索确认，`objc2-web-kit` 本身提供了该 trait 方法）。回调没人接管，`window.close()` 就被静默忽略。

这是框架能力缺口，不是 Quota01 的配置问题，因此**用 `NewWindowResponse::Allow` 或 `Create` 都改不掉**——两者的区别只在于窗口归谁管，而 `Create` 的价值是让 Quota01 能主动关它。

要解决它，可行的路径是用 `Create` 接管子窗口，再由 Quota01 在检测到授权完成后主动 `close()`。前置条件是先确认抖音授权完成后子窗口的确切行为（是导航跳转，还是只给 opener 发消息而不导航）：

- 若会导航回 `trae.cn`，可用 `on_page_load` 监听 URL 变化触发关闭；
- 若只发消息不导航，则 `on_page_load` 抓不到，需要换别的信号。

该行为尚未观测，因此本轮未实现。

## 缺陷 5：Cookie 类 provider 关窗即抓取，而窗口已经没了

**现象**：用户报「明明登录好了、用量数据也看到了，卡片却一直显示请先登录 TraeWork CN，以查看用量」，并带着 **打开登录 / 我已登录** 两个按钮。日志里 trae-cn 每分钟一次 `refresh failed (0ms, kind=Authentication): Sign in to TraeWork CN to view usage.`，**除此之外没有任何抓取失败的痕迹**——没有 `storage diagnostic`，也没有 `session saved`。

**证据**：

- 网页侧确实已登录：`~/Library/HTTPStorages/com.lingyi.quota01.dev.binarycookies` 里有 `.trae.cn` 的 `X-Cloudide-Session`（值长度 61），共 30 条 cookie。
- Quota01 侧确实没凭据：解密 `~/Library/Application Support/com.lingyi.quota01.dev/credentials.vault`，账户只有 `commandcode` 与 `workbuddy-cn-session`，**没有 `trae-cn`**。
- 刷新失败耗时 `0ms`，说明本地就判定未登录，根本没发请求：`refresh_with_context` 的 `auth.load()?.ok_or(SessionMissing)` 返回了 `None`。

**根因**：抓取要求登录窗口**还活着**，而触发抓取的时机恰恰是窗口已经消失之后。

1. `capture_provider_session_inner` 第一步取窗口，取不到就直接返回
   `Open the provider sign-in window first.`；
2. 这条提前返回发生在 `log_web_storage_diagnostic` **之前**，所以整条路径**一条日志都不留**；
3. 而 `CloseRequested` 的拦截（`api.prevent_close()`）当时只对 `LocalStorage` 类 provider 生效——
   原注释写着「`Cookie` 类 provider（Trae）不走这条路径」；
4. 于是 Cookie 类 provider 的窗口直接销毁，`Destroyed` 才 emit
   `provider-session-window-closed`，前端收到后才去 `capture_provider_session`。

关键在于 Tauri 的时序：`on_event_loop_event`（`tauri/src/app.rs`）在把
`RunEvent::WindowEvent` 交给监听器**之前**就调了 `manager.on_window_close(label)`，而
`on_window_close`（`tauri/src/manager/mod.rs`）会把窗口从 `windows_lock` 摘掉。等前端那次
`invoke` 到达后端，`get_webview_window("trae-cn-login")` 只能是 `None`。

顺带说明为什么不能靠 Main 窗口兜底：`wry` 的 `cookies()`（`wry/src/wkwebview/mod.rs`）读的是
`data_store.httpCookieStore().getAllCookies()`，即**整个数据存储**的 cookie，本身与页面无关；
但窗口一旦销毁就没有 WebView 句柄可读，兜底到 Main 窗口是另一回事——那条路已在缺陷 3 中因读到
`tauri://localhost` 而被移除。

**修复**：两类凭据来源统一走 `CloseRequested` 拦截。

- 关窗时先 `api.prevent_close()`，再通知前端抓取，抓完由抓取路径自己 `close()`；
- 拦下的同时把窗口**隐藏**。用户点的就是「关闭」，不该看到一个关不掉的窗口；而
  `cookies()` 只读 `WKWebsiteDataStore`，与窗口是否可见无关，抓取照常拿得到凭据；
- 抓取失败（登录没完成）时把窗口重新 `show()` 出来，让用户接着完成登录，而不是让窗口
  看起来已经消失、卡片却还在报未登录；
- 抓取失败还会 `mark` 放行关闭请求，避免把用户卡在一个关不掉的窗口里。

两条新的兜底，都是为了不再出现「无声失败」：

- 关闭被拦下后若 **5 秒**（`CLOSE_HOLD_GRACE`）仍无人接手，或用户再次点关闭，直接放行。
  拦截依赖前端监听，而承载监听的 `ProviderSessionActions` 只在卡片处于错误态时才渲染
  （`Dashboard.svelte` 的 `state.errorKind === 'authentication' | 'permission' | 'credentialStorage'`）；
  没有这条兜底，从别处打开的登录窗口会变成一个关不掉的窗口；
- `Destroyed` 走到「没被 `consume`」的分支时不再 emit 一个必然失败的抓取，改为记
  `was destroyed without a capture; credentials were not saved`；抓取取不到窗口时同样记
  `capture for {provider} found no sign-in window`。

**影响面**：`LocalStorage` 类 provider（DeepSeek 等）的关窗行为只多了一条宽限期兜底，正常路径不变。

## 验证情况

| 项目                         | 结果                                                                                                                                 |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `cargo check`                | 通过                                                                                                                                 |
| `cargo clippy --all-targets` | 无警告                                                                                                                               |
| `cargo test --all-targets`   | 853 项通过（含本次新增 5 项）                                                                                                        |
| `pnpm test`                  | 397 项通过（48 个文件）                                                                                                              |
| `svelte-check` / `lint`      | 0 错误 0 警告；Prettier 全部通过                                                                                                     |
| `pnpm verify:contracts`      | 通过（含 27 项 Rust/TypeScript 模型字段契约、16 个 locale）                                                                          |
| 实机验证                     | 缺陷 1 已由用户确认修复（可完成抖音登录，日志出现 `WebView session saved for trae-cn`，trae-cn 刷新由 `0ms 失败` 变为 `357ms 成功`） |

**关于本机跑前端工具链的坑（供参考，非仓库缺陷）**：DSH 内置的 node
（`com.deepseek.dsh.runtime.*`，`flags=runtime`，Team ID `NAN929V4UM`）会拒绝加载
`@rolldown/binding-darwin-arm64`——该 binding 是 adhoc 签名、无 Team ID，macOS 报
`mapping process and mapped file (non-platform) have different Team IDs`。原生 binding 加载失败后
rolldown 落到 WebContainer 回退分支，去 `pnpm i @rolldown/binding-wasm32-wasi`，于是报
`Cannot find module '@rolldown/binding-wasm32-wasi'`。**换成 nvm 的 node
（`~/.nvm/versions/node/v24.16.0/bin/node`，Team ID `HX7739G8FX`）即可正常运行
`vitest` 与 `vite build`**，无需改动仓库或 node_modules。

**尚未端到端验证**：缺陷 2、3、4、5 目前只有代码层与单元测试层的证据。它们的实机表现需要在一次完整登录中确认，建议下次登录时留意三点——反复点击 **Open Sign-In** 不再堆出多个窗口；关窗后日志应出现 `WebView session saved for trae-cn`；登录没完成就关窗时窗口会自己回到屏幕上。

## 排查过程中的错误方向（供参考）

复盘时值得记下，因为这几条都基于当时看似有力的证据：

- **误判为 tao 主线程 panic**。tao 在 macOS 上对非主线程建窗有硬断言 `panic!("Windows can only be created on the main thread on macOS")`，而同步命令确实跑在 threadpool 上，推理链一度看起来完整。但日志里从来没有 `PANIC` 行，且窗口实际被创建出来了，该假设被证伪。
- **误判为进程冻结**。一份日志读到中段时恰好在 `11:35` 截止，误以为「日志停止 = 事件循环停转」；实际日志一直在滚动，只是两次读取之间有间隔。进程主线程 `sample` 显示停在 `mach_msg`，是正常的空闲等待。
- **误判为代理阻断**。系统代理指向 `127.0.0.1:10808`，而 `lsof` 查不到监听进程，一度以为是代理已退出导致 WebView 请求挂起。实际是该端口的监听进程属于其他用户、`lsof` 权限不足看不到；直连与走代理访问 `trae.cn` 都返回 200。
- **把「卡片说未登录」当成「网页没登录」**。两者是分开的状态：网页登录体现在 WebView 的 cookie，而刷新用的是 Quota01 vault 里的会话。缺陷 5 里正是 cookie 在、vault 不在。区分二者最快的办法是直接解密 vault 并对照日志——`0ms` 失败就已经说明请求根本没发出去。

真正把方向纠正过来的是用户提供的登录窗口截图（登录页正常渲染、只有按钮卡在转圈）+ 一句「点击后应该是 `window.open` 一个抖音的登录界面」。定位问题时，**能直接观察到的页面行为比间接的日志推断可靠得多**。

## 相关提交

- `f180e36` — `fix: unblock provider sign-in popups and duplicate sign-in windows`
- 本次 — `fix: capture cookie providers before the sign-in window is destroyed`
