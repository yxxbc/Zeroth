# 横幅 mascot 与 `/mascot` picker

启动横幅在 session 元数据旁边画一个小小的 **mascot**（横幅本身见
`docs/ui/header.md`）。支持原生图形协议的终端会显示透明底的 `crest` PNG 图标
`assets/mascot-crest.png`；
其他终端、关闭图片或图标缓存写入失败时继续显示紧凑的块字符画。PNG 会内嵌在二进制
中，图形协议可用时再落到 `~/.zeroth/cache/mascot-crest-v2.png`，已安装的程序不依赖
仓库目录。这张 PNG 是 384×351——一个 18 列的小标记，更大的源图只换来更多上传：这个
尺寸下文件 78 KB，安装器的 4096 字节分块是 18 块而不是 138 块。它声明的尺寸是
`ui::MASCOT_ICON_PX`，并有测试读文件自己的 IHDR 来保证两者一致。README 用同一角色绘制在 `assets/banner-v2.jpg`。
mascot 用 **`/mascot`** 命令挑选 —— 第八个替换 composer
的内联 picker —— 选择会跨 session 保留，且**按工作目录**保留
（`docs/state/per-directory-state.md`）：戴宝石的那个 repo，旁边是戴幼雏的那个
scratch 目录。

## 目录

六个 mascot，每个是三行单宽的四分之一/半块字形：

| 名字      | 生物                                    |
| --------- | --------------------------------------- |
| `crest`   | 一只竖起冠羽的幼雏（默认）              |
| `bloom`   | 一个怒放中的圆润生物                    |
| `sprout`  | 一株顶出第一片叶子的幼苗                |
| `twin`    | 两只毛簇耳朵，共用一个笑                |
| `skiter`  | 一只多足的疾行者                        |
| `gem`     | 一颗切磨过的宝石                        |

图形住在纯的 `app::Mascot` enum 上（`name()`、`description()`、`art()`、
`art_width()`、`Mascot::ALL`）；每一行都是单宽（一个宽的 emoji/CJK 字形会把
它后面的行剪歪 —— `docs/ui/table-streaming.md` 的*宽字形* —— 目录测试把它
钉住）。
mascot 穿横幅自己的强调色 → 链接渐变（`header_gradient_start()` →
`header_gradient_end()` —— 主题的，`docs/ui/theme.md`），按图形块宽度内的显示列
作键。

## `/mascot` picker

在有空间放下列表、一段间隙和横幅的终端上 —— 在这个目录下是 56 列 —— 预览坐在
列表**旁边**，即 `/theme` picker 的布局（`docs/ui/theme.md`）：

```
────────────────────────────────────────────────────────────────────────────────
  Select mascot
  The startup banner's art — saved for this directory

  ❯

  → crest ✓     Crest
    bloom       A crested hatchling flaring its frill
    sprout
    twin         ▙▄▙▄▟▄▟   Zeroth (v0.18.0)
    skiter      ▝▜▄███▄▛▘  ~/zeroth
    (1/6)         ▘▘ ▝▝    / commands   @ files   ? shortcuts

  ↑/↓ to navigate · Type to search · Enter to choose · Esc to cancel
────────────────────────────────────────────────────────────────────────────────
```

在更窄的终端上，同一个预览叠在列表下面：

```
──────────────────────────────────────────────────────
  Select mascot
  The startup banner's art — saved for this directory

  ❯

  → crest ✓
    bloom
    sprout
    twin
    skiter
    (1/6)

  Crest
  A crested hatchling flaring its frill

   ▙▄▙▄▟▄▟   Zeroth (v0.14.0)
  ▝▜▄███▄▛▘  ~/zeroth
    ▘▘ ▝▝    / commands   @ files   ? shortcuts

  ↑/↓ to navigate · Type to search · Enter to choose …
──────────────────────────────────────────────────────
```

刻意采用 `/theme` picker 的布局 —— picker 家族的那一页（`docs/ui/view-flow.md`
的*一个 picker 的页面*）：一个 `Select mascot` 标题，压在一行说明这个选择已为本
目录保存的暗淡文字上（`docs/state/per-directory-state.md`），同一个内联框与
`❯` 打字即搜索（同时匹配名字*和*描述），各行带家族的 `→`、选中项在 `❯` 之下
显示粗体名字、其他每行都缩进在它下面，五行窗口 —— 于是六个 mascot 一次滚过一行
—— 压在 `(n/total)` 计数器上，以及一行以导航提示开头的提示 —— 全都围着 mascot
picker 存在的那个唯一理由：一份**实时横幅预览**。它由启动横幅用的*同一个*
`ui::header` 构建器构建（`header_lines_for(app, mascot, width)`，按预览列的
宽度），所以 picker 显示的东西和按 Enter 得到的东西绝不可能不一致 —— cwd、
版本、提示和渐变全都预览得分毫不差。盖在它上面的是 `/theme` 的说明文字：
mascot 的名字作为粗体标题（`Crest`），压在框的暗淡文字里的描述之上。

- **宽终端上在列表旁边，窄终端上在它下面。** 一旦终端有空间放下列表列（目录能
  画出的最宽行，12 列）、然后 `MASCOT_PREVIEW_GAP`（4，即 `/theme` 那段间隙）、
  然后 `MASCOT_PREVIEW_MIN_COLS`（40：横幅把它的图形留在标题旁边，并带上它的
  `/ commands   @ files   ? shortcuts` 提示，36 列，而每段说明都完整可读，37），
  预览就坐在列表旁边。那是 56 列，所以每台标准终端都能用。在此之下，预览在列表
  之后隔一段间隙叠起来，内缩两列。只有一个断点，而在该断点上横幅仍然安放它的
  图形、每段说明仍完整可读
  （`the_layout_switches_once_and_beside_the_list_the_banner_keeps_its_art`）。
  列表列是目录的一个常量，所以无论 ↑/↓ 还是搜索都不会让预览横向移动
  （`the_preview_column_holds_still_as_the_list_moves_and_filters`）。
- **框从不跳。** 横幅槽位被填充到任何 mascot 在该宽度下会画出的最高横幅。如今的
  目录是齐平的，但正是这段填充让任何后来加入的 mascot 也保持这一点
  （`/settings` 的“总是输出”规则）。那些 `→ {name}` 行在 session 当前的 mascot
  上带着 `/model` picker 的绿色 `✓`，而打开时高亮就落在它上面。
- **一套布局，三个 picker。** `ui::model_view::PreviewLayout` 为 `/theme`、
  `/mascot` 和 `/spinner` 一样地把预览排在列表旁边或下面，压在共用的头
  （`picker_header`）、行首（`marked_row_head`）、计数器（`list_counter_line`）
  和说明文字（`caption_lines`）之上。

并排摆开时这一页是十五行，而叠起来的是二十二行。

一个**什么都没匹配到**的搜索会收缩到它的要点 —— 占位符、一段空行、提示 —— 因为
没有列表、没有说明文字、也没有横幅可预览，而把这些槽位画成空行会在帧中间留下
一条空白带。这是 `/model` picker 的占位符规则（`model_has_detail`），而这一页
那时恰好是：

```
────────────────────────────────────────────────────────────────────────────────
  Select mascot
  The startup banner's art — saved for this directory

  ❯ ?

  No matching mascots

  ↑/↓ to navigate · Type to search · Enter to choose · Esc to cancel
────────────────────────────────────────────────────────────────────────────────
```

按键：↑/↓ 移动，**在两端环绕**（每个菜单都遵循的共用 `wrap_step` 语法）；
PageUp/PageDown/Home/End 跳（夹取）；可打印字符过滤（Backspace 弹出）；**Enter
或 Space 选定并关闭**；Esc 先清空查询，再关闭；Ctrl+C 关闭。picker 打开时占用
每一个键（在 `App::on_key` 的顶部路由，先于 composer 的全局 Ctrl+C/Ctrl+O），
粘贴被吞掉（任何人粘贴的都不可能是 mascot 名字）。它像 `/settings` 一样在
**回合中途**可用 —— 它只替换 composer，而流式条带仍把自己的行留在它上方。

在太矮、放不下这一页的终端上（旁边是列表时十五行，叠起来二十二行），picker 改为
构建家族的**紧凑**页（`docs/ui/view-flow.md` 的*在流动之前先减行的页面*）：预览
让位，说明文字和横幅一起，三个 mascot 坐在头之下预留的溢出槽位之间 —— 十二行，
所以 12 行的终端还能保住标题 —— 然后是去掉头的同一页，于是搜索行、被高亮的
mascot 和提示都还留在屏幕上。

```
──────────────────────────────────────────────────────────
  ❯

  → crest ✓
    bloom
    sprout
  ↓ 3 more below
  ↑/↓ to navigate · Type to search · Enter to choose · Es…
──────────────────────────────────────────────────────────
```

在那里把整页底部锚定会保住预览，却把搜索行和被高亮的 mascot 流进 scrollback。
`mascot_view_lines(app, width, height)` 构建第一个放得下的页面
（`first_page_that_fits`），而在比紧凑页还矮的终端上，那个页面一次减一行、减掉
剩下的东西，且从不减掉它的搜索行（`docs/ui/view-flow.md` 的*紧凑页之下*）——
`↑`/`↓` 那一对、那些横线、直到被高亮 mascot 的各行、提示 —— 所以任何高度下都
没有东西流动，而光标的位置在构建出的页面里找到搜索行（`view_prompt_row`）。

## 应用一次选择

`Action::SelectMascot(mascot)` —— 纯的那一半已经把 `App::mascot` 换了、也关掉了
picker；边界（`tui::mascot::Session::select_mascot`）：

1. **持久化** `{config_home}/mascot.json` —— 既作为**这个工作目录**自己的选择，
   也作为在任何地方做的最后一次选择，一次对该文件读-改-写
   （`tui::config::save_look`，与那里的每次写入一样尽力而为）：纯的格式是
   `app::MascotFile`，即两个 look 共用的 `app::LookFile` 的 `mascot.json` 实例
   （下面的*按目录*），
2. 抬起确认用的 `Mascot: {name}` toast，并且
3. **purge-rebuild**（`repaint_active_view`）：横幅是*scrollback 顶部*的装饰，在
   `history` 之外（`docs/ui/header.md`），所以这次重建 —— 它经 `ui::banner_tail`
   → `header_lines` 重新发出横幅 —— 是重画它的唯一办法。切换立刻可见，与一次
   resize 的重画完全一样；回合中途的切换经标准重建路径保住在飞的半截
   （不变量 3）。

启动时 `tui::bootstrap` 在第一帧提交 header 之前从 `mascot.json` 播种
`App::mascot` —— 该目录自己的条目，或者，在一个首次被启动的目录里，任何地方做的
最后一次选择，并当场**钉**为该目录自己的（`tui::config::adopt_look`）；文件缺失
或损坏时保留默认值，绝不造成启动失败。Ctrl+O 的 transcript 也用自己的
`header_lines` 作为顶部，所以它免费地显示切换后的 mascot。

## 按目录

mascot 是一个关于*项目*的事实，与一次 `/model` 选择一样，所以 `mascot.json` 遵循
`config.json` 的规则（`docs/state/per-directory-state.md`）：

```json
{
  "mascot": "sprout",
  "projects": {
    "/home/user/work/api": { "mascot": "bloom" }
  }
}
```

顶层是任何地方做的**最后一次**选择 —— 也恰好是 mascot 变成按目录之前所写的那个
单值文件，所以一个旧文件仍然能加载，并为每个目录播种。一个首次被启动的目录取用
最后一次选择并当场把它**钉**为自己的条目（`LookFile::adopt`，只在那确实改变了
什么时才写），于是一个终端里的 `/mascot` 切换绝不会移动另一个终端里的横幅；在某
个目录里做的切换既是该目录的条目，*也是*新的最后一次（`LookFile::record`）。
“一个目录”就是进程的 cwd，与每个其他按目录的文件作键的方式完全一样
（`Session::cwd`，不是 git 根）。这个格式只写一次 —— 泛型的 `app::LookFile<T>`
压在 `app::Look` trait 上（`KEY`、`name`、`from_name`），`Mascot` 和 `Spinner`
都实现了它 —— 因为这两个目录按设计就是孪生的；`MascotFile` 是它的 `mascot`
实例。`/theme` 的颜色仍然是用户的：主题是与终端匹配，不是与项目匹配。

## API

- `app::Mascot` —— 目录（`ALL`、`name`、`description`、`art`、`art_width`、
  `from_name`），`Default` = `Crest`。
- `app::MascotPicker` / `MascotRow` —— 打开的 picker 的状态与一行派生的行；
  `App::mascot()`、`set_mascot`、`open_mascot_picker`、`close_mascot_picker`、
  `mascot_rows`、`highlighted_mascot`、`on_key_mascot_picker`。
- `app::MascotFile` —— `mascot.json` 的格式：`app::LookFile` 压在 `app::Mascot`
  上（`Look::KEY = "mascot"`）—— `parse`/`to_json`、`project`、`choice_for`、
  `adopt`、`record`，压在 `last` + `projects` 两个字段上。
- `ui::mascot_view` —— `mascot_view_lines`（页面构建器；它的长度就是预留的高度，
  `docs/ui/view-flow.md`）、`mascot_picker_height`、`render_mascot_picker`；
  `ui::header::header_lines_for` 渲染预览，`ui::model_view::PreviewLayout`
  （压在 `MASCOT_PREVIEW_GAP` 和 `MASCOT_PREVIEW_MIN_COLS` 之上）把它摆在列表
  旁边或下面。
- `tui::mascot::Session::select_mascot`、`tui::config::{mascot_json_path,
  adopt_look, save_look}`。

## 测试

- `app/tests/mascot.rs` —— 目录（六个 mascot、crest 在首位、单宽图形、
  `from_name` 往返），按目录持久化的格式（旧单值文件作为最后一次选择为每个
  目录播种、一个条目压过它、`adopt` 的钉一次、`record` 的“条目兼最后一次”、
  往返之后无条目的形状与旧文件逐字节相同、宽松的解析丢掉一个未知名字而从不
  丢文件），`/mascot` 命令打开 picker，以及整套按键语法（过滤、环绕的 ↑/↓ 对上
  夹取的跳转键、Enter/Space 选择、Esc/Ctrl+C、占用每一个键）。
- `ui/tests/mascot_view.rs` —— 带框的那一页（横线、标题压在其副标题上、搜索、
  家族 `→` 之下的五行、计数器、带说明的预览、提示）在一个宽和一个窄的宽度下，
  标记与缩进，预览在一列上位于列表旁边（80 列的终端也算）而在窄终端上隔一段
  间隙叠在它下面，那一个断点上横幅的图形和每段说明都保住，预览列在 ↑/↓ 和搜索
  期间保持不动，说明文字的粗体标题压在其暗淡描述上、且 80 列下每段说明完整，
  预览跟随选择、高度稳定、活跃的 ✓ 及其位置、无匹配的占位符、没有叠出来的空白、
  宽度安全、高度契约；紧凑页（它的形状、窗口随选择移动、一旦放得下就恢复整页、
  光标在它的搜索行上）以及它之下的各级减行直到搜索行，任何高度下都没有东西流动。
- `ui/tests/header.rs` —— 横幅那一侧（`docs/ui/header.md`）；`ui/header.rs` 的布局测试
  检查图片 carrier 行、元数据位置和窄宽度回退，`images::tests` 检查原生协议门控。
- `scripts/smoke.sh` Phase 139 —— 并排页完整放进一个 12 行的终端，以及 7 行时的
  搜索行。
- `scripts/smoke.sh` Phase 86 —— picker 在真终端里端到端：从面板打开，80 列下
  预览在列表旁边，标记、说明文字和预览跟随 ↓，过滤后 Enter 切换横幅，toast
  确认，并且**对着同一个 config home 的第二个进程以切换后的 mascot 启动**
  （持久化）。
- `scripts/smoke.sh` Phase 114 —— 按目录规则端到端，`/mascot` 与 `/spinner` 一起
  测：两个目录对着一个 config home，第一个目录里的选择被记作它自己的、也是最后
  一次，第二个目录首次启动时把那次最后钉住，那里的一次选择绝不移动第一个的横幅，
  第三个目录则取用新的最后一次。
