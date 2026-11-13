# 极长的输出行 —— 有界的行数，诚实的计数

一个折叠的 tool 单元格显示它输出的开头，其余藏在
`… +N lines (ctrl+o to expand)` 提示之后。「开头」*是什么意思*，就是这份文档
的全部主题，而它曾经错了两次 —— 两次都是因为单元格用**源码行**来量自己，而
用户是用**显示行**来读它的。

第一个版本只按源码行做预算（`TOOL_PEEK_LINES` 行），每一行都完整换行渲染，这
样长行的尾巴永远不会消失在终端边缘之外（`docs/ui/tool-streaming.md`）。这在
某一行*病态*之前是对的 —— 一个压缩过的 bundle、一块 base64、一段 `curl`
拿回的 2 KB JSON 正文。那时单独一个源码行就吃掉了整个单元格：

```
● Bash(which yt-dlp youtube-dl 2>/dev/null; curl -s
      "https://www.youtube.com/oembed?url=https://youtu.be/Vr-NEWVLN9Y&f
      ormat=json" 2>/dev/null)
  ⎿  /home/linuztx/.local/bin/yt-dlp
     {"title":"How to Use Burp MCP with Claude to Find Bugs from HTTP
     History","author_name":"\ud835\ude47\ud835\ude64\ud835\ude68\ud835\
     ude69\ud835\ude68\ud835\ude5a\ud835\ude58","author_url":"https://ww
     w.youtube.com/@lostsec_ftw","type":"video","height":113,"width":200
     ,"version":"1.0","provider_name":"YouTube","provider_url":"https://
     www.youtube.com/","thumbnail_height":360,"thumbnail_width":480,"thu
     mbnail_url":"https://i.ytimg.com/vi/Vr-NEWVLN9Y/hqdefault.jpg","htm
     l":"\u003ciframe width=\u0022200\u0022 height=\u0022113\u0022
     src=\u0022https://www.youtube.com/embed/Vr-NEWVLN9Y?feature=oembed\
     u0022 frameborder=\u00220\u0022 allow=\u0022accelerometer;
     autoplay; clipboard-write; encrypted-media; gyroscope;
     … +1 lines (ctrl+o to expand)
```

十二行读不动的折行 JSON —— 整个块的上限全花在**一行**上 —— 收尾的提示却声称
只藏了*一*行。两半都错，而且错得一模一样：单元格用源码行量自己，而用户是用
**显示行**读它。

## 规则

> 折叠单元格的输出块在 **`TOOL_FOLD_ROWS`** 个显示行处**折叠** —— Claude Code
> 的三行 —— 那一行是 `… +N lines` 提示，恰好再多一行的输出整个显示；提示统计
> 的是展开将**追加的行数**；命令单元格不把那些行花在一个**空**行上；而一行若
> 是一个 **JSON 文档**，会在以上这一切之前先被重塑成适合显示的样子。带编号的
> 文件单元格也按行做预算，而它的十行是一个**上限**：一瞥开始的那一行，在剩下的
> 行装得下时整个显示，需要更多的那一行则在最后一行处被切断，以 `…` 收尾
> （§7、§8）。

这条规则是分层到达的，而这些层就是把它做错的历史：§1–§3 约束的是*行*；§0
（在那个麻烦第二次换装回来之后加入）约束的是*块*；§4 决定这个块是*什么的*块；
§5 与 §6 是 Claude Code 自己那个折叠的移植，而块的预算最终就变成了它；§7 把行
预算从文件单元格里撤掉，那是它最后的据点；§8 让它们的块预算重新成为上限，因为
§7 放过的一行涂满了一屏。它们保持这个顺序，因为每一节都解释下一节。

### 0. 是行，不是列（块有行预算）

约束行并不等于约束单元格。四个源码行，每个都*在*它那 3 行预算之内，仍然花了
十二行 —— 而那正是对任何网页做一次 `curl` 的日常形状：

```
● Bash(curl -s "https://en.wikipedia.org/wiki/World_Chess_Champion" |
      grep -i "champion" -A 5 | head -n 20)
  ⎿  <title>World Chess Championship - Wikipedia</title>
     <script>(function(){var className="client-js
     vector-feature-language-in-header-enabled
     vector-feature-language-in-main-menu-disabled…
     RLSTATE={"ext.globalCssJs.user.styles":"ready","site.styles":"ready
     ","user.styles":"ready","ext.globalCssJs.user":"ready","user":"read
     y","user.options":"loading","ext.cite.parsoid.styles":"ready","ext…
     <script>(RLQ=window.RLQ||[]).push(function(){mw.loader.impl(functio
     n(){return["user.options@12s5i",function($,jQuery,require,module){m
     w.user.tokens.set({"patrolToken":"+\\","watchToken":"+\\","csrfTok…
     … +122 lines (ctrl+o to expand)
  ⎿  Allowed by auto mode classifier
```

十行标记，对于一个全部职责只是说*命令跑了，这是瞥一眼*的单元格。里面没有一件
是行层面的 bug：每一行都在 3 行处被截断并标记。bug 是**块**没有行预算 —— 它的
上限是两个行预算的*乘积*（`TOOL_PEEK_LINES * TOOL_LINE_MAX_ROWS` = 12），所以
同样的四行在短时花四行，在折行时花十二行。一个高度取决于输出恰好是什么形状的
单元格，就是一个你没法扫一眼的单元格。

块被给了行预算：就这么多个显示行，不管输出长什么样 —— 起初是四行，旁边是一个
四源码行的预算，谁先用完算谁的；后来，在量过 Claude Code 自己的折叠之后（§5），
变成**提示之上的三行**。定下来的块的上限两种方式都是 `TOOL_PEEK_ROWS`（4），
提示也算在内，而那也正是**运行中**尾部所显示的窗口
（`running_command_lines` 显示最后四*行*），所以头和尾的预算是一致的：一个
`bash` 单元格在落定时不会*变高*。（它可以落定得更矮 —— §4 让块在第一个空行处
结束 —— 但尾部仍然是那个上限。）

### 1. 逐行的行预算（`TOOL_LINE_MAX_ROWS = 3`）—— 已退役

针对那种块的第一剂药是*逐行*封顶：任何单个源码行都不能在一个折叠单元格里花掉
超过三行，于是那块东西只显示头就停住，它后面那一行仍能拿到一行。在 exec 单元格
的块预算之内，这个封顶在块于三行处折叠之后就多余了（§5）：折叠约束那块东西
和其余一切的方式相同，而 Claude Code 自己的单元格显示一个块的头部三行、不给后面
那一行留任何行，所以 exec 单元格把它去掉了。它在带编号的文件单元格里留了下来
（`ui/file_cell.rs`），直到 §7 在那里也把它拿掉。

### 2. 截断是可见的（`TOOL_LINE_ELLIPSIS`）—— 随之退役

在一行*确实*被截断的地方（文件单元格），最后保留的那一行以 `…` 结尾，位置经过
拟合，那一行永不溢出宽度；没有它，一个被截断的行读起来就是一行恰好在哪儿结束
了。这个标记随封顶一起走掉（§7），又为文件一瞥的 ceiling 所造成的唯一一次截断
回来了（§8）。exec 折叠从来没有过它：`… +N lines` 提示就落在第三行正下方，
已经说了后续还有 —— 那是 Claude Code 的样子，在它之上再加一个标记，只会为了把
同一件事说两遍而替换掉那一行的最后一个字符。

### 3. `+N lines` 统计的是按 Ctrl+O 会追加多少

这个提示过去统计**没有完整显示的源码行**，这就是 1.8 KB 被藏起来的 JSON 变成
`… +1 lines` 的原因。它现在统计没有显示的**显示行** —— 也就是展开实际绘制的
那些行 —— 用于那些输出没有行号的单元格（`bash`、diff fallback、ask 单元格的
各个答案；`!` shell 单元格把它的整个输出内联显示，没有提示可统计，
`docs/input/shell-command.md`）。对日常输出来说这两个数字*相同*（短行各占一
行）；它们恰好只在旧计数撒谎的地方分岔。

这个计数是精确的而不是估算的：`WrapMode`（`ui/wrap.rs`）把每个换行器与它自己
的行**计数器**和**截断器**配对，三者都由一次发出区间的扫描驱动，所以一个提示
永远不可能统计出别的换行器会产生的行。那次扫描在贪婪断行之外的唯一一条规则是：
一个落在**恰好满**行上的断点，会消费掉它之后的空白串。把那个空白串带过去，过去
会把空格安置在下一行的行首（`total` / ` 12`，差一列对不齐），或者 —— 在下一个
满宽词之前 —— 让它自成一整行，`hello` / ` ` / `world` 之间一个幽灵空行，而
`+N lines` 提示随后会把它数进去；一个*装得下*的空格仍然留在它断行的那一行的
末尾，所以装得下的列对齐输出不受影响。Ctrl+O 视图用同一个 `wrap_output` 换行
命令输出、用同一个 `wrap_verbatim` 换行 diff 正文，所以 `+N` 就是展开所追加的
行数，逐行对得上。计数不分配内存，这正是运行中尾部的页脚能在每一动画帧都用它、
而无需把保留的缓冲换行成 `String` 的原因（`docs/ui/tool-streaming.md`）。

**带编号的文件单元格仍然数行。** 一个 `read`/`write`/`edit` 单元格
（`ui/file_cell.rs`）绘制行号装订线，展开它显示的是同样的那些带编号的文件行
—— 所以那里的 `… +20 lines` 意思是再多二十*文件*行，也就是用户正从装订线上
读的那个单位。这条规则是同一条规则的两套打扮：**用展开所显示的单位来计数。**
那些单元格在上限处截断的那一行（§8），改为在它自己的行上做标记，所以一个被藏
起来的行仍然总是一个完整的行、由提示来统计。

### 4. 一瞥是输出的第一个块，不是它的头四行

上面一切都在为这一瞥做预算。这一节决定*它是哪一段的一瞥* —— 而诚实的答案曾经
是「头四行恰好是什么就是什么，包括空行」。

一个空行在四行单元格里花掉整整一行却什么都不说。有两种形状平常到会不断撞上：

```
● Bash(./build.sh)                              ● Bash(git status -sb)
  ⎿                                               ⎿  ## main...origin/main
     Building…                                       M src/ui/tool.rs
     Linking…
     Done                                            Untracked files:
     … +16 lines (ctrl+o to expand)                  … +16 lines (ctrl+o to expand)
```

左边那个单元格把第一行花在一个开头的 `\n` 上 —— 整个预算的四分之一花在什么都
没有上 —— 四行装得下，它只显示了三行。右边那个在中间画出一道空隙，然后又画了
*下一*节的一个碎片，读起来像一串行，其实不是：`M src/ui/tool.rs` 和
`Untracked files:` 在输出里并不相邻，而单元格说它们相邻。

所以块是输出的**第一个块**：

> 开头的空行被跳过；它们之后的第一个空行结束这一瞥。

```
● Bash(./build.sh)                              ● Bash(git status -sb)
  ⎿  Building…                                    ⎿  ## main...origin/main
     Linking…                                         M src/ui/tool.rs
     Done                                             … +18 lines (ctrl+o to expand)
     … +17 lines (ctrl+o to expand)
```

输出有四行内容行的地方就四行，第一个块有两行的地方就两行 —— 绝不是一行什么也
没有，也绝不是两段输出被当成一段呈现。

三个细节让它站得住：

- **计数保持精确。** 被跳过的开头空行和结束块的那个空行都是 Ctrl+O 会绘制的行，
  所以它们像任何其他被藏起来的行一样被计入（§3）。提示回答的是「还有多少」，
  而不是「还有多少我判定为有趣的」—— 悄悄把空行从计数里去掉的那个版本，会以
  更客气的打扮重新打开 `+1 lines` 那个谎言。
- **全空白的输出原样不动。** 哪里都没有一个非空行，就没有第一个块可优先，而一个
  空窗口会让 `… +N lines` 提示孤零零地挂着，上方没有 `⎿` 拐角。这条政策此时
  空操作（`BlankPolicy::window` 返回整个切片）。
- **只有命令输出用这条规则。** diff 正文的间距就是内容，而 ask 单元格的
  `· Q → A` 行没有空行可跳 —— 那些单元格传 `BlankPolicy::Keep`，所以它们的
  渲染逐字节相同。传 `FirstBlock` 的是 backend 的 `bash` 单元格。`!` shell
  命令，按设计只有一种单元格形状，现在**完全不折叠**了 —— 它的整个输出内联
  显示，空行什么的全都在，`…` 封顶标记只给一次被截断的收尾
  （`docs/input/shell-command.md`）—— 所以它不接受任何政策：一瞥窗口是折叠
  的事，而 shell 单元格没有折叠。

这条规则作用在**源码**行上，在换行之前，所以块里一个会换行的行仍然整个显示到
折叠处为止，它后面的空行仍然结束这个单元格。它让位于 §5 的那个例外：四行或更
少的输出整个显示，空行什么的一并显示 —— 没什么可折叠的，而一条提示会花的比它
藏的还多。

运行中的*尾部*刻意不动：它显示流式到达时的最后几行，在那里，一个空行是命令刚
刚打印的内容的一部分，把它丢掉会让尾部与终端不一致。

### 5. Claude Code 的折叠（`TOOL_FOLD_ROWS = 3`，以及那个例外）

参考实现是量出来的，不是看出来的。Claude Code 的 tool 结果
（它 bundle 里的 `OutputLine` → `renderTruncatedContent`）按终端宽度换行输出、
保留头**三**行，并在它们下面挂上 `… +N lines (ctrl+o to expand)`，其中 `N`
是第三行之后的行数 —— 有一个例外：当恰好有**一**行在折叠之外时，它改为显示
那一行，因为一条藏一行的提示，代价正是它所藏的那一行。我们现在以同样的方式
折叠，而这个例外也保留：四行的输出整个显示，五行的显示三行加 `+2 lines`。它
有两个细节被刻意**不**移植。它在列处硬断折叠的那些行（它自己的 transcript 视图
会给它们按词换行 —— 硬断是它折叠辅助函数的省事，不是一种样子）；我们按词换行、
保留空格，与其他各处一样，并且在遇到比任何一行都宽的 token 之前填满一行，就像
它自己的标题换行那样（`docs/tools/tools.md` *Long headers*），这正是两者在
token 密集的输出上一致的原因。而它的隐藏计数在超过字符预算时是估算
（`ceil(len / width) - 3`）；我们的是精确的，因为 `WrapMode::rows` 是边数边
构建的。

尾部的空行在折叠之前被丢掉（Claude Code 的 `trimEnd`）：`echo; echo` 的尾巴和
formatter 收尾的换行没什么可显示、也没什么可藏，所以它们既不计入单元格也不计入
提示 —— 而一个只有空行的输出在单元格里和 Ctrl+O 里都读作 `(no output)`。

### 6. 一行 JSON 被重塑成适合显示的样子（`TOOL_JSON_PRETTY_MAX_BYTES`）

报告的另一半。对 Wikipedia API 的一次 `curl` 打印出紧凑的一行
—— `{"batchcomplete":"","query":{"pages":{"75936044":{"pageid":…` —— 在我们
折叠出三行大括号的地方，Claude Code 显示

```
  ⎿  {
       "batchcomplete": "",
       "query": {
     … +16 lines (ctrl+o to expand)
```

因为它的结果渲染器在折叠之前**会把一行若是 JSON 文档的内容美化成缩进形式**
（两个空格缩进，`JSON.stringify(v, null, 2)`）。我们做同样的事，在
`ui::exec_display_lines` 里：显示正文中每一行只要能被解析成一个 JSON 对象或
数组**并且能往返** —— 忽略空白之后重新序列化得到同样的文本，即 Claude Code
自己的守卫 —— 就被替换成它缩进后的形式，落定的内联单元格与 Ctrl+O 展开都一样
（所以提示仍然精确统计展开所追加的行数）。这个守卫是让显示保持诚实的东西：
重复的键（解析时后者胜出）、一个序列化器会写成另一种拼法的数字、一个它会被
解析掉的转义，全都原样保留，因为重塑后的文本那时会是一份命令从未打印过的文档。
超过 `TOOL_JSON_PRETTY_MAX_BYTES`（10 000 —— Claude Code 的上限）的输出整段
不动，这给一次渲染所解析的 `Value` 树划了界（`docs/architecture/memory.md`
那条规则讲的是为了读取一份正文的几个字段而保留的树；这一棵是瞬时的，且按构造
就很小）。运行中的尾部显示按打印出来的样子 —— 还在流式到达的部分是不完整的，
而一行在光标下变形会让尾部与终端不一致。记录、面向模型的结果、rollout 与
context 重放都不受影响：这是一个渲染接缝，就像标题的路径显示。

### 7. 文件单元格把每一行整个显示

逐行封顶（§1）对唯一还保留它的那种单元格而言是笔亏本买卖。一个
`read`/`write`/`edit` 单元格是用户读 agent 读或写了什么的地方，而在那里跑过
三行的行很少是那种大块数据：一个 Markdown 段落（每段一行）、一个长字符串、
一个 HTML 行、一个 docstring。每一个都在它的第三行以 `…` 停住，于是句子的结尾
或闭合标签在屏幕上哪儿都没有，而报告很直白：把整行显示出来，而不是藏起来。
所以现在一个带编号的行从不被截断。内联、Ctrl+O 里以及 permission prompt 的
预览里都一样，一行显示它折成的每一行，diff 的着色给它每一行垫上底色。

仍然约束单元格的是这一瞥的行预算，作用在整行上：

> 只要还剩 `FILE_PEEK_LINES`（10）之内的行，一瞥就**开始**一行，并且把开始
> 的每一行**整个**显示。

所以一个短行文件像从前那样显示头十行；一个在那十行之内开始的长行被写完而不是
被截断，用它自己剩下的部分把单元格带过十行；而行用完之后，无论多短都不再开始
新的一行。一个按 72 列写成的 `README.md`，它第 3 行的那个段落过去停在第三行的
`tool call an…`：

```
● Write(README.md)
  ⎿  Wrote 11 lines to README.md
       1 # zeroth
       2
       3 An agent harness for coding, cybersecurity and automation that
         runs in your terminal, streams each reply and tool call inline
         as it happens, and leaves every finished message, tool call and
          file change in the terminal's own scrollback, where you can sc
         roll back and copy it like any other output.
       4
       5 ## Install
       6
     … +5 lines (ctrl+o to expand)
```

（内容行在列处硬断，代码在这些单元格里和在 Ctrl+O 里一向如此。）提示统计剩下
的文件行，而那总是整行。在一次 edit 里，被替换行的 `+` 行只要 context 和它之前
的 `-` 行还给预算留出一行，就会被显示。一条只在行*装得下*时才接纳的规则，会在
两半合起来超过十行时把它丢掉，显示被移除的却不显示替代它的。

代价是 §1 为之而写的那个案例：一瞥开始的一行病态的行被整个显示，一个压缩过的
bundle 把它的每一行都内联涂了出来。当时判断它是有界的 —— 被 `read` tool 自己
的上限（`TOOL_OUTPUT_MAX_BYTES`，64 KB，80 列下不到一千行）所界，以及被「提交
的单元格落进 scrollback 而不是 live 区域」所界。它没有：`write` 没有这样的
上限，而 scrollback 正是用户读东西的地方。§8 就是说这件事的那份报告，以及它
放回去的那个上限。

一个流式写进单元格的 `write`（`docs/ui/write-streaming.md`）显示文件当前状态下
最新的 `FILE_PEEK_LINES` **行** —— 是行不是列，像终端的 tail 那样。一行还在
到达时，它在其底行显示刚刚到达的文本（在截断下它显示的是该行的头三行加 `…`，
流进来的内容一个字都没有），而窗口的顶行可能是一行已经开始滚出去的行的结尾。
无论行多长，窗口都不长过十行，所以实时条带保持它的高度；内容全部到达之后，
单元格翻到头部，那正是提交后一瞥逐行相同的样子。

这个窗口每帧要构建好几次，而为保留一行最后几行而换行整行要付出整行的代价：一个
1 MB 的压缩行一次构建花 38 ms，而它所替代的截断三行之后就停了。所以一行的尾部
在不换行它的头部的前提下被找到（`code_content_tail_rows`）。当每个字节都是可打印
ASCII —— 一行那么长时压倒性的情况 —— 每个字符是一个字素、一列宽，第 `k` 行从
字节 `k × width` 开始，只有从那里往后的行被换行：同一行一次构建 0.7 ms。其他
任何文本都被整体换行、丢掉头部。两种方式得到的行相同，有一个差分测试把这个算术
钉住。

### 8. 文件一瞥的那十行是一个上限

§7 接受的代价以一份报告的形式回来了：一次 `Write` 写一个样式表，它的第四行是
被压缩的整张表，一行几千列。

```
● Write(src/styles.css)
  ⎿  Wrote 10 lines to src/styles.css
       1 @import url('https://fonts.googleapis.com/css2?family=DM+Sans:wght@400;450;500
         ;550;600;650;700&family=Manrope:wght@400;500;600;650;700;750;800&display=swap'
         );
       2
       3 :root{font-family:'DM Sans',sans-serif;color:#272932;background:#fafbfc;font-s
         ynthesis:none;font-weight:400;--purple:#6c5ce7;--purple-dark:#5948d6;--muted:#
         90919c;--line:#e8e9ee;--surface:#fff;--green:#2c9a73}
       4 *{box-sizing:border-box}body{margin:0;min-width:360px}button,a{-webkit-tap-hig
         hlight-color:transparent}button{font:inherit;cursor:pointer}button:disabled{cu
         rsor:not-allowed}button:focus-visible,a:focus-visible{outline:3px solid rgba(1
         08,92,231,.28);outline-offset:4px}button{color:inherit}button svg{flex-shrink:
         [… well over a hundred more rows of line 4 …]
     … +6 lines (ctrl+o to expand)
```

第 4 行从一瞥的第八行开始，所以一瞥开始了它，而 §7 的规则把它每一行都显示了：
一个本意只是十行一瞥的单元格，把一屏压缩 CSS 涂进了 scrollback，而它下面的提示
仍然只数六行。§7 所依赖的那个界，对一个 `read`（它的输出由 tool 封顶）成立，
对其他任何东西都不成立：模型所*写*的东西没有上限，而一个已提交单元格的行会永久
留在用户的 scrollback 里。这正是 §0 的教训穿上文件单元格的打扮：一个高度取决于
内容恰好是什么形状的单元格，就是一个你没法扫一眼的单元格。

所以现在那十行是一个上限。§7 的规则里，只有撞上它的那一行发生了什么改变：

> 只要还剩 `FILE_PEEK_LINES`（10）之内的行，一瞥就**开始**一行，剩下的行装得
> 下就**整个**显示，否则就显示剩下的那些行并在最后一行处**截断**，用一条暗色
> 的 `…` 收尾（`FILE_PEEK_CUT_MARKER`）。

```
● Write(src/styles.css)
  ⎿  Wrote 10 lines to src/styles.css
       1 @import url('https://fonts.googleapis.com/css2?family=DM+Sans:wght@400;450;500
         ;550;600;650;700&family=Manrope:wght@400;500;600;650;700;750;800&display=swap'
         );
       2
       3 :root{font-family:'DM Sans',sans-serif;color:#272932;background:#fafbfc;font-s
         ynthesis:none;font-weight:400;--purple:#6c5ce7;--purple-dark:#5948d6;--muted:#
         90919c;--line:#e8e9ee;--surface:#fff;--green:#2c9a73}
       4 *{box-sizing:border-box}body{margin:0;min-width:360px}button,a{-webkit-tap-hig
         hlight-color:transparent}button{font:inherit;cursor:pointer}button:disabled{cu
         rsor:not-allowed}button:focus-visible,a:focus-visible{outline:3px solid rgba(…
     … +6 lines (ctrl+o to expand)
```

§7 想要达到的目的在它之后仍然成立：

- 一个短行文件显示它的头十行，一如既往；而剩下那些行装得下的长行整个显示：
  一个段落再也不会按逐行计数被截断，像它在三行处那样（§1）。§7 的那个
  `README.md` 渲染得与那里画的一模一样。
- 最多只有一行被截断，而且只被上限截断，绝不因为自身长度：它是这一瞥显示的最后
  一行。
- 一次 edit 仍然在被替换行的 context 和 `-` 行还给出一行时显示它的 `+` 行 ——
  开始规则是 §7 自己的 —— 在剩下的行处截断，而不是被带过上限。一个自己就到了
  上限的 `-` 行会让它的 `+` 行不被开始，与它在 §7 下一样。在两半之间分摊行数会
  显示更多被替换的内容，但那只能通过在单元格中间截断旧行来实现，而那就是 §1
  的逐行封顶。

这个标记是 §2 的，为这唯一一次截断回来了，而 exec 折叠不用标记的理由在这里不
成立。折叠的提示数行，所以它第三行下面的 `+N lines` 已经说了那一行的文本还在
继续。文件单元格的提示数文件行，而它的代码行在宽度处硬断，所以一个满的最后一行
无论它的行在那里结束还是还要跑一千行，读起来都一样；只有 `…` 说明是哪种。它占
用该行的最后一列，暗色，落在该行自己的底色上，所以一个新增行的绿色条带一直不
断地延到截断处；这一行的内容从自己的末尾让出它需要的列。

**提示仍然数整行。** `+6 lines` 是第 5–10 行，也就是一瞥从未开始的那些文件行，
所以装订线上最后一个数字加提示就是头部的计数，与每个文件单元格一样；被截断那一行
的其余部分，就是它那行上的 `…` 所代表的。一个后面没有行的被截断行 —— 一个压缩
成单行的文件，读出来或写进去 —— 不藏任何整行，而 `+0 lines` 会说 Ctrl+O 后面
什么都没有。那时提示行是 `… (ctrl+o to expand)`：没有计数，但仍是通向其余部分的
路。

**被截断行的尾部永不换行。** `code_content_head_rows` 构建一瞥所保留的行，并在
第一个会开启超出它们的新行的字素处停下：贪婪断行只会往回看，所以整个换行的头部
就是头部的换行，有一个差分测试把它钉住。这一点对一个流式写进单元格的 `write`
最要紧，因为调用等待和运行期间，条带每帧都会重建好几次它的头部。在 §7 下，那个
头部每次构建都会把一行 1 MB 的内容整个换行并保留每一行 —— 80 列下是 13,889 行
—— 那就是 §7 早已用算术从尾部挪走的代价（一次构建 38 ms）。现在同一行的十行
一次构建约四分之一毫秒，比尾部的算术还少，而
`the_head_of_a_huge_finished_line_stays_inside_the_frame_budget` 把它和尾部的
闸门一起锁在一帧之内。

Ctrl+O 和 permission prompt 的预览不受影响：展开仍然把每一行整个显示，而你仍然
看得见你所批准的东西。

## 它在哪些地方*不*适用

- **Ctrl+O**（`tool_full_lines`、`file_cell_lines(peek: false)`）—— 展开正是整行
  所在的地方。截断它会让那些文本无处可去。（它确实共享 §5 的尾部空行裁切和 §6
  的 JSON 重塑，所以提示统计的就是它绘制的。）
- **permission prompt 的预览**（`ui::permission_view`，作用于
  `numbered_body_lines`）—— 你没法批准你看不见的东西。一个比终端还高的 prompt
  本来就会流入真正的 scrollback（`docs/ui/view-flow.md`）。
- **assistant 文本** —— 一个换行的 URL 或一段长散文就是答案本身，不是附带的输出。

## 结果

报告里那条命令，在它被捕获时的 72 列下 —— Claude Code 的单元格与我们在标题
（`docs/tools/tools.md`）和折叠上都逐行一致：

```
● Bash(curl -s --max-time 15 "https://en.wikipedia.org/w/api.php?action=
      query&prop=extracts&exintro&explaintext&format=json&titles=World%2
      0Chess%20Championship%202026"…)
  ⎿  {
       "batchcomplete": "",
       "query": {
     … +9 lines (ctrl+o to expand)
```

（单元测试 `an_exec_cell_pretty_prints_a_json_line_like_claude_code` 钉住
那些行。）一块*不是* JSON 的东西以同样的方式折叠，截断由它下面的提示暗示，
而不是在行上标记：

```
● Bash(curl -s "https://www.youtube.com/oembed?url=https://youtu.be/Vr-NEWVL
      N9Y&format=json")
  ⎿  {"title":"How to Use Burp MCP with Claude to Find Bugs from HTTP
     History","author_name":"\ud835\ude47\ud835\ude64\ud835\ude68\ud835\ude69\ud
     835\ude68\ud835\ude5a\ud835\ude58","author_url":"https://www.youtube.com/@
     … +11 lines (ctrl+o to expand)
```

三行而不是十二行，而那个数字名副其实：展开里正好有十四行，其中三行被显示。
（一个其转义会被序列化器解析掉的 oEmbed 正文过不了往返守卫，所以它保持打印出来
的样子。）

以及块预算所回答的那个多行报告 —— 同一个页面、同一个 grep，三行而不是十行：

```
● Bash(grep -i "champion" -A 5 /tmp/wiki.html | head -n 20)
  ⎿  <title>World Chess Championship - Wikipedia</title>
     <script>(function(){var className="client-js
     vector-feature-language-in-header-enabled
     … +114 lines (ctrl+o to expand)
  ⎿  Allowed by auto mode classifier
```

## 代码地图

| 部件 | 位置 |
| --- | --- |
| `TOOL_FOLD_ROWS`、`TOOL_PEEK_ROWS`、`TOOL_JSON_PRETTY_MAX_BYTES`、`FILE_PEEK_LINES` | `ui/theme.rs` |
| `WrapMode::{wrap, rows}` —— 一次扫描，两处用途；`fills_from_here` | `ui/wrap.rs` |
| 显示行（`exec_display_lines`、`prettify_json_lines`、`pretty_json_line`） | `ui/tool.rs` |
| 折叠输出块的折叠（`result_peek_block`） | `ui/tool.rs` |
| 第一个块的窗口（`BlankPolicy`、`is_blank_row`） | `ui/tool.rs` |
| 运行中尾部的 `+N lines ({secs}s · timeout …)` 时钟行 | `ui/tool.rs`（`running_command_lines`） |
| 带编号文件单元格的整行一瞥（§7）与它的上限（§8） | `ui/file_cell.rs`（`file_cell_lines`、草稿的头部、`numbered_row_lines` 的 `LineRows`、`peek_hint_line`）、`ui/assistant.rs`（`code_content_head_rows`） |
| 流式 `write` 的最新行尾部（§7） | `ui/file_cell.rs`（`draft_cell_lines`）、`ui/assistant.rs`（`code_content_tail_rows`） |

测试：`ui::tests::wrap`（计数原语与填充规则）与 `ui::tests::tool`（每一种
单元格形状 ——
`a_finished_peek_never_spends_more_rows_than_the_row_ceiling` 是块预算自己
的，`an_exec_cell_pretty_prints_a_json_line_like_claude_code` 与
`a_four_row_output_shows_whole_instead_of_hiding_one_row` 是移植的那个，
`a_read_cell_shows_a_very_long_file_line_whole`、
`the_peek_cuts_the_line_that_runs_past_its_tenth_row`、
`the_reported_minified_write_stays_inside_its_ten_rows`、
`a_cut_last_line_still_points_at_ctrl_o` 和
`an_edit_of_a_long_line_shows_both_versions_inside_the_peek` 是文件单元格
的），流式单元格的在 `ui::tests::file_cell` 里
（`a_long_line_still_arriving_shows_its_newest_rows`、
`a_finished_write_with_a_long_line_shows_the_head_the_committed_cell_will`，
以及帧预算闸门
`the_tail_of_a_huge_streaming_line_stays_inside_the_frame_budget` 与
`the_head_of_a_huge_finished_line_stays_inside_the_frame_budget`），差分
测试在 `ui::tests::assistant` 里
（`a_code_lines_tail_rows_are_the_last_rows_of_the_whole_wrap`、
`a_code_lines_head_rows_are_the_first_rows_of_the_whole_wrap`）。在屏幕上，
折叠是 Phase 38 落定的 `Bash(ping …)` 单元格（dummy backend 真实命令之下的
`… +3 lines (ctrl+o to expand)`）；Phase 92 通过 `!` 命令驱动同样的三种形状
—— 一行 750 个字符、四个换行的行、一个空白形状的输出 —— 而自
`docs/input/shell-command.md` 退役了 shell 单元格的折叠之后，它们断言的是相反
的契约：每一行内联、没有提示，且 Ctrl+O 逐行一致。§7 是 Phase 138b 的：真
backend 对着一个 stub provider 流式写一个 467 字符段落行的 `write`，再把文件读
回来，而流式单元格、提交后的 `Write` 与 `Read` 单元格都不允许用 `…` 截断一个
文件行。§8 是 Phase 138c 的：同一个 stub 流式写那份被报告的样式表并读回来，
流式单元格的任何样本都不得超过十行正文，其后的提交 `Write` 单元格和 `Read`
单元格必须各显示正好十行 —— 第十行以 `…` 结尾 —— 之上是
`… +6 lines (ctrl+o to expand)`，且压缩行的最后一个词内联无处可见，而 Ctrl+O
必须显示那个词。
