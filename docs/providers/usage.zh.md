# `/usage` —— 你的订阅还剩多少

日期：2026-10-25

## 目标

一个 **`/usage`** 斜杠命令，回答订阅一直在提的那个问题：*我的套餐用了多少，什么
时候恢复？* 页脚的量表说明上下文窗口有多满（`docs/ui/footer.md`），`/context`
说明是什么填满了它（`docs/architecture/context-usage.md`）；两者都只字不提账户。
一个 ChatGPT 席位有一个周窗口（某些套餐上还有一个 5 小时窗口），一个 GitHub
Copilot 席位有一份月度额度，ClinePass 有三个滚动窗口——而在此之前，用户第一次
听说其中任何一个，都是一个被拒绝的回合。

这个页面显示应用登录着的每一个**订阅**——即 `/login` 在 *Sign in with an
account* 下所列的那些（`docs/providers/llm.md`）——每个都带它的套餐、每个限额
一个量表、每个限额何时重置，以及 provider 说现在正在阻止请求的任何东西。一个
粘贴的 API key 按 token 计费，没有套餐可衡量，所以 API-key provider 不在这个页
上。一个订阅都没登录时没有东西可显示，`/usage` 就不打开页面：一个 toast 说明
缘由并指向 `/login` → *Sign in with an account*——`/export` 的空规则。

## 这一页

```
────────────────────────────────────────────────────────────────────────────────────────────────────
  Usage limits
  3 subscriptions · updated just now

  ❯ ChatGPT Codex  Pro · dev@example.com · in use
      Weekly limit   █░░░░░░░░░░░░░░░░░░░   7% used  resets in 6d · Sat 3:58 PM
      Credits        62,500 credits · ≈ 15.6k–81.3k local messages
      Limit resets   2 available

    ClinePass  Monthly
      5-hour limit   ░░░░░░░░░░░░░░░░░░░░   0% used
      Weekly limit   ████████████████████ 100% used  limit reached · resets in 1d 23h · Tue 2:36 PM
      Monthly limit  ████████████████░░░░  81% used  resets in 17d · Nov 12, 1:26 PM
      ⚠ Paused until the weekly limit resets — switch with /model meanwhile.

    GitHub Copilot  Pro
      AI credits     ████████░░░░░░░░░░░░  42% used  420 of 1,000 credits · resets in 6d 8h
      Chat           Unlimited

  Details at https://chatgpt.com/settings/usage
  ↑↓ select  r refresh  c copy link  esc close
────────────────────────────────────────────────────────────────────────────────────────────────────
```

它是一个替换 composer 的内联页面——`/context` 和 `/donate` 的家族：同样的规则、
两列的内缩、被高亮小节上的 `❯`、各页那套标题装扮、没有文本输入因而没有硬件
光标（它的座位跟着 `❯` 走）、Esc 之后 composer 回来。它**回合中途**可用：流式
条带保留它在它上面的行，于是一个长回合可以被看着吃掉一个窗口。

为什么是一页而不是一条 scrollback 报告：数字在动。这一页是活的——它打开期间每
分钟重读每个订阅（见下文*实时*）——而且它在对话里什么都不留。

副标题统计订阅数并说明它们的数字有多新：`updated just now`、`updated 3m ago`、
`2h ago`、`3d ago`——相对的，像应用显示的每一个别的年龄，因为一个时钟时间把减法
留给读者。在一次读取在飞时它加上 `loading…`（第一轮，还在落地）或
`refreshing…`（压在早先一轮留下的数字之上）。

### 每个订阅一节

正在用的订阅排在最前；其余按 `/login` 的顺序。一节的头是该 provider 的名字
（粗体；被高亮小节上是强调色）、套餐（`Pro`、`Monthly`），以及——dim 的——当
回答点名一个账户时的那个账户；而在正在用的订阅上，`in use` 用成功绿，是那条
` · ` 链的最后一环。它坐在它所描述的东西旁边，绝不被推到页的边缘、隔着一片空列
——那在一个宽终端上读起来像一个走失的词，而不是头的一部分。一个长过它那一行的
头先放弃账户，再放弃标签。它的行缩进在名字之下，于是每一节读起来是一个块。

每个**量表**一行，label 在全页对齐的一列里：

| 量表 | 画成 |
| --- | --- |
| 一个限额 | 一条**已用**份额的条、份额（`42% used`）、provider 说用尽时红色的 `limit reached`、provider 会计数时其后的计数与单位（`420 of 1,000 credits`），以及它何时重置 |
| 无限 | `Unlimited`，用成功绿 |
| 一个余额 | 它的值亮着，后面跟一句 dim 的细节（`62,500 credits · ≈ 15.6k–81.3k local messages`） |

每个限额都排在每个余额之前——套餐的各个窗口，然后是按模型的和代码评审的限额，
然后是信用余额和账户可兑换的限额重置——于是这些条构成一列。

条随着限额被花掉而填充——绿色，75% 起琥珀，90% 起红色（`/context` 量表的阈值），
而一旦 provider 说限额已达，无论份额多少都是红色——旁边的份额一旦告警就取条的
颜色。份额是整数百分比，绝不声称并非如此的 0 或 100：一小条是 `<1%`，差一点到
满是 `>99%`，只有 provider 这么说时才是 `100%`（Copilot 和 ClinePass 的回答可能
超过它；条停在满）。条也守同样的承诺：任何使用至少填一格，差一点到满的份额至少
留一格空。

**整页一个条长**，好让量表那一列对齐——只由终端宽度决定，绝不由条旁边的文字
决定：有空间放它和它旁边的一个倒计时时是 20 格（codex `/status` 所画的段数），
只有在一个两者都放不下的窄终端上才更短，且绝不低于 8。条是一行上唯一保住尺寸的
东西，旁边的文字改为让位：20 格时条以 5% 为步进，于是 81% 和 86% 看起来不同；
一个长行只花掉它自己（上面 Copilot 那行放弃了它重置的钟点，而别的行一格都没丢）；
而条在倒计时跳动时保持长度。把每条都按最宽那一行的文字定尺寸，是第一版的做法，
它在 100 列上把 81% 和 86% 画成一样。

**已用，不是剩余。** codex 的 `/status` 条显示*还剩*多少；GitHub 的、Cline 的和
Claude Code 自己的用量页显示*已用*多少。一个叫 *Usage limits* 的页面只读成一种，
而随一周过去而填充的条，是不需要图例的那一种。

**何时重置**既是相对也是绝对的：`resets in 2h 14m` 说还要等多久，
`· Tue 2:36 PM`（本地时间——今天的 `3:58 PM`，一周之内的某个星期几，
更远的 `Nov 12, 1:26 PM`）说什么时候回来。一个窄行先放弃钟点；然后计数挪到条的
下面自己一行，在条的那一列；而条旁边完全没有空间时，整条尾随挪到它下面。一个
限额是否用尽、用了多少、还有多久重置，永远不会因窄终端而丢失。一个不点名重置的
窗口（ClinePass 的 5 小时窗口在开始之前）什么都不显示。

一个按模型的限额（ChatGPT 的 `additional_rate_limits`、它的代码评审限额）坐在
一个点名模型家族的 dim 小标题之下、套餐自己那些行之后——codex 的顺序——于是一个
长的模型名永远不必被截来适应 label 列。

**注记**跟在量表之后：量表说不出的东西。红色用于某件正在阻止请求、而没有一行
解释它的事（一个工作区的支出上限、一个没有已用窗口的 `limit_reached`、一个用完
的 Copilot 组织预算），琥珀用于某件要动手的事（ClinePass 的 *Paused until the
weekly limit resets — switch with /model meanwhile.*、一个没有 ClinePass 套餐的
Cline 账户），dim 用于一个事实（越过 Copilot 额度的用量计费；Console 登录的按
token 计费）。一个已用尽的窗口在它自己那行说 `limit reached`，所以它不需要第二
句红字。

### 一个链接

在提示之上，**被高亮订阅的**用量页，作为可点击的链接（`docs/ui/links.md`）：
`Details at https://chatgpt.com/settings/usage`。一个链接而不是每节一个，因为它
跟着高亮走——用户移到的那一行就是他们想要的那一页——而它否则要花掉的那些行，是
一个矮终端最需要的行。`c` 复制它，走 `/copy` 的剪贴板路径
（`Copied the usage link to clipboard`），页面保持打开；提示只在有链接可复制时
点名 `c`。

### 状态

| 情况 | 该节显示 |
| --- | --- |
| 首次读取在飞 | `Loading usage…`，dim |
| 读取失败，还什么都没显示 | 红字的错误，`Couldn't load usage: …` |
| 一次刷新失败 | 上一份报告，以及它下面一行琥珀的 `Couldn't refresh: …` |
| 一次刷新在飞 | 上一份报告；副标题说 `refreshing…` |

一个最终没有任何订阅的页面——它打开期间每一个都登出了——说明这一点并指向
`/login` → *Sign in with an account*，用 `/model` 对同样建议所用的那个强调色。

## 按键

| 键 | 作用 |
| --- | --- |
| ↑ / ↓ | 在订阅之间移动（回绕） |
| Home / End | 第一个 / 最后一个订阅 |
| `r` | 立刻重读每个订阅（在一次读取在飞时忽略） |
| `c` | 复制被高亮订阅的用量页链接 |
| Ctrl+O、Ctrl+D | 压在页面上的 transcript 和上下文视图，如同 `/context` 放行的；回来的路上页面还在 |
| Esc、Ctrl+C | 关闭页面 |

页面打开时其他每个键都被吞掉——`/donate` 的规则——而从页面打开的 overlay 永不
武装 Esc-Esc 回退（`App::overlay_esc_backtracks`）。高亮在一个矮终端上要紧
（见下文），那里它决定哪一节保持展开。

## 一个太矮的终端

家族的规则（`docs/ui/view-flow.md` 的 *A page that sheds rows before it
flows*），第一个装得下的页面：

1. 整页；
2. 去掉链接；
3. 一个**手风琴**：被高亮的那节展开，其他每一节折成它的头行，附上它最满那个
   限额的摘要（`GitHub Copilot  Free · Chat 86% used`）——`/donate` 的手风琴；
4. 那一页把标题和副标题折成**一行**（`Usage limits · updated just now`）——一页
   永不丢它的名字；
5. 紧凑页：那一行标题，然后每个订阅一行折叠行，在 `↑`/`↓` 槽位之间围绕高亮
   开窗，然后是提示——家族的紧凑梯级（`model_view::compact_page`），一路甩到只剩
   标题行，所以页面永不流出去。

## 实时

页面在打开时取一次，并在它打开且没有正在读取时**每分钟**重读——codex 轮询同一个
ChatGPT 端点的节奏——或者用 `r` 立刻重读。一次刷新把每份报告留在屏上直到它的
替换落地，并把页面铺在当前登录着的订阅之上：一个在此期间登录的会被加上，
`Loading usage…`；一个登出的被丢掉。关闭页面取消在飞的一切；一个给已关闭页面的
结果，或给被更新一轮取代的那一轮的结果，被丢弃（`App::finish_usage_fetch` 给每个
结果按它的轮编号）。

**一个关闭的页面留着它最后那些报告**（`App::usage_kept`）：再 `/usage` 立刻显示
它们，`refreshing…`，而不是打开在一列 `Loading usage…` 行上、等重读它们的那一
秒。高亮重新打开在第一节，即正在用的那个。

相对时间在走：边界在每次绘制时把墙钟注入打开的页面（`App::set_usage_now`），并
在页面在屏期间排一个慢帧（每 5 s）——绝不在 overlay 之下，它的返回会重新播种——
于是一个倒计时和新鲜度不靠任何别的事件就移动。绝对时间是本地的：边界一次性地
交给应用一个函数，给出某一时刻的本地 UTC 偏移（`App::set_local_offset`），于是
一次横跨夏令时变化的重置仍读对钟点——`App::set_clock` 的模式，而页面本身从不读
时钟或时区。

## 每个数字从哪来

每次读取都跑在它自己的 worker 线程上（`tui::workers::spawn_usage_fetch`
→ `llm::usage::fetch_usage`），每个订阅一个，于是一个慢 provider 永远不拖住
其余。每一次都复用它的 chat 请求所用的凭据接缝（`llm::auth::request_auth`）：
同样铸造并缓存的已存 token，同样写回 `.env` 存储的轮换 refresh token
（`docs/providers/chatgpt.md`、`docs/providers/cline.md`）。每节的链接是
provider 自己的用量页（`llm::usage::dashboard_url`，按登录方案）。

### ChatGPT

`GET https://chatgpt.com/backend-api/wham/usage` —— Codex 自己的读法
（`backend-client`），落在 backend 的根上而不是 chat base 的 `/codex` 之下
（`llm::usage::chatgpt_usage_url`；一个不是 ChatGPT backend 的 base 在
`/api/codex/usage` 上被问，codex 的另一种路径风格）。请求带铸出的 bearer、
`chatgpt-account-id`、Codex 的 user agent 和 `originator`——chat 请求自己的
身份。回答点名套餐、邮箱、一个带至多两个窗口（`used_percent`、
`limit_window_seconds`、`reset_at`）的 `rate_limit`、按模型家族的
`additional_rate_limits`、一个 `code_review_rate_limit`、`credits` 余额（一个
字符串——`"62500.0000000000"`——带一个 `approx_local_messages` 范围）、一个
工作区的 `spend_control`（它的月度信用限额，一个自己的量表）、请求被停时的
`rate_limit_reached_type`，以及 `rate_limit_reset_credits.available_count`，即
账户可兑换的限额重置数。一个窗口按它长度的 5% 容差命名（`5-hour`、`Daily`、
`Weekly`、`Monthly`、`Annual`——codex 的容差），否则按长度本身
（`2-hour limit`），并在已用 100% 时算达到。一个工作区停下的理由按谁能解除它来
措辞（codex 的句子）；一句光秃秃的 `limit_reached` 只在页上没有任何窗口已经说了
这一点时才措辞。在一个 Pro 席位上实测过。

### GitHub Copilot

`GET https://api.github.com/copilot_internal/user` —— VS Code 自己的 Copilot
仪表盘所读的东西——用它存下的 GitHub token 本身
（`Authorization: token ghu_…`），**不是** chat 请求把它换来的那个 Copilot
bearer，外加 `X-GitHub-Api-Version: 2025-04-01` 和交换所发的编辑器身份
（`copilot::user_headers`）。

套餐先从 `access_type_sku` 来：`free_limited_copilot` 就是 **Free**，不管
`copilot_plan` 说什么——一个 Free 席位在那里报 `individual`，即 Pro 自己的
slug，只有 SKU 能把两者区分开（VS Code 的规则）。否则由 `copilot_plan` 命名它
（`individual` → Pro、`individual_pro` → Pro+、`individual_max` → Max、
`business` → Business 等）。

`quota_snapshots` 每个特性一个桶：`premium_interactions`（在一个按 GitHub 的
AI Credits 计费的席位上——`token_based_billing`——是 **AI credits**，否则是
**Premium requests**）、`chat`（**Chat**）和 `completions`
（**Code completions**），在一个按信用计费的席位上按信用计，否则按请求、消息和
补全计。一个桶的份额是它自己的 `percent_remaining`，正如 VS Code 所读；计数是
`entitlement` 减去剩下的（`quota_remaining`，否则 `remaining`）；每个数字都可能
以数字、数字字符串或 `null` 到达。一个桶在什么都不剩时**达到**——它的份额、它的
余量或 `has_quota: false` 这么说——除非该席位可以越过它（`overage_permitted`），
而越过额度的用量计费是一句 dim 注记（`12 credits billed past the allowance.`）。
一个 `unlimited` 的桶这么说，除非 `has_quota: false` 标出组织预算已用尽——一句
红注记，一次。一个并非无限的零额度是占位符（一个池化的 Business 席位），被略去。
所有东西在月底 1 日的 UTC 午夜重置（`quota_reset_at`，否则
`quota_reset_date_utc`，否则 `quota_reset_date`）。一个在旧 Free 套餐上的席位
改为对着 `monthly_quotas` 报 `limited_user_quotas`。`ZEROTH_GITHUB_API_BASE`
把这次读取——以及 token 交换——指向另一个 host
（`copilot::set_github_api`，在边界处读），smoke 套件就是这样替身 GitHub 的。

### ClinePass

两次读取，按 Cline 仪表盘自己的顺序：`GET /api/v1/users/me/plan`（套餐，由它的
计费 `interval` 命名——`Monthly`——否则由它的 `displayName`；`402`、`404` 或一个
`null` 套餐意味着该账户没有 ClinePass 订阅，而该节照实说，而不是去读窗口），然后
是 `GET
/api/v1/users/me/plan/usage-limits` —— `limits[]`，元素为
`{type, percentUsed,
resetsAt?}`，针对 `five_hour`、`weekly` 和 `monthly` 三个窗口。两者都带铸出的
`Bearer workos:<jwt>`，并去往生效中的 API host（`ZEROTH_CLINE_API_BASE`，
`docs/providers/cline.md`）。一个用尽的窗口在它自己那行达到，而一行琥珀说该怎么
办，点名**最后**重置的那个窗口——即暂停持续多久——仪表盘的横幅。实测过。

### Google Antigravity

一次读取，即目录那次：`POST {base}/v1internal:fetchAvailableModels`，取该账户
的 Cloud Code 项目，经与 chat 请求同一个接缝铸造并找到
（`docs/providers/antigravity.md`）。每个模型的记录带一个 `quotaInfo`——
`remainingFraction` 和 `resetTime`——于是该节是每个 `/model` 行一条条（一个行的
最稀缺变体：已用份额是 `1 -
remainingFraction`，而一个没有 fraction 的 `quotaInfo` 是用尽），分组在 Gemini
之下，然后是其他家族，外加一行 dim 说明配额是一个家族共享的池。套餐是
`loadCodeAssist` 的 `paidTier` 名，否则是它的 `currentTier`（即使在一个付费席位
上也读作 `free-tier`），账户是 `userinfo` 的邮箱。Google 不发布配额页，所以该节
不链任何页。`smoke.sh` Phase 148 对着一个 stub 读它。

### Console 登录

一个 Console 登录按 token 向该账户的 API 组织计费——没有套餐也没有窗口
（`docs/providers/claude.md` 说明了为什么这刻意不是一个订阅登录）。该节**谁都不
问**：它说 `Pay as you go`、该组织按 token 计费且没有套餐限额，并链接 Console 的
用量页。为一个页面并不需要的事实而铸造一个 token 去点名组织，只会轮换登录的
refresh token。

### 失败

一次被拒的读取按该做什么来措辞（`llm::usage::usage_advice`）：`401` 要求
`/login`，`429` 等一分钟，服务器错误稍后再试——当 provider 自己的话可以展示时
（一个 JSON `message`，绝不是拦截代理的 HTML 页）跟在后面。一个不再能铸造的
登录，以登录模块自己的句子失败。

## 接缝

| 部件 | 位置 | 种类 |
| --- | --- | --- |
| 报告模型、每个解析、建议、链接 | `src/llm/usage.rs` | 纯，有单元测试 |
| 各次读取 | `llm::usage::fetch_usage` | 边界（HTTP），stub 测试 |
| 页面状态、按键、轮次、留存的报告 | `src/app/usage.rs` | 纯，有单元测试 |
| 页面 | `src/ui/usage_view.rs` | 纯，有单元测试 |
| worker、时钟、每分钟刷新、剪贴板 | `src/tui/usage.rs` | 边界，smoke Phase 145 |

每个解析都宽松地读 provider 的回答：它的每一节都按到达时的原始切片保留、各自读，
于是一个这个解析不再理解的节只花掉它自己——这些是没有文档的端点，它们不打招呼
就变。什么都不解析进一棵 `serde_json::Value` 树
（`docs/architecture/memory.md`）。

## 测试

- `llm::usage::tests` —— 每个 provider 的回答（真实的形状，替换掉标识符）、
  窗口的 label、已达的限额和解释它们的注记、Copilot 的 SKU 与单位、宽松度、
  URL 与链接、建议，以及对着 loopback stub 的各次读取。
- `app::tests::usage` —— 命令、空 toast、按键、轮次与陈旧结果、跨打开留存的
  报告、`c`、各 overlay。
- `ui::tests::usage_view` —— 各节、量表、`limit reached`、重置的措辞和一个窄行
  放弃什么、那一个链接、新鲜度、边框在每个宽度和高度都贴着内容、梯级。
- `tests/live_usage.rs` —— 每个 provider 对着它真实的端点，`#[ignore]` 的
  （它需要凭据；会轮换的那些写回 `ZEROTH_LIVE_TOKEN_STORE`）。
- `scripts/smoke/phases/145-usage.sh` —— 对着 ChatGPT、Copilot（一个按信用计费
  的 Free 席位）和 ClinePass 的 stub 端点跑这个页面：各节、红字的
  `limit reached`、`r`、一次被拒的刷新、↓ 移动链接、`c`、压在页面上的 Ctrl+O、
  一次重开显示最后的数字、一个矮终端、回合中途的页面、没登录任何东西时的 toast。
