# 浏览器登录页

每一次浏览器登录都经由同一个 loopback 回调回来（`llm::loopback`，它的服务器在
`docs/tools/mcp.md` 里描述）：MCP server 的 OAuth 流程、ChatGPT 的
（`docs/providers/chatgpt.md`）、Anthropic Console 的
（`docs/providers/claude.md`）以及 Google Antigravity 的
（`docs/providers/antigravity.md`）。浏览器落到的那个页面，是用户在回到终端之前
对一次登录所见的最后一样东西。它过去是一个光秃秃的 `<h3>` 加一句话，白底黑字。
现在它是项目自己打扮的一个页面，由 `src/llm/loopback/page.rs` 构建。

```
                          ▙▄▙▄▟▄▟                ← the mascot, glowing
                         ▝▜▄███▄▛▘
                           ▘▘ ▝▝

                  Authorization received         ← serif headline
     You can close this tab and return to Zeroth.
```

## 外观

浏览器的登录页已经收敛到一种构图：一个像素图标、一个大的衬线标题和一句柔和的话，
居中放在一片暗色地面上，别的一概没有。这个页面保留那种构图，并把它穿成项目自己的
身份。

- **图标就是 mascot。** 页面读取默认 mascot 的块状艺术
  （`Mascot::default().art()`，即启动横幅和 README 横幅所画的那只冠毛雏鸟，
  `docs/ui/mascot.md`），把它画成内联 SVG，每个象限一个像素。一个象限字形把一个
  终端单元格分成四份，而一个单元格的高度约为宽度的两倍，所以每个像素画成 10 个
  CSS 像素宽、20 个高：是终端所显示的那只生物，而不是被压扁的。因为页面读的是那
  份艺术而不是它的一份拷贝，页面与横幅不会彼此漂移。一行里连续点亮的像素是一个
  矩形，而 SVG 用 `crispEdges` 渲染，所以任何缩放下像素之间都不会显出接缝。
- **横幅的洗色。** mascot 穿横幅的青色 → 蓝色渐变（Catppuccin Mocha 的 `sky` →
  `blue`，即默认主题给它的横幅上洗的那对 `accent` → `link`），并在 Mocha 的
  `mantle` —— README 横幅的地面 —— 上发光，之上是一层淡淡的暗角。
- **一片淡淡的像素天空。** 四十颗暗色像素星代替 README 横幅的夜空。图案是固定的
  （一个带种子的生成器，所以每个页面都是同一片天空，测试也不需要随机），每第五颗
  星闪烁，而没有一颗落在文字所占的那片空地里 —— 在那里一颗星读起来像页面上的
  一粒灰。
- **字体。** 标题是系统自己的衬线体（Safari 里是 New York，macOS 上是 Charter，
  Linux 上是 Bitstream Charter，Windows 上是 Sitka、Cambria 或 Georgia），那句话
  用系统的无衬线体，而一条命令是强调色里的等宽 chip，即 TUI 的行内代码打扮。
- **浅色浏览器。** 在 `prefers-color-scheme: light` 下页面取 Catppuccin Latte，
  同样的角色用浅色变体，而星星没了：浅色地面上的暗色小点读起来像脏东西。

## 页面一眼说出什么

每个页面有一个**语气**，它决定 mascot 的颜色、它怎么动，以及标签页的图标
（`page::Tone`）：

| 语气 | 何时 | Mascot | 动作 |
| --- | --- | --- | --- |
| `success` | 登录拿到了它的 code | sky → blue | 跳，落地时压扁，跳起的顶点有四颗像素火花在它周围爆开 |
| `refused` | 授权服务器拒绝了这次登录（用户在其同意屏上拒绝了） | red → peach | 摇头 |
| `notice` | 没有失败也没有收到东西：另一个登录的答案、一个来得太晚的答案、一个误入的请求 | yellow → peach | 歪头，困惑 |

在每个页面上，mascot、标题、那句话和任何细节依次升起到位，相隔 80 ms。之后
mascot 每五秒半眨一次眼：它的眼皮从上合到眼睛上。眼睛是*找出来的*，不是列出来的：
它们是艺术中那些外面无法经由其他未点亮像素到达的未点亮像素（从艺术四周做一次
flood fill），所以重画的 mascot 也能继续眨眼。在
`prefers-reduced-motion: reduce` 之下这一切都不跑，而每个元素的静止状态就是它完成
后的样子，所以没有任何东西被留在不可见或动到一半的状态。

## 每个页面说什么

| 请求 | 状态 | 标题 | 那句话 |
| --- | --- | --- | --- |
| 这次登录的 code | 200 | 流程自己的（`Pages::received`）：`Authorization received`、`Signed in to ChatGPT` | You can close this tab and return to Zeroth. |
| 这次登录的拒绝 | 200 | Sign-in refused | The authorization server turned the sign-in down. Return to Zeroth to try again. |
| 不是这次登录的答案 | 400 | Not this sign-in's response | 那个理由，作为一句句子，然后是：To sign in, start again from `/mcp` in Zeroth. |
| 登录已停止等待之后的一个答案 | 410 | This sign-in has ended | It stopped waiting before this answer arrived. To sign in, start again from `/mcp` in Zeroth. |
| 既无 code 也无 error 的回调路径 | 400 | Not a sign-in response | It carries neither an authorization code nor an error. |
| 其他任何路径（手敲的地址） | 404 | Nothing here | This address only receives sign-in responses. |

这条命令是流程自己的（`Pages::restart`）：MCP server 是 `/mcp`，provider 是
`/login`。应用的名称是 `crate::APP_NAME`，即每一个点名应用的字符串都读的那一个
常量。

一个拒绝的页面把授权服务器自己的话在细节框里单独摆出来，它的 `error` code 用红色
等宽字压在它的 `error_description` 之上。这就是 `loopback::Rejected::Refused` 携带
一个把两者分开保存的 `Refusal`、而不是一句成品句子的原因：页面显示各个部分，而流程
与终端仍然读到它们一向读的那句
（`the authorization server refused the sign-in: access_denied (…)`，即 `Refusal`
的 `Display`）。

## 自包含，并且锁死

- **什么都不抓。** 没有网络字体、没有图片、没有脚本、没有样式表链接：这个页面由
  一个终端应用在 loopback 上提供，所以它必须离线绘制，而一个来自 CDN 的字体会把
  这次登录告诉第三方。样式表、mascot、天空和 favicon 全都是内联的。favicon 还是
  那个 mascot，作为按语气配色的 SVG `data:` URL，取 Latte，因为它饱和的两端在暗色
  与浅色的标签条上都读得出来。内联它同时也意味着浏览器从不向回调要
  `/favicon.ico`。
- **外面来的一切都被转义。** 授权服务器的错误与描述、以及一个重定向的 issuer，在
  页面显示它们的任何地方都被 HTML 转义，标签页标题也算在内。
- **响应只允许页面用得到的东西。** 它的 `Content-Security-Policy` 是
  `default-src 'none'` 加上页面的内联样式与 `data:` 图片：完全不许脚本、不许表单、
  不许 `<base>`，永不在 frame 之内。`Referrer-Policy: no-referrer`，因为浏览器所在
  的那个地址携带着授权 code，而 `X-Content-Type-Options: nosniff` 挨着页面一向有的
  `Cache-Control: no-store`。

## 为什么颜色是固定的

终端的颜色跟随 `/theme`（`docs/ui/theme.md`）；页面的不跟。浏览器读不到终端的
调色板，`ansi` 主题也没有任何 RGB 值可给它，而页面是项目的脸面，不是一个 session
的视图。它的颜色是 Catppuccin 自己的，Mocha（默认主题的变体）与浅色浏览器用的
Latte，也就是 `ui/palette.rs` 的 `MOCHA` 与 `LATTE` 两张表所取自的那两个变体。
它们作为 CSS 住在页面的样式表里，而不是作为 `Color` 值，所以那条禁止在
`palette.rs` 之外写字面颜色的规则达不到它们。出于同样的理由，页面画的是默认
mascot 而不是目录 `/mascot` 的选择，那个选择登录 worker 从来就看不到：它显示的是
项目的印记。

## 测试

`llm::loopback::page::tests` 钉住这个页面：任何 mascot 被画出的每一个字形都能
解码，
默认 mascot 解码成横幅那只生物、它的眼睛在艺术有洞的地方，SVG 把终端的 1:2 像素
留在它自己的框内，只有 success 会爆火花，外面来的每一个词都被转义，页面上没有
任何东西被抓取，favicon 穿上了语气，响应携带着它的策略 header 与精确的
`Content-Length`，天空是固定的并让文字保持干净，而动效会让位于 reduced motion。
`llm::loopback::tests` 把每一个裁决与每一个误入的请求都钉到它的页面上，而真实
socket 的测试检查浏览器实际收到的内容。`scripts/smoke.sh 148` 对着一个 stub 登录
Google Antigravity，并读真实二进制所提供的那张回调页。
