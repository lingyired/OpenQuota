# macOS 凭据来源移除 Keychain 设计

日期：2026-09-26

## 目标与边界

Quota01 在 macOS 上读取五个 provider 的凭据时不得访问系统 Keychain，也不得通过外部命令间接触发 Keychain 提示。凭据缺失或来源不符合本设计时，provider 返回明确的不可用状态，不尝试其他凭据源。provider 实例和上次成功取得的额度快照保留，快照标记为过期并显示更新时间。

本次只定义凭据来源与运行行为。实施顺序固定为：

1. 移除 macOS Keychain 访问并替换 Quota01 自有 vault 密钥来源。
2. 重构设置窗口与 provider popup 窗口。
3. 配置代理后，对五个 provider 执行真实在线验收。

每个阶段都必须能够编译，并通过该阶段相关的局部回归。第三阶段是五个 provider 的整体在线验收；UI 阶段完成前不要求进行它。

本设计的凭据边界覆盖 Codex、Cursor、GitHub Copilot、Antigravity、Claude Code，以及 Quota01 自有加密 vault 的主密钥。Windows 凭据管理器继续保留。Linux 凭据来源不在本次范围内。

## 设计选择

采用 provider 明确的本地凭据来源，并在 macOS 构建中删除系统 Keychain 后端。每个 provider 仅访问下表列出的来源。来源不可读、缺失、损坏或刷新失败时，返回对应的结构化错误；不回退到 Keychain、另一应用的 credential store 或外部 CLI。

| Provider | 唯一凭据来源 | 刷新行为 | 不可用行为 |
| --- | --- | --- | --- |
| Codex | `CODEX_HOME/auth.json`；未设置时按现有候选路径读取 `~/.config/codex/auth.json`、`~/.codex/auth.json` | OAuth 刷新成功后原子写回相同的 `auth.json`，保留文件其余字段 | 文件缺失、无有效 OAuth token、文件不可写或刷新失败时返回来源/认证错误 |
| Cursor | Cursor `state.vscdb` 中的 `cursorAuth/accessToken` 与 `cursorAuth/refreshToken` | 刷新成功后更新同一数据库记录；不得切换到 Keychain 账户 | 数据库或 token 不可用时返回凭据不可用；保留实例和旧快照 |
| GitHub Copilot | Copilot editor 配置（`apps.json`、`hosts.json`）或 GitHub CLI `hosts.yml` 中直接存在的 `oauth_token` | 当前 Copilot 用量请求使用配置 token；本设计不增加 gh CLI 登录或 token 获取过程 | 配置无 token 时返回凭据不可用；macOS 不执行 `gh auth token` |
| Antigravity | 本机 Antigravity language server quota RPC | 不读取 refresh token，也不访问云端 OAuth refresh 路径 | language server 不可发现、RPC 失败或响应不可解析时返回本机服务不可用 |
| Claude Code | Claude Code CLI `~/.claude/.credentials.json`（或对应 `CLAUDE_CONFIG_DIR` 下文件）和 `CLAUDE_CODE_OAUTH_TOKEN` 环境变量 | 文件凭据刷新后原子写回同一文件；环境变量凭据只用于 quota 请求，不尝试持久化或刷新 | 文件/环境变量均不可用时返回 credentials unavailable。只检测到 Claude Desktop 数据时返回 Desktop-only unsupported |

Cursor 数据库中 refresh token 或用户资料状态的读取仍以当前实现为准；不得在刷新时读写系统凭据存储。Codex reset-credit claim 是 Codex 的另一条认证入口，必须通过同一文件来源加载候选凭据。其手动 claim 不得恢复旧 Keychain 路径。

Copilot 配置文件中的 token 可能由其他应用以明文保存。Quota01 只读取已配置文件中直接存在的 token 字段，不调用 `gh auth token`，也不尝试解密、查询 Keychain 或推断 gh 的 token 存储后端。`GH_CONFIG_DIR`、`XDG_CONFIG_HOME` 和既有配置路径发现规则继续适用。

Antigravity 本机 RPC 使用专用 HTTP client，并显式绕过所有代理。访问目标仅为已发现的 `127.0.0.1` language-server 端口。该 client 不接受系统 HTTP proxy 或 `HTTP_PROXY`/`HTTPS_PROXY` 环境代理。远端 Antigravity cloud、Google token refresh 和读取本机保存 refresh token 的旧分支不再是 macOS quota 流程的一部分。

## Quota01 自有凭据 vault

现有 vault 使用 ChaCha20-Poly1305 加密数据文件，并将随机 32 字节主密钥放在系统凭据存储。macOS 改为将主密钥放在 Quota01 专属数据目录中的独立本地文件；数据目录权限为 `0700`，密钥文件权限为 `0600`。文件系统不支持或无法验证所需权限时，vault 初始化失败，不降低权限后继续保存密钥。

首次创建主密钥时，生成随机 32 字节密钥，写入同目录临时文件、设置权限、flush 并原子 rename。并发启动不得产生多个有效密钥；创建流程需使用原子 create/锁定语义，若另一个进程先完成创建，则读取该密钥。密钥只在进程内存中缓存，并在释放时清零。加密格式、nonce、AAD 和现存 vault 数据结构保持不变。

若 vault 数据文件已存在但本地密钥文件缺失、长度错误或权限过宽，初始化返回可识别的 vault-key error，现有加密数据保持不变。界面向用户说明 vault 无法解密，并提供明确的重置入口；重置会删除 Quota01 vault 数据和本地密钥，再创建新密钥。此流程不得静默覆盖旧数据或尝试 Keychain 恢复。

当前没有已发布用户需要从 Keychain 自动迁移。开发者改用本地密钥文件前需重新录入保存在 Quota01 vault 中的 secret；实现不读取旧 Keychain 来搬迁数据。Windows 继续通过 Windows Credential Manager 保护 vault 主密钥。Linux 的 Secret Service 行为保持现状，另不在本次设计范围内。

本地密钥文件避免 macOS Keychain 访问与弹窗，但其安全性依赖账户文件权限、磁盘加密和本机账户隔离；获得同一用户目录访问权的恶意程序可能同时读取密文 vault 与主密钥。这是以零 Keychain 访问为优先的明确安全权衡。文件权限是必要保护，不等同于硬件保护或 Keychain 隔离。

## 检测、启用与刷新数据流

启动 credential detection、用户手动启用、周期 refresh、手动 refresh、OAuth token refresh、Codex reset claim 和 Claude account discovery 必须使用同一组 provider 本地来源规则。检测只检查本设计规定的本地文件/数据库，或 Antigravity language server 是否提供可用本机 RPC；检测不得因旧 Keychain 项存在而报告已检测。

启用 provider 后，运行时按上述唯一来源加载凭据并执行额度读取。周期刷新和手动刷新复用同一个 runtime，不因调用入口不同而扩展凭据候选。需要 token refresh 的 Codex、Cursor、Claude 只允许读取、刷新并写回其对应文件或数据库。刷新写回失败视为该次刷新失败；不得把新 token 只留在内存后继续报告成功。

任何凭据来源缺失、无法解析或刷新失败时：

1. 返回可区分的结构化错误类别，包括凭据缺失、来源不可读、凭据格式无效、token 已过期、刷新失败、vault 密钥不可用、Antigravity 本机服务不可用、Claude Desktop-only。
2. 不把缺失来源伪装成“未安装”，不自动删除 provider 实例，不自动关闭 provider。
3. 保留上一次成功的 quota snapshot，并记录其最后更新时间及过期状态；UI 显示明确的当前错误和缓存时间。
4. 下一次周期刷新仍按相同唯一来源重试。用户修复本地来源后，provider 可恢复，不要求删除后重新添加。

## Keychain 不可达保证

macOS 产品代码不应链接或调用 Security.framework。删除 `security-framework` macOS 依赖、provider Keychain 分支、credential-store existence probe 与任何 `keychain_access` 授权状态。同步设置时不再发布“允许读取其他应用 Keychain”的 grant；启动检测逻辑也不再用 Keychain 可达性推断 `Unknown`。

下列间接入口必须一并移除：Copilot `gh auth token` 子进程；Codex reset claim 的通用候选加载；Claude account discovery 对 Keychain service existence 的探测；五个 provider 的 `has_local_credentials` 中 Keychain 探测；Antigravity 本机 RPC 失败后的 refresh-token/cloud fallback；以及 macOS vault 初始化时的 owned-password Keychain 读写。

`credential_store` 在 Windows 和 Linux 下保留各自平台实现。macOS 对外提供本地 vault 主密钥后端，不再暴露能发出 Security.framework 查询的 external/owned password 实现。其他 provider 的 macOS Keychain 路径也必须经仓库全局搜索确认无残留，不能只保证五个重点 provider。

“完全不访问 Keychain”指 Quota01 的产品运行代码在 macOS 不调用系统钥匙串 API，且不通过外部命令要求系统钥匙串 token。Cargo 编译、测试和开发启动同样遵守此约束；任何开发或测试流程都不得用旧 credential store 分支探测本机真实凭据。

## HTTP 代理与本机 RPC

Codex、Cursor、Copilot、Claude 和 Antigravity 的公网请求客户端继续使用 reqwest 默认代理环境行为。真实在线验收通过测试进程显式设置 `HTTP_PROXY` 和 `HTTPS_PROXY`；测试记录代理地址是否生效，但不记录凭据、Authorization header 或 token。代理变量只注入验收进程，不持久化到 Quota01 设置或用户配置。

Antigravity local RPC 的 client 必须设置 `no_proxy()`，并只请求 loopback。公网请求仍走环境代理。测试需要分别验证公网请求经过代理，以及本机 RPC 即使设置全局代理仍直连 loopback。

## 错误与用户可恢复性

用户可通过设置窗口中 provider 的认证/状态区域看到具体来源及错误类别。错误不展示 token、refresh token、配置文件完整内容、vault 密钥或带认证信息的代理 URL。日志只记录 provider id、错误类别和必要的 HTTP 状态，不记录秘密或请求 header。

Codex、Cursor、Claude 的认证失效与无法刷新需要提示用户回到对应应用重新登录，确认其本地凭据文件/数据库重新生成后再重试。Copilot 缺少可直接读取 token 的配置时，提示用户提供可读取的 Copilot/gh 配置来源；Quota01 不启动 gh 登录流程。Antigravity 不在线时提示打开 Antigravity/agy，继续保留实例与旧快照。检测到 Claude Desktop 的 token cache 与 Cookies 文件、但没有 Claude Code CLI 凭据时，标记为 Desktop-only unsupported；不读取 Desktop cookie 或 token cache。

## 验收门槛与测试策略

### 阶段一：macOS 零 Keychain 凭据重构

- macOS target 编译成功，Windows credential manager 与 Linux Secret Service 仍保留各自实现。
- 全仓产品代码搜索不存在 Security.framework 调用、Keychain 查询、Codex/Claude/Cursor/Antigravity Keychain candidate、Copilot Keychain lookup、`gh auth token` 执行或 vault 主密钥的 system credential-store 后端。
- 在设置中启动检测、启用五个 provider、执行周期与手动刷新、token refresh、Codex reset claim 和 Claude account discovery 时，不出现 Keychain 访问提示；本机测试账号是否存在真实旧 Keychain 项不影响结果。
- 对每个 provider 测试：唯一来源存在、缺失、格式无效、权限/读写错误、token 过期或刷新失败；验证错误类别、旧 snapshot 保留与更新时间、实例不被删除/关闭。
- Codex 与 Claude refresh 后验证同一来源文件更新；Cursor refresh 后验证同一 `state.vscdb` 对应 key 更新；Copilot 测试验证不执行 gh 子进程；Antigravity 测试验证 RPC 成功路径与 RPC 失败后的 unavailable 行为。
- Vault 测试验证目录/密钥权限、首次原子创建、并发创建只得到一个密钥、现存加密 vault round-trip、密钥丢失/权限过宽不覆盖旧数据，并验证显式 reset 行为。

### 阶段二：独立设置窗口与 provider popup

- 独立设置窗口和 provider popup 实现与已批准的窗口设计一致；检查错误状态与旧快照缓存时间在 popup 中可见，设置按钮始终可用并定位到对应 provider。
- macOS 移除总 app icon 的窗口/UI 改造不得重新引入 Keychain 权限提示、凭据检测副作用或 provider 实例自动删除。
- 本阶段编译成功，并通过窗口路由、provider 状态/错误显示、设置定位和 macOS menubar 局部回归。

### 阶段三：经代理的五 provider 在线验收

此阶段在 UI 阶段完成后运行。使用受控测试环境中的真实本地 provider 凭据，禁止把 token 写入日志、截图、测试输出或提交文件。代理配置仅存在于验收进程环境。

对 Codex、Cursor、Copilot、Antigravity、Claude 分别验收：

1. 本地来源 detection 与首次启用结果正确，只有用户选择启用才创建/启动实例。
2. 启用后周期刷新实际通过 HTTP_PROXY/HTTPS_PROXY 完成 quota 请求，显示正确余额/quota 与 reset/expiry 时间。
3. 手动刷新沿用相同代理和凭据路径，返回一致且可解释的状态。
4. 对支持 OAuth refresh 的 provider，使用临近到期或受控可刷新凭据触发实际 token refresh，并确认新 token 写回规定的同一文件/数据库；Copilot 不执行 gh 命令；Antigravity quota 只通过 loopback RPC 且不经过代理。
5. 关闭 popup 后后台刷新仍运行；模拟网络故障或凭据错误后实例仍在、旧值带更新时间且标记过期，修复后可恢复刷新。

任一 provider 的 detection、启用、周期刷新、手动刷新、适用的 token refresh 或存储写回失败，均不得宣称五 provider 在线验收完成。代理不可用时将验收结果标记为受阻并保留具体失败项，不回退到直连网络。

## 范围外

- Windows Credential Manager 的凭据来源改造。
- Linux Secret Service 的凭据来源改造。
- 将 provider token 导入 Quota01 自有 vault，或让 Quota01 代替 provider 完成登录。
- Claude Desktop token/cookie 解密或复用。
- 代理 UI、代理凭据持久化、自动探测系统 PAC/代理设置。
- macOS Keychain 凭据迁移。开发者重新录入 Quota01 自有 vault 内的 secrets；provider 本地凭据仍由对应应用管理。
