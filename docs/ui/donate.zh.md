# `/donate` —— 加密货币捐赠页

**`/donate`** 命令打开一个只读页面，列出项目的加密货币捐赠地址，好让一个想支持开
发的用户在他本来就待着的地方找到这些地址 —— 在终端里，一条命令之外 —— 并且不离开
对话就复制一个。它是第十一个替换 composer 的内联 picker，而且刻意是 `/hooks` 浏
览器的兄弟：**没有文本输入**，硬件光标藏起来，打开时拥有每个键，Esc 让 composer
回来。

## 那些地址

| ticker | coin     | address                                        | networks                                                |
| ------ | -------- | ---------------------------------------------- | ------------------------------------------------------- |
| `BTC`  | Bitcoin  | `bc1qhwamfrwuhz64pk00l75ykfff2ang22ns64chf7`   | Bitcoin (Native SegWit)                                 |
| `ETH`  | Ethereum | `0xEAf6fbabB9DBE7a23BfE22A7A6c4aCe02063524b`   | Ethereum, Linea, Base, Arbitrum, BNB Chain, OP, Polygon |
| `SOL`  | Solana   | `Gwhv5c6uAa6aAz1MjwzV9QJpbm7CJWy2kuCeZ75mFc94` | Solana                                                  |

**每个条目都点名它的网络，而页面把它们显示出来。** 一个地址不意味着一条链：那个
EVM 地址在上面七条链上是同样的二十个字节，所以一个只点名 Ethereum 的页面会让用户
去猜 Base 安不安全 —— 而猜错的后果不可挽回。条目说它们的地址*可以*发到哪里；那段
告诫说一次，其他地方都不行。（更早的一版根本不点网络，理由是每种币恰好只有一条
链；一个多链地址让它退役了。）

目录是纯的 `app::DONATION_ADDRESSES` —— 每行一个 `DonationAddress`（`ticker`、
`coin`、`address` 和 `networks`，一个 `&'static [&'static str]`），按页面列出的
顺序。它是一个 const 而不是一个文件：这些地址是项目的，不是用户的，而一个从磁盘
读它们的页面，会是一个任何对配置目录有写权限的人都能改向的页面。目录的测试钉住每
个地址都是 ASCII、没有空白，每个 ticker 都是一个互不相同的大写词，每个条目至少有
一个朴素的、不带填充的网络标签 —— 这样将来某个条目里的错字，或者一个告诫覆盖不到
的条目，会让构建失败，而不是让捐赠失败。

## 页面

```
────────────────────────────────────────────────────────────────────────
  ♥ Support Zeroth

  Free and open source, developed in the open. If it earns a place in
  your terminal, a donation keeps the work going — thank you.

  ❯ 1. BTC  Bitcoin
     ╭──────────────────────────────────────────────╮
     │  bc1qhwamfrwuhz64pk00l75ykfff2ang22ns64chf7  │
     ╰──────────────────────────────────────────────╯
     Network: Bitcoin (Native SegWit)

    2. ETH  Ethereum
     ╭──────────────────────────────────────────────╮
     │  0xEAf6fbabB9DBE7a23BfE22A7A6c4aCe02063524b  │
     ╰──────────────────────────────────────────────╯
     Networks: Ethereum, Linea, Base, Arbitrum, BNB Chain, OP, Polygon

    3. SOL  Solana
     ╭────────────────────────────────────────────────╮
     │  Gwhv5c6uAa6aAz1MjwzV9QJpbm7CJWy2kuCeZ75mFc94  │
     ╰────────────────────────────────────────────────╯
     Network: Solana

  Send each coin only over a network listed under its address — a
  transfer on any other network cannot be recovered.

  ↑↓ navigate  enter/c copy address  esc close
────────────────────────────────────────────────────────────────────────
```

外框是 picker 家族的 —— 同样的分隔线、同样的两列内缩、同样的暗色提示行 —— 而框里
坐的东西是用外壳已有的零件搭的，所以这个页面读起来是*这个* app 的，而不是一张粘
进来的表单：

- **标题穿着横幅的渐变。** `♥ Support Zeroth` 被从左到右刷过头部自己的强调色
  → 链接色渐变（`ui::header::gradient_spans`，mascot 横幅所用的那个构建器），粗
  体，前面一颗调色板红色的心 —— 那是这个色相唯一意味「喜爱」而不是「失败」的地
  方。名字读自 `APP_NAME`，那是 app 名字唯一待着的地方，而一个测试把它钉在那里。
- 它下面的**简介**是两行暗色换行文本：这个项目是什么，一次捐赠做什么。是换行，不
  是截断 —— 页面的高度就是它自己的行数（`docs/ui/view-flow.md`），所以一个窄终端
  花掉一行，而不是一个词。
- **每个地址是圆角框上的一个带编号行。** 那一行是 `/hooks` 菜单的形状 —— `❯` 标
  记在被高亮的那个上、一个绝对 `{n}.` 编号、**ticker** 粗体，然后是币名暗色（
  `Bitcoin`），后面什么都没有 —— 没有任何网络从句骑在这一行上，而一个测试把每一
  行整体读一遍来保持这个状态 —— 而地址坐在它下面、`/login` 设备页那个圆角框里，
  明亮粗体，像那个一次性验证码，因为它是页面上唯一要抄下来的东西。框按地址大小定
  尺寸；在一个窄得坐不下它的终端上，地址**在框内换行**（`wrap_output`，什么都不
  截断）而不是溢出外框，而 `c` 正好是为这种情形准备的。
- **网络是框下面的一行说明。**
  `Networks: Ethereum, Linea, Base, Arbitrum, BNB Chain, OP, Polygon` —— 暗色，
  内缩到框自己的左墙，在那里换行（七个链名放不进一个窄窗格，而一个读者看不见的网
  络就是他们无从知道安全的网络）。它坐在框*下面*、而不是标签行上，有两个理由：标
  签保持一眼的宽度，不管一个地址对应多少条链；而一个东西下面的说明读起来是关于那
  个东西的注记。标签与数量一致 —— `Network: Solana`、`Networks: …` —— 因为一个
  `Network(s):` 的含糊写法，在一个读者正一个字一个字核对的页面上读起来像生成的文
  本。它在被高亮的那一行上也保持暗色：强调色属于那个选中项，而点亮说明会让框的边
  框和它抢眼睛。
- **选中由颜色显示。** 被高亮那一行的标记、编号和 ticker 在强调色里亮起来，它的
  框边框也是；其他行保持柔和的墨色和暗色框边框。这是调色板的规则 —— 整个选中行亮
  起来，没有第二个光标 —— 也正是它让 `❯` 下面的那个框读成*那个*地址，而不是三个
  中的一个。
- **那段告诫是琥珀色的。**
  `Send each coin only over a network listed under its address …` 穿着调色板的警
  告色，也就是 ask 审查里未回答问题的色相，在一个其他一切都是暗色或明亮的页面
  上：一次错网络的转账是这个页面可能导致的唯一不可逆错误，所以它是地址被使用之前
  眼睛被拉过去的那一行。它**指向**那些说明而不再重述它们 —— 不点任何币也不点任何
  网络 —— 所以一个目录条目，或者加进某个条目的链，不用碰它就都被覆盖了。
- **块之间一个空行，绝不是一个以上。** 页面是用恰好一个空隙连接起来的块搭的
  （`/login` 页面规则），而一个测试走遍每个宽度断言没有两个空行会叠在一起。

页面是**静止的** —— 上面没有任何东西在跳 —— 所以它的 scrollback 流动像 `/mascot`
picker 那样按行签名。

## 终端太短，放不下每个框

三个框及其说明让整页大约三十行。在一个更短的终端上从底部锚定，它会留住最后几个地
址和那段告诫，而把最初几个从顶上流走 —— 其中就有被高亮的那个地址，也就是 Enter
会复制的那个。所以构建器拿到它可以有的行，构建第一个放得下它们的页面（
`docs/ui/view-flow.md` *A page that sheds rows before it flows*）：

1. 整页，每个地址都带框；
2. 一个**手风琴**：高亮不在的那些地址塌缩成它们的标签行，而被高亮的那个在它自己
   的标签下面保住它的框和网络说明 —— 列表保持顺序，所以 ↑/↓ 让打开的框从一行移到
   另一行；
3. 那个手风琴去掉标题和简介；
4. 被高亮的地址**单独一个** —— 它的标签、框和网络 —— 它带编号的标签（
   `2. ETH  Ethereum`）说出它在其他几个之间站在哪。紧凑页的 `↑ N more above` /
   `↓ N more below` 槽（`model_compact_list`）在这里会直接坐在顶部分隔线下面、在
   第一个地址处空着贴上去 —— 那是外框从不保留的填充行（`docs/ui/view-flow.md`
   *The frame hugs its content*）—— 而一个随选择来去的 `↑` 槽会移动外框，所以这
   一页不要。

告诫和提示在每一页上都留着：一个说一种币可以走哪些网络，另一个说怎么复制它。只有
比最后一页还短的终端才从底部锚定它，并像每个带框视图一样把它的顶部流进
scrollback；在 12 行上，那唯一一个地址整体放得下，它的标签直接坐落在顶部分隔线下
面。`smoke.sh` Phase 139 在 12 行上打开这个页面。

## 键

页面打开时拥有**每一个**键（在 `App::on_key` 顶部路由，先于 composer 的全局
Ctrl+C/Ctrl+O），`/hooks` 的语法：

| 键                | 做什么                                                          |
| ------------------ | ------------------------------------------------------------- |
| ↑ / ↓              | 移动高亮，在两端**环绕**（`wrap_step`）    |
| Home / End         | 跳到第一个 / 最后一个地址                              |
| `1`–`9`            | 跳到那个地址**并复制它**（ask 模态框的规则）   |
| Enter / `c`        | 把被高亮的地址复制到剪贴板                 |
| Esc / Ctrl+C       | 关闭 —— composer 回来                                  |

其他任何键都被吞掉：可打印键永远到不了下面那层 composer 草稿，粘贴也到不了（没人
会粘贴的东西是捐赠地址 —— `/settings` 的吞掉规则）。复制**让页面保持打开**：确认
toast 落在外框上方、地址仍在视野里，而一个想要不止一个地址的用户不用重新打开就复
制下一个。这个页面像每个 picker 一样在**回合中途**工作 —— 它只替换 composer，流
式条保住它上方的行 ——而且像每个 picker 一样，它在打开时把运行中 cell 的
`(ctrl+b to run in background)` 提示清空（`App::background_hint_elapsed`），因为
它会吞掉那个提示所宣传的 Ctrl+B。

页面上任何地方都不输入文本，所以硬件光标是**藏起来的**（`ui::cursor_visible` ——
权限 prompt 的规则：一条 kitty 光标拖尾会在每次 ↑/↓ 时在框上拉出痕迹），而它的*
座位*仍然跟着被高亮的 `❯` 行（`menu_marker_seat`，`/hooks` 的座位），所以页面关
闭时光标回来时会从一个合理的地方开始。

## 复制一个地址

Enter、`c` 或一个数字返回 `Action::CopyDonationAddress(address)` —— 纯核心决定复
制 *什么*，循环做 I/O（`tui::donate::Session::copy_donation_address`）：`/copy`
的剪贴板路径（`clipboard::copy_to_clipboard` —— arboard，配 OSC 52 兜底，用于无
头、SSH 或 tmux 的 session；原生租约持有到 app 的生命期结束，
`docs/ui/copy.md`），然后一条瞬时 toast ——
`Copied the BTC address to clipboard`，按实际复制的东西措辞，也就是 `/login` 设
备页 `Copied the code to clipboard` 的兄弟 —— 或者一条红色
`Copy failed: {reason}`。绝不是一个 scrollback 圆点（`docs/ui/toast.md`）。

## API

- `app::DonationAddress`（`ticker`、`coin`、`address`、`networks`）/
  `app::DONATION_ADDRESSES` —— 目录。
- `app::DonatePicker` —— 打开页面的状态（被高亮的行）；
  `App::open_donate_picker`、`close_donate_picker`、`highlighted_donation`、
  `on_key_donate_picker`。
- `Action::OpenDonatePicker` / `CloseDonatePicker` /
  `CopyDonationAddress(DonationAddress)`。
- `ui::donate_view` —— `donate_view_lines`（页面构建器；它的长度就是保留高度）、
  `donate_picker_height`、`render_donate_picker`。
- `tui::donate::Session::copy_donation_address` —— 那次剪贴板写入和那条 toast。

## 测试

- `app/tests/donate.rs` —— 目录（三个地址 —— BTC、ETH、SOL —— 确切的字符串，
  ASCII 且无空白，互不相同的大写 ticker，以及每个条目的网络：那个 ETH 地址上的七
  条 EVM 链，BTC 和 SOL 各一条，每个名字都朴素、不带填充）、`/donate` 命令打开页
  面（空闲和回合中途）并放弃共享 composer 的那些条带，以及整套键语法（环绕的 ↑/
  ↓、Home/End、Enter/`c`/数字复制、Esc/Ctrl+C 关闭、拥有每个键、Ctrl+B 提示时钟
  被抑制）。
- `ui/tests/donate_view.rs` —— 带框的页面（分隔线、点名 `APP_NAME` 的渐变标题、
  简介、每一行和每个框、网络说明、那段告诫、提示）、每一行整体读作 ticker 加币名
  后面什么都没有、说明直接坐在它的框下面而标签与数量一致、对齐到框的墙并在每个选
  择下都是暗色、七名说明在 40 列上换行（并整体读回）、选中点亮它的行和框、地址在
  窄宽度下在框内换行、宽度安全、高度契约、流动资格、藏起来的光标坐在 `❯` 上，以
  及不成叠的空行规则。
- `scripts/smoke.sh` Phase 112 —— 页面在真终端里端到端：从 palette 打开，三个地
  址都在它们的框里、网络说明在它们下面，而每一行整体读作（币名后面没有网络），↓
  把 `❯` 移到 ETH，`c` 复制被高亮的地址（那条 toast，以及 OSC 52 兜底原样落进
  tmux 的粘贴缓冲），↓ 到达 SOL 并环绕回 BTC，Esc 让 composer 回来。
