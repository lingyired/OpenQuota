# Quota01 provider 收录候选清单

本文记录 provider 的**收录规则、当前名单、以及尚未收录的候选**（含接入成本分档、已证伪的路由、
以及已放弃的候选）。

- 名单与首轮探针结论截至 **2026-09-18**。
- **2026-09-18 第二轮**：对全部候选做了开源生态检索 + 无凭据探针 + 协议级细读，
  结果补进第二/三/四节与附录。凡结论都标注 **【实测】**或 **【实现所述】**（= 来自开源实现，
  本轮未用真凭据复现）或 **【推断】**。不要把「实现所述」当已验证。

## 收录规则

> Quota01 主打国内支持，同时支持国际大厂，国际小众的不支持。

判据按顺序：

1. **国内厂商** —— 收录，含其国内站。
2. **国际大厂** —— 收录（自研前沿模型、有独立订阅体系）。
3. **国际小众** —— 不收录（模型/服务非自研，或体量不足）。
4. **同厂双站** —— 国内外账号体系分离时，**拆成两个 provider 并存**，国内侧用 `-cn` 后缀，
   **id 与显示名都标**（先例 `trae-cn` / `TraeWork CN`）。

## 一、已收录（19 个 runtime）

注册顺序即 `src-tauri/src/lib.rs` 的组装顺序，也是
`scripts/verify/verify-provider-registry-contract.js` 里 `expectedRuntimeOrder` 的断言顺序。

### 国内（8）

| id | 显示名 | 站点 | 凭据形态 | 文档 |
|---|---|---|---|---|
| `deepseek` | DeepSeek | `platform.deepseek.com` | WebView 登录 | [deepseek.md](providers/deepseek.md) |
| `infini` | Infini | `cloud.infini-ai.com/maas/coding` | API key | [infini.md](providers/infini.md) |
| `kimi-cn` | Kimi CN | `api.kimi.com` | API key | [kimi-cn.md](providers/kimi-cn.md) |
| `minimax-cn` | MiniMax CN | `www.minimaxi.com` | API key | [minimax-cn.md](providers/minimax-cn.md) |
| `siliconflow-cn` | SiliconFlow CN | `api.siliconflow.cn` | API key | [siliconflow-cn.md](providers/siliconflow-cn.md) |
| `trae-cn` | TraeWork CN | `api.trae.cn` | WebView 登录 | [trae-work-cn.md](providers/trae-work-cn.md) |
| `workbuddy-cn` | Workbuddy CN | `www.workbuddy.cn` | 复用本机登录数据 | [workbuddy-cn.md](providers/workbuddy-cn.md) |
| `zai-cn` | Z.ai CN | `open.bigmodel.cn` | API key | [zai-cn.md](providers/zai-cn.md) |

### 国际（11）

| id | 显示名 | 站点 / 数据源 | 凭据形态 | 文档 |
|---|---|---|---|---|
| `claude` | Claude Code | 本机 | 本机凭据（多账号） | [claude.md](providers/claude.md) |
| `codex` | Codex | 本机 | 本机凭据（ChatGPT 登录） | [codex.md](providers/codex.md) |
| `cursor` | Cursor | 本机 | 本机凭据 | [cursor.md](providers/cursor.md) |
| `antigravity` | Antigravity | 本机 | 本机凭据 | [antigravity.md](providers/antigravity.md) |
| `copilot` | Copilot | 本机 | 本机凭据 | [copilot.md](providers/copilot.md) |
| `grok` | Grok | 本机 | 本机凭据 | [grok.md](providers/grok.md) |
| `opencode` | OpenCode Go | `opencode.ai/zen/go` | API key | [opencode.md](providers/opencode.md) |
| `openrouter` | OpenRouter | `openrouter.ai` | API key | [openrouter.md](providers/openrouter.md) |
| `minimax` | MiniMax | `www.minimax.io` | API key | [minimax.md](providers/minimax.md) |
| `zai` | Z.ai | `api.z.ai` | API key | [zai.md](providers/zai.md) |
| `siliconflow` | SiliconFlow | `api.siliconflow.com` | API key | [siliconflow.md](providers/siliconflow.md) |

**凭据形态分布**：API key 10 个（`openrouter` / `zai` / `zai-cn` / `kimi-cn` / `minimax` /
`minimax-cn` / `siliconflow` / `siliconflow-cn` / `infini` / `opencode`）· WebView 登录 2 个
（`deepseek` / `trae-cn`）· 读本机已有凭据 7 个。

### 已移除

- `devin` —— 国际小众，2026-09-17 移除（模块、图标、文档、日志脱敏正则一并删）。

### 边界待定

- `cursor`（Anysphere）—— AI 编程头部，但不属于“大厂自研模型”档，**保留与否待拍板**。
  当前按存留处理（已收录、有文档）。

## 二、候选清单（未收录）

### 2.0 先看市场盘子

国内 Coding Plan 目前共 **6 家**，Quota01 只覆盖 3 家：

| 厂商 | 套餐档位 | 状态 |
|---|---|---|
| 智谱 GLM Coding | ¥49 / 149 / 469 | 已覆盖（`zai-cn`） |
| Kimi Code | ¥49–699 | 已覆盖（`kimi-cn`） |
| MiniMax Token Plan | ¥29 / 49 / 119 | 已覆盖（`minimax-cn`） |
| **阿里云百炼 Qwen** | ¥40 / 200 | **未覆盖** |
| **火山方舟** | ¥40 / 200 | **未覆盖 · ❌ 已放弃** |
| **腾讯云知识引擎** | ¥40 / 200 | **未覆盖 · ❌ 已放弃** |

> **⚠️ 2026-09-18 更正**：上面「6 家」是首轮口径，**已过窄**。本轮在
> `AIddlx/coding-plans`（国内 Coding Plan 资料库）里另见 **京东云 JoyBuilder、快手 KwaiKAT、
> 讯飞星火、优云智算、无问芯穹 Infini-AI** 五家也出了 Coding Plan 套餐，且其中
> **无问芯穹** 已被开源实现接入（见 2.5）。选型时应按「有 Coding Plan 套餐的国内厂商」重新点一遍名。

### 2.0.1 开源生态地图（先看这里再动手）

**本轮最大的结论：候选里绝大多数不是「要自己逆向」，而是「已经有人实现过，抄协议就行」。**
动手前先按这张表查一遍，能省掉大量逆向：

| 项目 | 语言 | ★ | 许可 | 最近推送 | 对本轮候选的覆盖 |
|---|---|---|---|---|---|
| `steipete/CodexBar` | Swift | 21562 | MIT | 2026-09-17 | **91 个 provider**：StepFun / Alibaba(QwenCloud) / Doubao / MiMo / Sub2API / LLMProxy / LiteLLM / LongCat / Infini 等 |
| `farion1231/cc-switch` | Rust (Tauri) | 133369 | MIT | 2026-09-15 | 火山方舟 `GetCodingPlanUsage`、MiMo |
| `nguyenphutrong/quotio` | Rust CLI + Swift | 4865 | MIT | 2026-09-17 | **阿里 Coding Plan（字段级完整）** / Sub2API / LiteLLM / LLMProxy / ClawRouter |
| `SaladDay/cc-switch-cli` | Rust (Tauri) | 5107 | MIT | 2026-09-17 | **StepFun 余额 / SiliconFlow / DeepSeek / OpenRouter**（Rust 实现，最好抄） |
| `smallmain/vscode-unify-chat-provider` | TS (VSCode) | 713 | MIT | 2026-09-06 | **NewAPI / Sub2API / SiliconFlow / LiteLLM / Moonshot** |
| `zbndev/tidemark` | Rust (GTK4) | 58 | MIT | 2026-09-17 | **StepFun（模块注释就是协议文档）** / MiMo / Alibaba / Sub2API / LLMProxy / LongCat |
| `btsouth/ceiling` | Rust | 28 | MIT | 2026-09-16 | **StepFun / Doubao / MiMo / Alibaba / Infini**（Rust，CodexBar 的移植） |
| `QuantumNous/new-api` | Go | 48307 | AGPL-3.0 | 2026-09-17 | 中转站本体；`controller/channel-billing.go` 内含 SiliconFlow 等上游余额探测 |

本地已 clone 可直接读：`~/Documents/github/{ceiling,tidemark,openusage}`。

> `robinebers/openusage`（本地也有）**不覆盖**本轮任何候选 —— 它只做本机 CLI 用量。
> `zbndev/tidemark` 的 `crates/tidemark-core/src/providers/keyed/*.rs` 是**质量最高的一批**：
> 每个文件顶部的 `//!` 注释几乎等于一份协议文档（端点、鉴权、字段、边界都写了），
> 且刻意不实现「自动刷新凭据」这类有副作用的路径。**建议以它为模板。**

### 2.1 第 1 档 —— 纯 API key，一个 GET，与现有 `ApiKeyStore` 同构

接入成本最低：建 `auth.rs` 照抄 `kimi/auth.rs`，实现 4 个 trait 方法即可。

| 厂商 | 端点 | 探针（无凭据） | 备注 |
|---|---|---|---|
| 阶跃星辰 StepFun | `GET api.stepfun.com/v1/accounts` | 401 `invalid_api_key` | |
| **硅基流动 SiliconFlow** | `GET api.siliconflow.cn/v1/user/info` | 401 `code:30014` | ✅ **已落地**（`siliconflow` + `siliconflow-cn`） |
| 阿里云百炼 | `GET dashscope.aliyuncs.com/api/v1/quotas` | 401 `InvalidApiKey` | 同时补上 Qwen Coding Plan 空白 |
| Kimi 余额（开放平台） | `GET api.moonshot.cn/v1/users/me/balance` | 401 `Invalid Authentication` | 与 `kimi-cn` 是**两条产品线**：Code 订阅 vs 开放平台余额 |
| DeepSeek 余额 | `GET api.deepseek.com/user/balance` | 401 `authentication_error` | 可把 `deepseek` 从 WebView 登录**降级**为 API key，少一个 Keychain 弹窗来源 |

> 最后两行属于**已收录 provider 的增强项**，按要求本轮未展开，仅保留端点事实，见附录 A。

以下把三家（真正的新 provider）展开到协议级；SiliconFlow 已实现，另两家待做。

#### 2.1.1 阶跃星辰 StepFun

**⚠️ 有两条完全独立的产品线，别混（这是本次最重要的发现之一）：**

| 产品线 | 取到的东西 | 端点 | 凭据 |
|---|---|---|---|
| **A. 开放平台余额**（按量计费） | 现金/代金券钱包余额 | `GET https://api.stepfun.com/v1/accounts` | API key（`Bearer sk-...`） |
| **B. Step Plan 订阅**（Coding Plan） | 5 小时 / 每周窗口、积分池 | `platform.stepfun.com` 的两个 Dashboard RPC | `Oasis-Token` cookie |

首轮文档只记了 A，且误以为 A 就能补 Step Plan 空白。**A 是钱包余额，不是套餐额度。**

**A 路（纯 API key，最低成本）**

- 方法 / 路径：`GET https://api.stepfun.com/v1/accounts`
- 头：`Authorization: Bearer <api_key>`、`Accept: application/json`
- **【实测】**无凭据 → `HTTP 401` + `{"error":{"message":"Incorrect API key provided","type":"invalid_api_key"}}`；
  对照组乱路径 `GET /v1/zzz_bogus_path` → `404`（空体）⇒ **路由存在已证实**。
  （`/v1/user/balance` → 404，那条路是错的。）
- **【实现所述】**成功响应字段：`{object, type, balance, total_cash_balance, total_voucher_balance}`
  —— 参考 `SaladDay/cc-switch-cli` `src-tauri/src/services/balance.rs::query_stepfun`（Rust）。
  余额取值用顶层 `balance`，单位 CNY。

**B 路（Step Plan 订阅，凭据形态是 Cookie）**

- `POST https://platform.stepfun.com/api/step.openapi.devcenter.Dashboard/QueryStepPlanRateLimit`
  —— 带窗口与用量
- `POST https://platform.stepfun.com/api/step.openapi.devcenter.Dashboard/GetStepPlanStatus`
  —— 带套餐名（**可选**，失败不要连累主请求）
- **【实测】**两条无凭据均 `HTTP 401`（无 body）⇒ 存在。
- 凭据：`Oasis-Token` cookie 的值；每请求要**同时回放** `Oasis-Token=<token>; Oasis-Webid=<device_id>`，
  其中 `device_id` 从 token 自身 JWT payload 的**第二段 base64url** 解出（不校验签名）。
  payload 里没有 device 的 token 直接拒绝。
- **⚠️ 两版实现的常量不一致，落地前必须用真 token 校准**：
  `zbndev/tidemark` 用 `APP_ID=10300`；
  `btsouth/ceiling` 用 `STEPFUN_WEB_ID=734152690100432` / `STEPFUN_APP_ID=111003695`。
- **【实现所述】**响应字段：`five_hour_usage_left_rate`、`weekly_usage_left_rate`
  （**是「剩余」比例，已用 = 1 − 该值**）、`plan_credit_rate_limit`（积分池形态）、
  `plan_family`、以及 `status`/`code`/`message`/`desc`。
  两种形态按「响应里实际有什么」判定：有 live window → 滚动套餐；有 credit 字段 → 积分池；
  `plan_family` 只做平局裁决。
- **建议照 `tidemark` 的做法**：只接受粘贴 token，**不实现 RefreshToken 恢复流程**
  （ceiling 提到 `/passport/proto.api.passport.v1.PassportService/RefreshToken`；
  自动刷新会动用户凭据，属不可逆操作）。

#### 2.1.2 硅基流动 SiliconFlow —— ✅ 已落地（2026-09-18）

> **已按本节实现并注册**为 `siliconflow`（`api.siliconflow.com`，USD）与 `siliconflow-cn`
> （`api.siliconflow.cn`，CNY）。代码在 `src-tauri/src/providers/siliconflow/`，
> 用户文档见 [docs/providers/siliconflow.md](providers/siliconflow.md) /
> [docs/providers/siliconflow-cn.md](providers/siliconflow-cn.md)。
> 落地时的三点补充：
> 1. **双站都收录** —— 按「同厂双站、账号体系分离则拆两个 provider」的规则，与
>    `zai`/`minimax` 同型，`siliconflow-cn` 走 `-cn` 后缀。两站仅域名与结算币种不同。
> 2. **错误码分两层判定**：HTTP 401/403 → 无效 key；HTTP 200 但 `code != 20000` 或
>    `status != true` → 无效响应；其中 `code == 30014`（`Token is invalid.`）单独识别为**无效 key**，
>    正是第四节第 1 条那个坑的针对性处理。
> 3. **总余额兜底**：`totalBalance` 取不到时用 `chargeBalance + balance`；三者全缺才算无效响应。

- 方法 / 路径：`GET https://api.siliconflow.cn/v1/user/info`（国际站 `https://api.siliconflow.com/v1/user/info`）
- 头：`Authorization: Bearer <api_key>`、`Accept: application/json`
- **【实测】**无凭据 → `HTTP 401` + `{"code":30014,"data":null,"message":"Token is invalid."}`；
  对照组乱路径 → `404` + `text/plain` `Not Found` ⇒ **路由存在已证实**。
- **【实测】国际站与国内站同路径响应体一致**
  （`api.siliconflow.com` 与 `.cn` 都返回同一份 `code:30014`）⇒ **双站只需换域名**，
  可按 `zai`/`minimax` 的 `Site` enum 参数化处理。
- **【实现所述】**成功响应结构：`{code, status, message, data:{ balance, chargeBalance, totalBalance, ... }}`
  - `balance` = **赠送**余额，`chargeBalance` = **充值**余额，`totalBalance` = 总余额
  - 缺失兜底：`totalBalance` 取不到时 = `chargeBalance + balance`
  - **业务码校验：`code` 必须 == `20000` 且 `status` 为 `true`**，否则视为失败
    ⇒ 又一个 **HTTP 200 + 业务码**实例（见第四节第 1 条）
- 参考实现：`SaladDay/cc-switch-cli`（Rust）、`smallmain/vscode-unify-chat-provider`
  `src/balance/providers/siliconflow.ts`、`QuantumNous/new-api` `controller/channel-billing.go`。

#### 2.1.3 阿里云百炼 Qwen Coding Plan

**⚠️ 配额不在 DashScope 推理域取，走的是 OneConsole RPC。**（首轮文档把「DashScope 按量余额」
当成了 Coding Plan 的补位，这是两回事。）

- 方法 / 路径：`POST {base}/data/api.json`
  - `base`：国内 `https://bailian.console.aliyun.com` ｜ 国际 `https://modelstudio.console.alibabacloud.com`
  - 查询串：
    `action=zeldaEasy.broadscope-bailian.codingPlan.queryCodingPlanInstanceInfoV2`
    `&product=broadscope-bailian`
    `&api=queryCodingPlanInstanceInfoV2`
    `&currentRegionId=<cn-beijing | ap-southeast-1>`
  - 头（**三处都塞同一个 key**）：
    `Authorization: Bearer <key>`、`x-api-key: <key>`、`X-DashScope-API-Key: <key>`，
    外加 `Accept: application/json`、`Origin`、`Referer`（后两者按区域给）
  - body：`{"queryCodingPlanInstanceInfoRequest":{"commodityCode":"<见下表>"}}`
- 区域参数（**【实现所述】**，取 `nguyenphutrong/quotio` `catalog/coding.rs::AlibabaRegion`）：

  | 区域 | base | `currentRegionId` | `commodityCode` |
  |---|---|---|---|
  | 国内 | `bailian.console.aliyun.com` | `cn-beijing` | `sfm_codingplan_public_cn` |
  | 国际 | `modelstudio.console.alibabacloud.com` | `ap-southeast-1` | `sfm_codingplan_public_intl` |

- **【实测】**无凭据 → `HTTP 200` + `{"code":"ConsoleNeedLogin","message":"请登录","successResponse":false}`
  （国际站 message 为英文 `You need to log in.`）。
  **⚠️ 对照组乱 action 也返回同一份 `ConsoleNeedLogin`** ⇒ 网关在路由匹配前就拦了，
  **该 action 的存在性靠探针证明不了**，只能靠上表的开源实现交叉印证。
- 业务码判定：`success == false` → 失败；`code` 为 `login`/`needlogin` → 判定为「需重新登录」，
  其他非 `0`/`200`/`success` → 解析失败；`message` 含 `login` 同理。
- **【实现所述】**成功响应里找 `codingPlanQuotaInfo`（或在顶层/任意嵌套层找下列键），字段：

  | 窗口 | used | total | 下次刷新 |
  |---|---|---|---|
  | 5 小时 | `per5HourUsedQuota` | `per5HourTotalQuota` | `per5HourQuotaNextRefreshTime` |
  | 每周 | `perWeekUsedQuota` | `perWeekTotalQuota` | `perWeekQuotaNextRefreshTime` |
  | 每计费月 | `perBillMonthUsedQuota` | `perBillMonthTotalQuota` | `perBillMonthQuotaNextRefreshTime` |

- **另一条路（不同实现，走 Cookie + OneConsole `SEC_TOKEN`）**：`btsouth/ceiling` `alibaba/`
  —— 三步：从区域 dashboard 读 `SEC_TOKEN`（回退 `/tool/user/info.json` → cookie 本身）→
  带同一 cookie jar 发 console 的表单 RPC → 解析 plan instance。
  **它明确标注 `SEC_TOKEN` 是活的控制台凭据，请求/回包不得进 raw-response 记录器。**
  两条路都可用，**优先 quotio 的 API key 路**（不需要用户导 cookie）。
- ⚠️ `GET https://dashscope.aliyuncs.com/api/v1/quotas`（按量 key）**【实测】401 `InvalidApiKey` +
  乱路径 404 ⇒ 路由此存在**，但它是 **DashScope 按量配额**，**不保证等于 Coding Plan 额度**，
  两套服务不要混用同一条链路。

### 2.2 第 2 档 —— ❌ 腾讯云已放弃（2026-09-18）

| 厂商 | 端点 | 探针 | 备注 |
|---|---|---|---|
| 腾讯云 Coding Plan | `GET api.lkeap.cloud.tencent.com/coding/v3` | 401 `not_authorized_error` | 与 `workbuddy-cn` 属不同产品线，需独立适配 |

**本轮结论：腾讯云 Coding Plan 的配额接口「社区零实现」，必须自研（或去控制台逆向）。**

- Base URL（**推理**用，非配额）：
  OpenAI 兼容 `https://api.lkeap.cloud.tencent.com/coding/v3`；
  Anthropic 兼容 `https://api.lkeap.cloud.tencent.com/coding/anthropic`。
  API key 形态 `sk-sp-xxxx`，**与按量 `sk-xxxx` 完全不互通**（官方明文警告，用错会直接按 token 扣钱）。
- **【实测】**`/coding/v3` 与乱路径 `/zzz_bogus_path` **返回同码同体**
  （都 401 + `{"id":"...","error":{"type":"not_authorized_error",...}}`）
  ⇒ 网关在路由前统一拦，**路由存在性未能证实**（与 DeepSeek 的 governor 同型）。
- **配额端点：本轮用尽手段仍未找到**：
  - 全 GitHub 代码搜索 `lkeap.cloud.tencent.com` / `getCharacterUsage` / `tokenhub` 组合，
    命中的全是**推理接入**文档与配置（`openclaw`、`NousResearch/hermes-agent`、`cc-switch` 预设），
    **没有任何一家在做配额查询**。
  - 腾讯云官方 OpenAPI `lkeap.tencentcloudapi.com` 有
    `Action=GetCharacterUsage&Version=2024-05-22&Region=ap-guangzhou` → 返回 `Used` / `Total`
    —— 但那是**知识引擎「原子能力」的字符额度**，**不是** Coding Plan 的 5h/周/月请求数。
  - 套餐额度的可视化只在控制台 `https://hunyuan.cloud.tencent.com/#/app/subscription` 与其
    「用量统计」页。
- **下一步（二选一）**：
  (a) 按 `usage-api-integration-doc` 的方法扒控制台前端 bundle，定位用量请求；
  (b) 暂时不做，标注为「需真账号 + 逆向」。
  **本条不建议现在投入** —— 收益（补 1 家）与成本（逆向一个无开源参考的控制台）不成比例。

### 2.3 第 3 档 —— 需要额外凭据

| 厂商 | 取数形态 | 卡点 |
|---|---|---|
| 小米 MiMo Token Plan | 需平台 Cookie | 要用户手动提供 Cookie |
| 火山方舟 | `GET /v1/quotas`，额度在响应头 `X-RateLimit-Remaining-5H` / `-Week` / `-Month` | 官方明确**只支持 Access Key 签名，不能用 `ARK_API_KEY`** → 要向用户索取主账号永久 AK/SK，**安全红线** |
| 百度千帆 | `DescribeServiceMetric` | 同样 AK/SK |

第 3 档的共同问题：索取的是**主账号长期凭据**，不是可随时吊销的子 key。除非有用户的明确授权和
隔离方案，否则不建议做。

#### 2.3.1 小米 MiMo Token Plan（三家独立实现，凭据是 Cookie 对）

- 端点（同一 base `https://platform.xiaomimimo.com/api/v1`）：
  - `GET /balance` → 钱包余额
  - `GET /tokenPlan/detail` → 套餐信息
  - `GET /tokenPlan/usage` → 套餐用量
- 凭据：**两个 cookie 必须成对** —— `api-platform_serviceToken` + `userId`；
  只带到其中一个一律按「从未登录」处理。
- **【实测】**无凭据 → `HTTP 401` + `{"code":401,"loginUrl":"https://account.xiaomi.com/pass/serviceLogin?callback=..."}`；
  **乱路径同码同体** ⇒ 网关统一拦，**路由存在性未能证实**，
  但有 **4 家独立实现**交叉印证（见下），可信度足够。
- **【实现所述】**信封统一为 `{code, message, data}`，**`code` 为 401/403 表示登录态失效**（外层仍是 HTTP 200）。
  余额字段：`data.balance`（字符串）、`data.currency`、`data.cashBalance`、`data.giftBalance`；
  `tokenPlan/detail` → `planCode` 等；`tokenPlan/usage` → 月度用量 + items。
  两个 plan 调用是**补充性**的：任一失败仍应报出余额，只是没有窗口/套餐行。
- 参考实现（**全部独立**）：
  `btsouth/ceiling` `mimo/mod.rs`(Rust)、`zbndev/tidemark` `keyed/mimo.rs`、
  `steipete/CodexBar` `Providers/MiMo/MiMoUsageFetcher.swift`、
  `Cmochance/codex-app-transfer` `src-tauri/src/mimo_quota.rs`(Rust/Tauri)、
  `Javis603/token-monitor`、`slkiser/opencode-quota`、`Seaony/Yomi`。

#### 2.3.2 火山方舟（Doubao / Ark）—— ❌ 已放弃（2026-09-18）

**⚠️ 首轮文档只知道「响应头读额度」这一路，实际还有一条正式 OpenAPI。**

**A 路 · 探针式（不需要 AK/SK，但会真实消费 token）**
- `POST https://ark.cn-beijing.volces.com/api/coding/v3/chat/completions`
  发一个 1-token 的请求，从响应头读 `X-RateLimit-Remaining-5H` / `-Week` / `-Month` / `X-RateLimit-Reset-5H`。
- **【实测】**有凭据无凭据都是 401 `AuthenticationError`（乱路径同样 401）⇒ 网关统一拦，存在性未证实。
- 参考实现：`btsouth/ceiling` `doubao/mod.rs`（含探针模型列表
  `doubao-seed-2.0-code` / `doubao-1.5-pro-32k` / `doubao-lite-32k`）。
- 缺点：**每次刷新都会真的产生一次模型调用**，成本与副作用都不可忽略。

**B 路 · 正式 OpenAPI（需主账号 AK/SK）**
- `POST https://open.volcengineapi.com/?Action=GetCodingPlanUsage&Version=2024-01-01`
- 签名：Volcengine V4（HMAC-SHA256）。signed headers = `content-type;host;x-content-sha256;x-date`；
  请求头 `X-Date`、`X-Content-Sha256`、`Authorization`、`Host`。
  env 命名：`VOLCENGINE_ACCESS_KEY_ID` / `VOLCENGINE_SECRET_ACCESS_KEY` / region 默认 `cn-beijing`。
- **【实测】**`open.volcengineapi.com` 对**真假 Action 一律**返回
  `400` + `{"ResponseMetadata":{... "Error":{"CodeN":100025, ...}}}`（缺签名）⇒
  **Action 存在性靠探针证明不了**，靠下面两份实现交叉印证。
- **【实现所述】**响应：`{Result:{Status, UpdateTimestamp, QuotaUsage:[{Level, Percent, ResetTimestamp}]}}`
  - `Level` 取值含糊，实现里做**模糊匹配**：5h 认 `session` / `5-hour` / `five_hour`，
    周认 `weekly` / `week`，月认 `monthly` / `month`
  - `Percent` 是**已用百分比**（与 StepFun 的「剩余率」相反，注意别取反两次）
  - `ResetTimestamp` / `UpdateTimestamp` 是 epoch 秒，`<= 0` 视为无效
- 参考实现：`btsouth/ceiling` `doubao/mod.rs`、`diegosouzapw/OmniRoute`
  `open-sse/services/usage/volcenginePlan.ts`、`farion1231/cc-switch` `src-tauri/src/services/coding_plan.rs`、
  `nguyenphutrong/quotio` `catalog/doubao.rs`。
- **安全取舍不变**：B 路要的是**主账号永久 AK/SK**（权限远大于 API key，且官方明确不能用
  `ARK_API_KEY` 代替）。要接必须先和用户讲清这条，再谈隔离方案。

#### 2.3.3 百度千帆（社区零实现）—— ❌ 已放弃（2026-09-18）

- 方法 / 路径：`POST https://qianfan.baidubce.com/v2/service?Action=DescribeServiceMetric`
- **【实测】**无凭据 → `HTTP 400` + `{"message":"Signature is invalid: Signature is empty.","code":"SignatureEmpty"}`
  ；对照组乱路径 `/v2/zzz_bogus` → `404` + `ResourceNotFound`
  ⇒ **路由存在已证实**（签名校验先于路由，但乱路径确实被 404 挡掉）。
- 官方 SDK 常量（`baidubce/bce-qianfan-sdk` `python/qianfan/consts.py`）：
  `ServiceV2BaseRouteAPI = "/v2/service"`、`ServiceMetricAction = "DescribeServiceMetric"`
  ⇒ action 名确凿。
- 鉴权：百度云 IAM AK/SK 签名（`Authorization: bce-auth-v1/...`）→ **红线**。
- **社区零实现**：全 GitHub 代码搜索 `DescribeServiceMetric`，除官方 SDK 外无任何第三方配额客户端。
- 结论：**要做就得自己实现整套签名 + 索取主账号 AK/SK，投入产出比最差，建议排到最后。**

### 2.4 第 4 档 —— 通用兜底，性价比最高

| 目标 | 取数形态 | 备注 |
|---|---|---|
| NewAPI / one-api 系中转站 | `GET <base>/dashboard/billing/subscription` + `GET <base>/usage` | **一个适配器覆盖国内大量自建/第三方中转站**；只要用户填 base URL + key |

**本轮把这一档摸清了，而且发现它的字段细节比首轮记的更细。**

#### 2.4.1 NewAPI / one-api 系

两种模式（**【实现所述】**，取 `smallmain/vscode-unify-chat-provider` `src/balance/providers/newapi.ts`）：

| 模式 | 端点 | 响应关键字段 | 说明 |
|---|---|---|---|
| 仅 key（`api-key-only`） | `GET {base}/api/usage/token` | `unlimited_quota`, `quota`, `used_quota` | 只需一把中转站 key，UX 最好 |
| 带用户（`with-user`） | `GET {base}/api/user/self` | `quota`, `used_quota` | 能取到账号级余额 |

- **额度换算：`余额 = quota / 500000`**（NewAPI 默认 500000 配额 = $1）。
  实现里这个除数/乘数是**可配置**的（`quotaTransform`，默认 `quotaField='quota'`、`divisor=500000`、`multiplier=1`），
  **建议 Quota01 也做成可配置** —— 不同中转站改过这个比例。
- `unlimited_quota: true` 时不要画进度条。
- 老式 OpenAI 兼容路：`GET {base}/dashboard/billing/subscription`（含 `hard_limit_usd`）
  + `GET {base}/dashboard/billing/usage`（含 `total_usage`）。
  `hard_limit_usd == 1e8` 是「无限额度令牌」哨兵值，此时真实余额要去 `{origin}/api/user/self`
  的用户配额上取。**【实现所述】**（首轮记自同一系列，本轮未复测哨兵值）。
- 参考实现：`smallmain/vscode-unify-chat-provider`、`QuantumNous/new-api` `controller/channel-billing.go`、
  `MartialBE/one-hub` `router/dashboard.go`、`Veloera/Veloera`。

#### 2.4.2 Sub2API 系（本轮补上）

- `GET {base}/v1/usage?days=30`（`days` 窗口；客户端会带本机时区让服务端给 `usage.today` 分桶，
  无用户时区的守护进程场景按 UTC 处理即可）
- **无默认 host**，base URL 是**必填**的自由文本；HTTPS 强制，**明文 HTTP 只允许 loopback**。
- **【实现所述】**响应三类内容，**可能共存也可能互斥**：
  - 配额型 key：`quota:{limit, used, remaining, unit}`（`unit` 取根 → quota → USD 的优先序；
    `limit <= 0` 时不画窗口）
  - 订阅型：`daily_usage_usd` / `daily_limit_usd`、`weekly_*`、`monthly_*` → 三个窗口（1/7/30 天）。
    **线上没有任何字段说明它们何时重置**（订阅到期日是另一个日期），所以三个窗口都不该画 pace 标记。
    订阅与配额**不同时出现，订阅优先**。
  - 额外窗口：`rate_limits[]`，按 span 命名（`5h` / `1d` / `7d` 有已知长度与标题；
    陌生名字保留原字符串但不算长度）+ `pct` + `reset_at`。
    `pct` 的规则：limit <= 0 读作满格。
  - ⚠️ **订阅窗口与 `rate_limits` 可能报同一个 span**（周订阅窗口旁边再来一个 `7d`），
    那是两个额度不是同一个 —— 要按池分别编号（`subscription/w604800` 与 `rate/w604800`）。
- 参考实现：`zbndev/tidemark` `keyed/sub2api.rs`（注释极详细）、
  `smallmain/vscode-unify-chat-provider` `src/balance/providers/sub2api.ts`、
  `nguyenphutrong/quotio` `catalog/gateways.rs`。

> 同族的 **LiteLLM**（`{base}/key/info`）与 **LLM Proxy** 也都有现成实现
> （`quotio` / `tidemark` / `ceiling` 三家都有），做第 4 档时可以顺手一起做。

### 2.5 本次新发现的候选（原文档没有）

| 厂商 | 端点 | 实测 | 开源实现 |
|---|---|---|---|
| **无问芯穹 Infini-AI Coding Plan** ✅ **已落地（2026-09-18）** | `GET https://cloud.infini-ai.com/maas/coding/usage` | **401** `{"code":10021,"msg":"CodingPlan的api key不正确，请确认后再重试"}`；乱路径 `/maas/zzz_bogus` → **404** `404 page not found` ⇒ **路由存在已证实** | `btsouth/ceiling` `infini.rs`（Rust） |
| 京东云 JoyBuilder Coding Plan | 待查 | — | 待查 |
| 快手 KwaiKAT Coding Plan | 待查 | — | 待查 |
| 讯飞星火 Coding Plan | 待查 | — | 待查 |
| 优云智算 Coding Plan | 待查 | — | 待查 |

- **无问芯穹值得单独提一句**：这是本轮唯一一个**「一条 GET + 纯 API key + 探针可直接证实路由」**
  的新增国内 Coding Plan 厂商，接入成本与第 1 档同级。
  **【实现所述】**响应 `InfiniUsage` 的形状是三段固定周期：
  `{"5_hour":{quota,used,remain}, "7_day":{...}, "30_day":{...}}`，
  base 可覆盖（默认 `https://cloud.infini-ai.com`）。
- **✅ 落地实现（2026-09-18）**：provider id `infini`，代码在 `src-tauri/src/providers/infini/`，
  用户文档见 [docs/providers/infini.md](providers/infini.md)。落地时的取舍：
  1. 三个桶映射为 `Session (5h)` / `Weekly` / `Monthly` 三个 quota，格式 `Count`、
     单位 `requests`（复用既有 i18n 键，不新造词）。
  2. **`code == 10021` 在任何 HTTP 状态下都判为无效 key** —— 实测该码与 401 同时出现，
     但业务码才是真凭据，不依赖传输状态码。
  3. **不推算重置时间**：响应里没有时间戳，`resets_at` 一律留空，只给窗口长度，
     避免把「每周一 00:00」这类文档口径硬编码成可能错的倒计时。
  4. **不猜套餐档**：`5_hour.quota` 1000 / 5000 对应 Lite / Pro 是参考实现的推断，
     两份来源的示例数据自相矛盾（见 `AIddlx/coding-plans` 的响应示例），
     所以卡片只显示 `Coding Plan`，不宣称 Lite/Pro。
- 后四家来自 `AIddlx/coding-plans`（国内 Coding Plan 资料库，含火山/千帆/腾讯/阿里/阶跃等记录），
  本轮**未做端点侦察**，列为下一轮待办。

## 三、已证伪（别照抄）

| 路由 | 实测 | 说明 |
|---|---|---|
| `dashscope.aliyuncs.com/compatible-mode/v1/account/usage` | **404** | 网传的百炼用量路由，实际不存在 |
| `api.kimi.ai/v1/users/me/balance` | **404** | 余额只有 `.cn` 域有 |
| **`coding.dashscope.aliyuncs.com/v1`** | **404** | **本轮新增证伪**。首轮把它记为「阿里 Coding Plan 的 Anthropic 兼容基址」，实测 `POST /v1`、`POST /v1/messages` **全 404**（无对照组意义，因为该域任何路径都 404）。阿里 Coding Plan 的真实取数见 2.1.3。 |
| `api.stepfun.com/v1/user/balance` | **404** | 本轮新增。StepFun 钱包余额在 `/v1/accounts`，不是 `/user/balance`。 |

### 已放弃档（ling 2026-09-18 拍板，不再收录）

| 候选 | 放弃理由 |
|---|---|
| **火山方舟**（见 2.3.2） | 两条路都不可接受：A 路每次刷新**真的产生一次模型调用**（成本 + 副作用）；B 路要**主账号永久 AK/SK**（安全红线）。 |
| **百度千帆**（见 2.3.3） | 同为主账号 AK/SK 红线，且**社区零实现**，还要自己实现整套 BCE 签名 —— 投入产出比最差。 |
| **腾讯云 Coding Plan**（见 2.2） | 配额端点**社区零实现**、探针也证不出路由，只能逆向一个无参考的控制台。 |

> 这三条**不进入实现队列**，章节保留仅供日后复查。若哪天厂商补了「子 key 可读的用量接口」
> （即不再需要 AK/SK 或手工 Cookie），可以按 §附录 B 的四步重新评估。

## 四、接候选时必须先处理的坑

1. **HTTP 200 + 业务错误码**。`zai` / `minimax` 两家对**无效 key** 返回的是
   `HTTP 200` + `{"code":401,"success":false}`（智谱）/ `base_resp.status_code:1004`（MiniMax），
   **不是** HTTP 401。现有 `map_quota` 找不到 `data.limits` 就报
   "usage data is temporarily unavailable"，即**把鉴权失败误报成服务不可用**。
   新增国内 provider 前应先确认其错误码形态，别默认 4xx 就等于鉴权失败。
   **本轮新增三个同型实例**：
   - 阿里 Coding Plan：`HTTP 200` + `{"code":"ConsoleNeedLogin","successResponse":false}`
   - 小米 MiMo：`HTTP 200`（或 401）+ `{"code":401,"loginUrl":"https://account.xiaomi.com/..."}`
   - 硅基流动：`HTTP 200` + `code != 20000` 或 `status != true`
2. **GET / POST 要按客户端实现来探**。MiniMax 的 `/v1/token_plan/remains` 用 POST 探会 404，
   客户端走的是 GET —— 用错方法会把存在的端点判成不存在。
3. **同厂双站可能只有文案之别**。`api.z.ai` 与 `open.bigmodel.cn` 在相同路径上返回同构响应，
   仅 msg 中/英不同；`www.minimax.io` / `www.minimaxi.com` / `api.minimaxi.com` 的
   `GET /v1/token_plan/remains` 响应**逐字节相同**。所以双站通常只需换域名。
   **本轮新增一对**：`api.siliconflow.cn` 与 `api.siliconflow.com` 的 `/v1/user/info`
   返回同一份错误体 ⇒ 同构。
4. **【本轮新增·方法论】必须加「乱路径对照组」**。只打目标路径拿 401 **不能证明端点存在** ——
   很多网关（腾讯 lkeap、小米 MiMo、火山 Ark、火山的 `open.volcengineapi.com`）
   在路由匹配**之前**就统一拦截，任何路径都返回同一个 401/400。
   判法：**目标路径与一条乱路径同码同体 ⇒ 存在性未证实**；目标 401、乱路径 404 ⇒ 存在已证实。
   本轮据此把 `SiliconFlow` / `StepFun(api)` / `DashScope` / `千帆` / `Infini` 判为**已证实**，
   把 `腾讯 lkeap` / `小米 MiMo` / `火山 Ark` / `火山 OpenAPI Action` 判为**未证实**。
5. **【本轮新增】「Coding Plan 订阅」和「按量计费 API」是两套服务，端点、Base URL、key 都不通用**。
   首轮文档已经点出这条，本轮把它坐实了：
   - StepFun：`api.stepfun.com/v1/accounts`（钱包）≠ `platform.stepfun.com/.../QueryStepPlanRateLimit`（套餐）
   - 阿里：`dashscope.aliyuncs.com/api/v1/quotas`（按量）≠ `bailian.console.aliyun.com/data/api.json`（Coding Plan）
   - 腾讯：`sk-xxxx` + 按量 Base ≠ `sk-sp-xxxx` + `/coding/v3`
   **用订阅 key 打按量 Base = 直接按 token 扣钱（用户被坑钱的典型）。**
6. **【本轮新增】别把「剩余率」和「已用百分比」搞反**。
   StepFun 是 `*_usage_left_rate`（**剩余**），火山 `QuotaUsage[].Percent` 是**已用**。
   取反两次就会画出一条永远 100% 的假进度条。

## 五、落地优先级与状态（2026-09-18 定稿）

**状态列是本轮拍板后的结果**：P0 两家已落地，火山/千帆/腾讯已放弃，其余按原优先级排队。

| 优先级 | 目标 | 状态 | 理由 |
|---|---|---|---|
| **P0** | **SiliconFlow**（2.1.2） | ✅ **已落地** | 纯 API key + 一个 GET + 单/双站同构 + 已有 Rust 实现可抄 + 探针已证实路由 |
| **P0** | **无问芯穹 Infini-AI**（2.5） | ✅ **已落地** | 同样是纯 API key + 一个 GET，且探针已证实路由；补一家国内 Coding Plan |
| **P1** | **NewAPI / one-api 系**（2.4.1） | 待做 | 一个适配器覆盖一大类中转站，用户 base URL 自填，零厂商适配成本 |
| **P1** | **StepFun 余额**（2.1.1 A 路） | 待做 | 成本同级，且能顺带补 `StepFun` 这家国内厂商 |
| **P1** | **Sub2API / LiteLLM / LLM Proxy**（2.4.2） | 待做 | 与 NewAPI 同批做，边际成本低 |
| **P2** | **阿里云百炼 Qwen Coding Plan**（2.1.3） | 待做 | 能补最大一块市场空白，但要处理区域 + 三 header + 嵌套 commodityCode，且 action 存在性只能靠开源实现背书 |
| **P2** | **小米 MiMo**（2.3.1） | 待做 | 实现有 4 家可抄，但凭据是**用户手动导 Cookie**，体验差；Cookie 过期要引导重取 |
| **P2** | **StepFun Step Plan 订阅**（2.1.1 B 路） | 待做 | 有完整参考，但要用户粘贴 `Oasis-Token`，且两版实现常量不一致需校准 |
| ~~P3~~ | ~~火山方舟~~（2.3.2） | ❌ **已放弃** | 安全红线（主账号永久 AK/SK）；探针式那路每次刷新真花钱 |
| ~~P4~~ | ~~百度千帆~~（2.3.3） | ❌ **已放弃** | 红线 + 社区零实现 + 需要自己写整套签名，投入产出比最差 |
| ~~P4~~ | ~~腾讯云 Coding Plan~~（2.2） | ❌ **已放弃** | 社区零实现，要逆向无参考的控制台 |

**实现清单（P0 落地时实际动过的文件）**，供下一个候选 provider 照抄：

- `src-tauri/src/providers/siliconflow/`（`mod.rs` / `client.rs` / `auth.rs` / `mapper.rs`，
  双站 `Site` enum）、`src-tauri/src/providers/infini/`（同结构，单站）
- 注册链：`providers/mod.rs` → `providers/registry.rs` 测试 → `lib.rs` `providers.extend(...)`
  → `scripts/verify/verify-provider-registry-contract.js` 的 `expectedRuntimeOrder`
  （**19 个 runtime**，且该脚本的 `providerLiteral` 反例清单也加了新 id）
- 图标 4 处：`providers/provider_icons.rs`、`menu_bar.rs`（常量 + `OnceLock` + match + 测试列表）、
  `src/lib/providerIconPaths.ts`（`visuals` / `colorAssetSlugs` / `colorAssets`）、
  `src/lib/visual-parity.test.ts`（两个 `it.each`）。
  新增两个品牌图标源文件：`src/assets/provider-icons/{siliconflow,infini}.svg`
  （取自 LobeHub 的 `siliconcloud` / `infinigence`，即两家厂商的英文品牌名）
- i18n：`backendGlossary.ts` + **16 个语言包**，新增 6 个键
  （`metric.grantedBalance` / `metric.rechargedBalance` / `plan.codingPlan` /
  `providerError.addSiliconFlowApiKey` / `addSiliconFlowCnApiKey` / `addInfiniApiKey`）
- 其他：`.github/ISSUE_TEMPLATE/bug_report.yml` 下拉、`README.md` 分组名单、
  `docs/providers/{siliconflow,siliconflow-cn,infini}.md`

## 附录 A：已收录 provider 的增强项（按要求本次未展开）

留档，不展开：

- `GET https://api.moonshot.cn/v1/users/me/balance`（Kimi 开放平台余额，与 `kimi-cn` 的 Code 订阅是
  两条产品线）—— **【实测】**401 `Invalid Authentication`。
- `GET https://api.deepseek.com/user/balance`（DeepSeek 余额）
  —— **【实测】**401 `authentication_error`；**【实现所述】**响应
  `{balance_infos:[{currency,total_balance,granted_balance,topped_up_balance}], is_available}`。
  接上的意义是把 `deepseek` 从 WebView 登录**降级**为纯 API key，**少一个 Keychain 弹窗来源**
  （ling 明确嫌弹窗烦）。
- 两条都属于**改动已收录 provider**，按 ling「已收录的不用管」的指示本轮不研究。

## 附录 B：调研方法

判断“某厂商能否接入”按以下四步，都不需要凭据：

1. **先查开源**（**本轮新增，应作为第 0 步**）—— 按 2.0.1 的表查
   `CodexBar` / `tidemark` / `ceiling` / `quotio` / `cc-switch-cli` / `vscode-unify-chat-provider`。
   用**端点字符串反查代码**最高效：`gh search code '"dashscope.aliyuncs.com/api/v1/quotas"'`。
   命中即说明有人实现过，直接读它的常量区与解析函数，比逆向快一个数量级。
   只有当全部仓库都查不到（本轮：腾讯云、千帆）才值得自己逆向。
2. **无凭据探针 + 乱路径对照组** —— `401/403` 只说明「不是 404」，还要和一条乱路径对比：
   同码同体 ⇒ 网关统一拦、存在性未证实；目标 401 而乱路径 404 ⇒ 存在已证实。
   注意区分 HTTP 状态码与业务码（见第四节第 1 条）。
3. **国际站 / 国内站同路径对照** —— 同一路径分别打两个域名，响应同构即可判定“只换域名”。
4. **凭据形态分档** —— 纯子 key（可做）→ 额外 Cookie（可做但体验差）→ 主账号 AK/SK（安全红线）。

接入落地流程见 `~/.workbuddy/skills/quota01-provider-change`，其中包含改名/新增时要同步的全部消费方
（`lib.rs` 注册顺序、契约脚本 `expectedRuntimeOrder`、图标映射 4 处、`RENAMED_PROVIDER_IDS` 迁移链、
i18n 16 个语言包、issue 模板、README）。

## 附录 C：参考实现索引

| 参考 | 路径 | 可抄什么 |
|---|---|---|
| `zbndev/tidemark` | `crates/tidemark-core/src/providers/keyed/` | **首选模板**。`stepfun.rs` / `sub2api.rs` / `mimo.rs` / `alibaba.rs` 的 `//!` 注释即协议文档；刻意不自动刷新凭据 |
| `btsouth/ceiling` | `rust/src/providers/{stepfun,doubao,mimo,alibaba,infini}/` | CodexBar 的 Rust 移植；火山 V4 签名的完整实现 |
| `nguyenphutrong/quotio` | `apps/cli/src/providers/catalog/coding.rs` | **阿里 Coding Plan** 的 `AlibabaRegion` 三元组（base / regionId / commodityCode） |
| `SaladDay/cc-switch-cli` | `src-tauri/src/services/balance.rs` | StepFun / SiliconFlow / DeepSeek 余额的 Rust 实现，字段名与兜底逻辑齐全 |
| `smallmain/vscode-unify-chat-provider` | `src/balance/providers/{newapi,siliconflow,sub2api}.ts` | NewAPI 的 `/500000` 换算与 `quotaTransform` 可配置设计 |
| `steipete/CodexBar` | `Sources/CodexBarCore/Providers/` | 91 个 provider 的覆盖清单，用来发现「还有谁已经被做过」 |
| `farion1231/cc-switch` | `src-tauri/src/services/coding_plan.rs` | 火山 `GetCodingPlanUsage` 的 Tauri 侧调用 |
| `AIddlx/coding-plans` | 仓库根 `*.md` | 国内 Coding Plan 套餐资料库（含京东云/快手/讯飞/优云智算等未侦察厂商） |
