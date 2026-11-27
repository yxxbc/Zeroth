# 品牌标记 —— 一眼认出模型、供应商与链接

一个模型 id 是「谁在回答」的诚实名字，但它是线路上的细节。
`[openrouter]claude-sonnet-4` 花十四列说一件一个单宽标记说得更好的事，而一行
独立的 `https://github.com/yxxbc/zeroth` 把「这是个链接」说了八遍。这一页是两边
共用的那一张表：一个值点名哪个品牌、它在一行的界面上穿什么标记，以及在有多余行
的地方拿到什么样的图块。

```
  ◈ deepseek-v4.1-flash medium · ~/Projects/zeroth                 auto
  ⎇ github.com/yxxbc/zeroth
```

## 它是什么

- **纯的。** `src/brand.rs` 把一个 **provider id**、一个**模型 id** 或一个**链接
  主机**映射到一个 [`Brand`](../src/brand.rs)。这里不碰网络也不碰文件系统，所以
  同一个值在一份捕获里和在屏幕上都是同一个标记，而每一张表都有单元测试
  （`cargo test --lib -- brand::`）。
- **一个封闭的集合。** 不认识的 provider、模型家族或主机返回 `None` —— 绝不猜一个
  长得像的。调用方保留自己的兜底：页脚的 `[provider]` 括号、模型行的
  `[provider]` 标签、原始 URL。
- **单宽字形。** 每个标记都是一个显示列
  （`every_glyph_is_one_column_and_carries_no_space`）。一个宽字形会把它之后的
  每一列都推移，而页脚是按显示宽度截断的。
- **主题颜色，不是品牌颜色。** 一个标记穿一个 [`Tone`](../src/brand.rs) ——
  accent、link、success、warning、purple、text、dim —— 由
  `ui::theme::brand_color` 从当前调色板读出。因此新增一个品牌从不需要在每个主题表
  里加一种颜色（`docs/ui/theme.md`），而 `/theme` 会把这些标记连同其他一切重新上色。

## 三张表

### 模型家族

检测是词法的：一个模型 id 按 `/` 切开，每一段都拿**已知前缀里最长的那一个**去读
—— `claude`、`gpt`、`gpt-oss`、`gemini`、`gemma`、`deepseek`、`qwen`、`kimi`、
`glm`、`grok`、`mistral`、`llama`，以及 `o1`/`o3`/`o4` 这几条线，…… —— 末尾
`:free` 一类的标签先去掉。于是 `anthropic/claude-sonnet-4` 从厂商段读出 Claude，
`openrouter/deepseek/deepseek-v4` 从模型自己的名字读出 DeepSeek，而 `gpt-5.5`
读出 OpenAI。家族的判断**先于** provider：一个聚合器的括号挂在一个读者认得出自己
家族的模型前面，正是这一条存在的理由。

### 供应商

`providers.toml` 的键，转小写，再加上同一批后端答的别名 ——
`anthropic_console` → Claude、`chatgpt_codex` → OpenAI、`a0_venice` → Venice、
`cline_pass` → Cline、`ollama_cloud` → Ollama、`lm_studio`/`lm-studio` →
LM Studio，…… provider 是**兜底**：只有当模型名什么也没说（一个本地 checkpoint）
或界面点名的就是*供应商*而不是模型时，它才决定标记。

### 链接主机

一个链接的主机，按可注册后缀匹配，所以 `docs.anthropic.com` 与
`anthropic.com` 一致，而 `anthropic.com.evil.example` 不会。

## 每种形状出现在哪里

| 界面 | 形状 | 备注 |
| --- | --- | --- |
| 页脚的模型段 | 字形 + 后缀 | `◈ deepseek-v4.1-flash`；只有不认识的一对才用 `[provider]` |
| `/model` 选择器的行 | `[provider]` 标签（无标记） | 这个列表是**选择一条路由**的地方，所以 provider 必须保持可读 —— 同一个模型名可能来自好几个 provider |
| 侧栏的 `model` 事实 | 字形 + 名字 | 在 agent 视图里，用被钉住模型自己的家族 |
| 侧栏的品牌区 | **真实 logo 图块** | 只在能画图的终端上（`docs/ui/images.md`） |
| assistant 的孤立链接行 | 字形 + 可点击的主机/路径 | 取代取回式的预览卡片（`docs/ui/rich-links.md`） |
| `web_fetch` 的头 | 字形 + URL | 只对认识的主机 |

一行的界面**从不**预留图片行：标记是一个字符，页脚的截断把它算作一个。

## 图块

`assets/brands/*.png` 是 64×64 的 RGBA 图块 —— 品牌自己的颜色作为圆角底色、它的
标记为白色，尺寸记在 `Brand::LOGO_PX`。它们是**生成并提交进仓库的**，绝不在运行时
下载：

```bash
scripts/brand-logos.sh          # render every mark from the SVG set
scripts/brand-logos.sh --check  # report missing files only
```

这个脚本记录了完整的来源：哪个图标文件、哪个底色、哪个变换。它的源是
`@yldm-tech/ai-logo-static-svg`（MIT，[lobe-icons](https://github.com/lobehub/lobe-icons)
的一个 fork）—— 见 `NOTICE`。在那个集合里没有可用标记的品牌（`llama.cpp`，以及
那几个只出现在链接里的品牌）不认领图块；有富余行的地方于是画字形，绝不画一个错的
logo。

边界把图块落地到 `~/.zeroth/cache/brands/`，就在吉祥物图标旁边
（`tui::settings::materialize_brand_images`），并经由
`images::set_brand_image_dir` 发布这个目录，好让一个已安装的二进制也能画它。它们
由**既有的**图片管线绘制（`images::place_in`、`images::carrier`），也就是说它们
遵循与其他任何图片相同的规则：`/settings` 的 **Image width**、**Show images**、
终端的协议，以及那条防止全屏滚动留下残影块的滚动帧抑制
（`docs/ui/images.md`、`docs/ui/full-screen.md`）。

侧栏的图块是今天唯一的图形界面，且只在图形协议那条分支上
（`images::graphics_available`）。

`/model` 选择器**不带**标记，这是有意的。那是你选择一条路由的地方，而同一个模型名
可能来自好几个 provider（订阅的 `cline-pass/glm-5.3` 旁边是 API key 的
`glm-5.3`），所以把它的 `[provider]` 标签换成一个图标，就是拿这一行存在的唯一理由
去换一个装饰。标记属于那个问题是「我在跟什么说话」的地方，也就是页脚和侧栏。

## 落选的另一种做法

把真实 logo 画进**页脚**是第一个草图，至今仍诱人。它输在页脚答不了的三件事上：
图形协议是每个终端各自的，所以这一行在有些终端上会变高、在另一些上不会；一张图锚
定在屏幕的单元格上，而 inline viewport 会在它下面提交和滚动行；而这一行只有一个
字符高，比任何能看清的标记都小。字形是每个终端都画得出的标记，图块是在终端有行可
画时的标记。
