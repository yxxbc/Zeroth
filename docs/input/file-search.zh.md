# `@` 文件路径提及 —— 输入框下面的文件 picker

日期：2026-06-18

## 目标

在 composer 里输入 **`@`** 会在输入框下面打开一个**文件 picker**——就像
openai/codex 的 `@` 提及弹窗那样工作，呈现成 Claude-Code 风格：输入 `@`，边打
边对工作区文件做模糊过滤，↑/↓ 选择，**Tab/Enter 把路径插入**消息（替换掉
`@token`），Esc 关闭。路径以纯文本插入——它只是变成你发送的消息的一部分（没有
特殊的 wire 编码；codex 也把原始文件路径留作字面量）。

## codex 怎么做（取自 `/tmp/codex/codex-rs`）

- **引擎**（`file-search/src/lib.rs`）：一个后台、尊重 `.gitignore` 的遍历，喂给
  **nucleo** 做模糊匹配；结果是 `FileMatch { score, path,
  indices, … }`，其中 `indices` 是匹配到的字符位置（用于高亮），有上限（默认
  20），按分数再按路径排序。
- **接线**（`tui/src/file_search.rs`、`chatwidget.rs`）：每当 `@` token 变化，
  composer 就发 `AppEvent::StartFileSearch(query)`；manager 在别的线程上跑搜索
  并发回 `AppEvent::FileSearchResult { query,
  matches }`。陈旧结果被丢掉（`current_token.starts_with(query)`）；在结果到达
  之前，一个「waiting」flag 显示 *loading…*。
- **触发**（`chat_composer.rs::current_prefixed_token_range`）：从光标往回扫过
  非空白到 token 起点；只有当那个字符是 `@` **且**它处在行首或空白之后（所以
  `email@host` 永不触发）时，它才是一个 `@` token。查询是 `@` 之后的那段。
- **状态机**（`bottom_pane/chat_composer/popup_state.rs`）：一个 `ActivePopup`
  枚举（None/Command/File/…）；文件弹窗是一条带，与斜杠命令弹窗互斥。↑/↓ 移动、
  Tab/Enter 接受、Esc 关闭（并记住被关闭的那个 token，好让它不再打开）。
- **插入**（`insert_selected_path`）：把整个 `@token` 范围替换成路径**加一个尾随
  空格**；含空白的路径加引号。

## 映射到这份代码库

这是一个**内联** TUI，它的逻辑是纯的、有单元测试的，所有 I/O 在
`main.rs`/`term.rs` 边界上。`@` picker 是输入框下面的**第三条 live 区域条带**，
与斜杠命令 palette 完全平行（`docs/ui/shortcuts.md`，palette 在
`app::COMMANDS` 里）——但它的列表来自**文件系统**，像 codex 那样**异步**取
（已定的设计：完整的异步请求/响应，不是启动索引）。

### 异步流水线（选定的设计）

镜像 codex 的 `StartFileSearch`/`FileSearchResult` 往返，适配到这个循环既有的
`select!` + 边界注入的惯用法：

```
key ─► App edits draft ─► (loop) app.file_search_query() changed?
                                   └─ yes ─► req_tx.send(query)         [loop → worker]
worker thread: recv(query) ─► coalesce (drain to newest) ─► walk afresh
              ─► file_search::rank_files(query, walk.paths)   (every match, ranked)
              ─► res_tx.send(FileSearchResult{query, matches, truncated})  [worker → loop]
loop select!: file_rx.recv() ─► app.set_file_matches(query, matches, truncated)  [stale-dropped]
```

- **Worker**（`tui::workers::spawn_file_search_worker`）：一个专用线程（不是回复
  backend——不变量 1 让那唯一的 stdin 读线程不被碰）。它拥有一个查询的
  `std::sync::mpsc::Receiver<String>`，以及一个回到循环的 tokio
  `UnboundedSender<FileSearchResult>`（它的 `send` 是同步的，任何线程都能调——
  与回复 backend 相同）。对每个请求它先**合并**（把排队的查询排到最新的一个
  ——即防抖），然后**重新**遍历 cwd 并对结果排序。这次遍历刻意按查询进行，
  *不*在启动时缓存：启动索引在 agent 创建一个文件的那一刻就陈旧了，于是一个新
  目录里的 `@` 只列得出启动时的内容（「新文件永不出现」那个 bug）。codex 通过为
  每个 `@` token 会话开一次新遍历得到同样的新鲜度；这里按查询更简单，而
  `FILE_INDEX_CAP` 的界加上合并，让重复的遍历既便宜又不占循环。
- **遍历**（`tui::workers::walk_files`）：无依赖（选定的做法）。对 cwd 做一次
  迭代式广度优先遍历，跳过**隐藏**条目（dotfile，所以 `.git` 也在内）和一个小
  **拒绝名单**（`target`、`node_modules`），封顶在 `FILE_INDEX_CAP` 以界住内存/
  时间。目录以尾随 `/` 列出。它返回一个 `Walk { paths, truncated }`，
  `truncated` 只在某个本该被列出的条目被略去时为真（见下文*A capped walk says
  so*）。每条路径**只构造一次**，即它所在文件夹的路径接上它的名字——待读的
  文件夹是列表里的位置，而不是各自的路径——每个文件夹的条目被收集起来，从不超过
  上限还有余量的部分，然后在列出任何一个之前**排成列表顺序**
  （`file_search::sibling_order`），于是遍历出来的顺序已经是一个光秃秃的 `@`
  所显示的（见下文*What a search costs*）。手工拼接保住了名字里自己的 `\`，那在
  Linux 和 macOS 上是普通字符，而一次反斜杠改斜杠的改写曾把它变成一个并不存在
  的文件夹。（与 codex 的已知分歧：不解析 `.gitignore`。）
- **派发**（`tui::workers::dispatch_file_search`）：每次按键之后，把
  `app.file_search_query()` 与上次派发的查询比较；一变就发送新查询（picker
  关闭时什么都不发）。边界状态（`last_file_query`），像 `committed`/`clocks`。
- **陈旧性**：`App::set_file_matches(query, matches, truncated)` 只在结果属于
  *当前活动*的 token 查询时应用，否则丢弃。

### 纯核心

- `file_search.rs`（新模块，镜像 codex 的 `file_search.rs`）：
  - `at_token(text, cursor) -> Option<AtToken>` —— 上面的触发规则。
    `AtToken { range, query }`：`range` 是整个 `@token` 的字节范围（接受时替换
    它），`query` 是 `@` 之后的文本。
  - `FileMatch { path: String, score: i32 }` —— 不保留查询匹配了哪些字符：那条
    带为它绘制的少数几行用 `fuzzy_match` 现算（列表持有遍历找到的每一个匹配，
    而几乎没有一个会真的上屏）。它的访问器喂那些分列的行：`is_dir()`（遍历用
    尾随 `/` 列出目录，所以类型搭在路径上）、`name()` / `name_start()`（最后
    一个分量，以及它开始的字节——把匹配字符重映射到名字列的那个偏移），以及
    `parent()`（到最后一个内部 `/` 为止的前导目录，根级条目为空）。
  - `fuzzy_match(query, candidate) -> Option<(i32, Vec<usize>)>` ——
    ASCII 大小写不敏感的**子序列**匹配（折叠用 `eq_ignore_ascii_case`，所以
    非 ASCII 字符必须精确匹配）；边界/连续/基名命中得分更高（一个目录的基名是
    它尾随 `/` *之前*的那个分量，否则每个目录都会丢掉这份加分）。空查询匹配
    一切（分数 0）。分数来自 `fuzzy_score`，那一个核心把每个匹配偏移交给一个
    回调：`fuzzy_match` 收集它们，排序什么都不收。
  - `sibling_order(a_name, a_is_dir, b_name, b_is_dir)` —— 同一个文件夹里的两个
    条目：先文件夹，再名字按大小写不敏感比较，拼写打破平局。列表顺序终结于它，
    而遍历用它对每个文件夹排序，所以两者不会漂移。
  - `rank_files(query, Vec<String>) -> Vec<FileMatch>` —— 过滤+排序（分数降序，
    再较短的路径，再字典序；空查询按下面 *A bare `@` reads like a directory
    listing* 里的目录列表顺序），**保留每一个匹配**（见下文 *Every match, five
    at a time*）。它按值取遍历的列表，并把每个匹配的路径搬进它的 `FileMatch`，
    所以对整趟遍历排序从不复制它，而给一条路径打分也不分配。
- `app/file_picker.rs`：
  - `App::file_search: Option<…>` 上的
    `FileSearch { selected, query, matches: Vec<FileMatch>, waiting,
    truncated }`（`None` = 关闭），与 `command_menu` 平行；`matches` 持有上次
    搜索返回的每一个匹配，也就是条带所滚动、所计数的那些，而 `truncated` 说明
    它们背后的遍历停在了它的上限。
  - `refresh_file_search(had_token)` —— 在 None→Some 的 token 过渡时打开，随着
    过滤器收窄夹取选中项，token 消失时关闭；Esc 关闭在同一个 token 内是**粘性**
    的（palette 的 `had_query` 技巧），在 shell 模式下被抑制。
  - `file_search_query()`（活动 token 的查询，供循环派发）、
    `move_file_selection`、`highlighted_file`、`accept_file_selection`
    （经 `TextArea::replace_range` 替换 `@token` 范围，+ 尾随空格，含空白的
    路径加引号）、`set_file_matches`（陈旧性，以及遍历的 `truncated` 放在匹配
    旁边）。
- `textarea.rs`：`replace_range(range, with)` —— 拼接，并把光标放在插入文本
  之后（新的纯方法；既有的 `insert_str` 只在光标处插入）。

### 按键派发（`App::on_key_conversation`）

picker 打开时（`file_search.is_some()`），与 palette 各分支平行：Esc 关闭、
↑/↓ 移动选中项、**Tab/Enter 接受**被高亮的文件（当有一个时——否则 Enter 落空
并提交，Tab 入队）。输入 / Backspace / Delete 编辑草稿，然后调
`refresh_file_search`（与 `refresh_command_menu`/`sync_shell_mode` 并列）。
picker 与 palette 按构造互斥（一个光秃秃的 `/token` 没有空白，所以其中任何
`@` 都不在边界上）。

### 渲染（`ui/menu.rs`）

第三条条带，共用 palette 在输入框下面的槽位：
`band = menu_rows + shortcuts_rows + file_menu_rows + skill_menu_rows`（最多
一条非零）。

- `file_menu_rows(app, width)` —— `file_menu_lines` 的**构造出行数**
  （palette 的 `menu_rows` 规则，于是在每个宽度上预留行与绘出行一致，计数器也
  随之来去）：关闭时 0；*Searching…* / *No matching files* 一行占位符（按
  `MENU_INDENT` 缩进，所以它是在名字那一列说，而不是在标记的栏里）；否则是封顶
  在 `FILE_MENU_MAX_ROWS`（**5**；更长的列表滚动）的匹配，计数器在它们下面。
- `file_menu_lines(app, width)` —— 每个匹配一行**分列**，开窗
  （`centered_window`，`/` palette 的那个窗口）以让选中项留在中间行，压在
  palette 那个 dim 的 `(n/total)` 计数器之上：

  ```text
    public      ./                                                        Dir
    assets      public/                                                   Dir
  → cv.pdf      public/assets/                                            File
    next.svg    public/                                                   File
    vercel.svg  public/                                                   File
    (3/12)
  ```

  被选中的行带 `→` 标记（`MENU_MARKER`，斜杠 palette 和 `$` picker 也穿它；
  其余在 `MENU_INDENT` 上开头，其宽度个空格）并亮成青色（palette 的惯例）。
  **名字**列是最宽的可见名字 + `FILE_MENU_GAP`；**父目录**列显示条目的目录
  （根级条目是 `FILE_MENU_ROOT_DIR` 的 `./`，更深的父目录被截断，好让行永不
  溢出）；**类型**列把 `File`/`Dir` 钉在右边缘
  （`width − FILE_MENU_TYPE_WIDTH`）。**匹配到的字符**保持强调（它们的偏移由
  `fuzzy_match` 为绘制的每一行现算，并在名字/父目录的切分上重映射——
  `file_menu_highlight`）。一个窄到放不下这些列的宽度退化成只有标记 + 名字。
  行下面挂着 palette 那个 dim 的 `(n/total)` 计数器（`menu_counter_line`），在
  名字那一列：高亮在第几个匹配上，查询一共匹配了多少个——当遍历停在它的上限时，
  `+` 收尾总数（`FILE_MENU_PARTIAL_MARK`）。一个窄到放不下整个计数器的终端
  干脆不要它，而不是把一个数字切成两半，而占位符没有东西可数。
- `render_live`/`cursor_position`/`live_height`（以及
  `tui::view::Session::live_region_height`）把 `file_menu_rows` 加进条带，于是
  picker 打开时输入框、光标和页脚原地不动——完全像 palette/快捷键那样。
- `?` 快捷键带点名那个符号，`@ for file paths`，在 `/`、`!` 和 `$` 旁边
  （`docs/ui/shortcuts.md`）。

## 每一条匹配，一次五行

日期：2026-11-11

picker 以前显示**最好的八个**匹配，再往后什么都没有：worker 给遍历找到的每个
条目排序，然后除了八个之外全丢掉（`FILE_MENU_LIMIT`），于是 ↑/↓ 在这八个里
回绕。在这个仓库里，一个光秃秃的 `@` 只够到遍历所列出的 890 个条目中的八个。
现在它全部保留，并像 `/` palette 和 `$` picker 显示它们那样显示：

- **worker 把每一条匹配发回来。** `rank_files(query, files)` 给整趟遍历排序，不
  设任何上限。遍历自己的界没变，而正是它们让「每一条匹配」有限：隐藏条目和
  `target`/`node_modules` 拒绝名单被跳过，而 `FILE_INDEX_CAP`（10,000 条）停下
  广度优先遍历，于是在 `~` 或 `/` 里启动列出的是最浅的一万条，而不是每次按键都
  走一遍磁盘。
- **一次五行**（`FILE_MENU_MAX_ROWS`），其余一个滚动之外，压在 `(n/total)` 计数
  器之上：一个查询匹配十二个文件时是 `(1/12)`，而旧列表上的计数器根本读不过
  `/8`。
- **高亮保持居中**（`centered_window`）：↓ 把 `→` 走到中间行，然后列表在它下面
  滚动，于是它上面*和*下面的文件都在视野里，只在列表那一侧到头时才滑向边缘
  （头几行锚在顶部，最后几行贴住尾部——从第一个匹配按 ↑ 回绕到底部那一行的
  最后一个）。它取代的边缘钉住的 `menu_window` 只在高亮到达底行之后才滚动，
  然后把它钉在那里，藏掉它之后的一切。

代价：`rank_files` 本来就在截断之前给每个走过条目打过分、排过序，所以一次搜索
做的工作不比以前多，而按值取遍历的列表意味着匹配的路径是被搬动，从不被复制。
变化的是匹配活过了搜索：`App::file_search` 在条带打开期间持有它们——至多
`FILE_INDEX_CAP` 条，在上限处约 1 MB 数据，典型项目里几十 KB——并在关闭时丢掉
它们。那一兆本来就已经驻留：旧排序在切到八个之前构造了同样的列表，而 glibc
会留住一个被释放列表的页。在 `/usr` 里测（遍历停在上限处），一个光秃秃的 `@`
开在 10,000 个匹配上（每个构建跑两次）：改动后常驻 16.3 和 16.4 MiB，改动前
16.4 和 16.6 MiB。之后又各搜十次，两个构建涨得一样。

考虑过而没做：遍历*一切*——隐藏条目在内，不设上限。没有 `.gitignore` 支持，
一个隐藏目录和 `.github/` 一样可能是 `.venv/` 或 `.next/`（几万个缓存和构建
文件），它们会填满上限、把项目自己的文件埋掉；而不设上限，在 `~` 里启动会每次
按键都走一遍整个家目录。要正确地找到 dotfile（`.github/workflows/ci.yml`、
`.env.example`），需要遍历尊重 `.gitignore`，那是一个单独的改动。

## 一个光秃秃的 `@` 读起来像目录列表

日期：2026-11-11

只输入 `@` 要求整个工作区，而空查询没有分数可排。它以前按最短路径优先排序，
截到八个时大致就是「顶层」——但放在每一条匹配上，它就把各层交错起来（`a/b.rs`
排在 `README.md` 前面，一个嵌套文件夹在两个顶层文件之间），读起来是噪声。现在
它按目录列表的读法列出工作区，一层一层地（`file_search::listing_order`）：

```text
→ docs         ./      Dir      the cwd's own entries first: directories,
  src          ./      Dir      then files, names compared case-insensitively
  Cargo.toml   ./      File
  README.md    ./      File
  guide.md     docs/   File     then a level down, grouped under their parents
  (1/1342)                      in the order the parents were listed above
```

父目录按分量逐个比较，所以每一层的各组跟着上一层的顺序。一个只差大小写的平局
（`B.txt`/`b.txt`）落到拼写上，这让顺序**全序**：`read_dir` 交回一个目录的条目
时没有任何承诺的顺序，而一个仅偏序的顺序会让同样的文件在两次按键之间重新洗牌。
打进去的查询照以前的排法（分数，再较短路径，再路径）。

列表顺序排起来比最短优先贵，而它跑在每一个光秃秃的 `@` 上：对 `/usr` 一次
10,000 条的遍历，它约花 22 ms，而最短优先是 6 ms——在两个不改任何顺序的捷径
之前是 55 ms，兄弟项（它们的父目录逐字节相同）跳过分量走查，而两个纯 ASCII 名字
逐字节折叠，而不是走 `char` 的 Unicode 表
（`the_bare_listing_folds_case_beyond_ascii_too` 钉住非 ASCII 的情形）。让它便宜
的是遍历现在交给它一个已经按这个顺序排好的列表：见 *What a search costs*。

### 一个被截断的遍历会说出来

遍历停在 `FILE_INDEX_CAP`（10,000 条），广度优先，于是一棵更大的树丢掉它最深的
条目——而在那里读作 `(1/10000)` 的计数器会声称那是整个工作区。遍历现在会在某个
本该被列出的条目被略去时报告 `truncated`；它会读过上限于上限之后直到碰上这样一个
条目，所以一棵恰好填满上限的树是完整的。这个 flag 搭在结果上、在匹配旁边
（`FileSearchResult::truncated` → `App::set_file_matches` →
`FileSearch::truncated`），而计数器用 `+` 收尾它的总数——`(1/10000+)`、
`(1/23+)`——因为那时总数只是一个下限。在宽度装不下它的地方，它整个被略去，
像计数器本身一样。

## 一次搜索花多少

日期：2026-11-11

一次搜索在每次按键都跑（已合并），所以它的成本在四项改动前后各测了一遍，在
`/usr` 里——那里遍历停在 10,000 条上限，最坏情形——以及在这个仓库里（890 条）。
时间是真实 `walk_files` 和 `rank_files` 之上一个一次性计时测试 21 次运行的中位
数；按键到列表是 tmux 在输入 `@` 之后轮询计数器（9 次的中位数）；内存是进程的
堆，`RssAnon`，因为 `VmRSS` 还算二进制自己的页，而那些随页缓存移动——两个相同
的空闲进程在那里差 1.5 MB，而这里差 4 KB。

| `/usr`，10,000 条 | 之前 | 之后 |
|---|---|---|
| 读文件夹 | 9.2 ms | 7–8 ms |
| 给一个光秃秃的 `@` 排序 | 22.6 ms | 2.6 ms |
| 给 `@s` 排序 | 5.0 ms | 2.2–2.6 ms |
| 给 `@lib` 排序 | 3.7 ms | 1.9–2.3 ms |
| 按键到列表，光秃秃的 `@` | 43 ms | 22 ms |
| 光秃秃的 `@` 打开时的堆 | +1,372 KB | +895 KB |
| `@s` 打开时的堆 | +2,438 KB | +1,752 KB |
| 关闭后保留的堆 | +2,438 KB | +1,316 KB |

在这个仓库里，一次搜索本来就已经是 2–3 ms，也仍然如此；那里的按键到列表是帧
调度的，两边都是 16–18 ms。

1. **每个文件夹读进来就是列表顺序。** 遍历读一个文件夹时就排序它自己的条目
   （`sibling_order`，只看名字），于是广度优先地，整趟遍历出来的就是光秃秃列表
   的顺序，而那次排序因为是自适应的，什么都找不到可挪：22.6 ms 变成 2.6 ms，
   代价约 0.2 ms 的遍历时间。列表仍然对它拿到的东西排序——遍历有序让排序便宜，
   从不让它正确。
2. **每条路径只构造一次。** 遍历每个条目分配三四个字符串、只留一个——一个完整
   路径、一份相对根的拷贝、一份反斜杠改写过的拷贝、一份文件夹的带 `/` 的拷贝
   ——现在把文件夹的路径和名字拼进它留下的那一个。
3. **排序提早停、只数一次。** 给一条路径打分不再分配（查询的字符收集一次，不
   保留偏移），单这一项就买来约 5%：时间在别处。打分循环现在停在查询最后一个
   匹配到的字符，而每个匹配的长度在排序之前只数一次，而不是在每一次分数相等的
   比较里数两次；合起来约一半。
4. **粗体字母只为绘制的行现算。** 一个匹配只保留它的路径和分数；
   `file_menu_lines` 每帧为至多五行向 `fuzzy_match` 要偏移。打进去的查询所省下
   的堆，大半在这里。

没做：为所有路径共用一个缓冲区而不是各一个字符串（剩下的最大堆项，但要为只有
超大文件夹才看得见的节省而重写遍历和条带的类型）；每个 `@` 遍历一次而不是每次
按键一次（更快，但列表打开期间创建的文件要等再打一次 `@` 才出现）；把释放的
内存还给系统（`malloc_trim` 需要 `unsafe`，本 crate 禁止）。

## 测试

- `file_search`（单元）：`at_token` 在空白/起始之后触发，在 `email@host` 内部
  不触发，能围绕光标找到 token，孤立的 `@` 得到空查询；`fuzzy_match` 是一个带
  正确下标的 ASCII 大小写不敏感子序列，不匹配时为 None；`rank_files` 先排基名/
  连续命中，平局按长度再按名字，丢掉不匹配的但保留每一个匹配，而空查询像目录
  列表一样列出——不管遍历以什么顺序找到它，都是同一个顺序。
- `src/tui/`（bin 单元测试，`cargo test --bin zeroth tui::workers`）：遍历
  列出这棵树并标出目录、跳过隐藏和拒绝名单里的条目、只在它的上限漏掉某个条目时
  报告 `truncated`、把名字里的 `\` 保留成名字的一部分，并把每个文件夹读成列表
  顺序——其输出等于一份显式列表，也等于对它的纯列表；`sibling_order` 有自己的
  单元测试。
- `textarea`（单元）：`replace_range` 拼接并把光标安置在其后。
- `app`（单元）：`@` 打开 picker 而 `file_search_query` 跟踪 token；↑/↓ 移动并
  在两端回绕；Enter/Tab 接受（`@token` 变成 `path `，光标在其后）；Esc 关闭且是
  粘性的；`set_file_matches` 丢掉陈旧结果并把遍历的 `truncated` 留在匹配旁边；
  picker 在 shell 模式下保持关闭，在 tool view 里被抑制；提交/清空/召回关闭它。
- `ui`（单元）：`file_menu_rows` 关闭时 0、占位符时 1、长列表时五行加计数器，
  且在每一个宽度上都等于绘出的行；`file_menu_lines` 把每个匹配拆成对齐的名字 /
  父目录 / 类型列（根级是 `./`，`File`/`Dir` 钉在 `width − 6` 处，一个被截断的
  深父目录保留那个钉子）、只用 `→` 和青色高亮标记被选中的行、在*两列*里都加粗
  查询匹配到的字符——为绘制的行从查询现算，查询已不再匹配的行画成普通——并在太
  窄时退化成标记 + 名字；dim 的 `(n/total)` 计数器跟着高亮、计每一个匹配、对
  被截断的遍历取一个 `+`、在宽度装不下它的地方被略去；窗口在它上方有位置之后把
  `→` 留在中间行；picker 打开时 `live_height` 按条带增长而 `cursor_position`
  原地不动。
- `src/tui/`（smoke，Phase 25）：在一个有已知文件的临时目录里启动；一个光秃秃的
  `@` 打开在 `subdir/` 上，然后是顶层文件，压在 `(1/16)` 之上，而 ↑ 回绕到嵌套
  的 `beta_smoke.txt`，压在 `(16/16)` 之上；输入 `@alpha`，picker 以分列的
  `→ name  ./  File` 布局列出那个文件，Enter 把它的路径插进 composer——然后创建
  一个文件再搜一次：按查询的遍历列出启动时并不存在的一个文件。然后十二个预埋
  文件上的 `@pick_`：五行压在 `(1/12)` 之上，三次 ↓ 把 `pick_04.txt` 放到中间行、
  `pick_01.txt` 滚出视野，而从第一个按 ↑ 回绕到 `pick_12.txt`，压在 `(12/12)`
  之上——第十二个匹配，旧的「最好八个」列表够不到它。
