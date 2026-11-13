# 构建时间 —— `cargo build --release`

`cargo build --release` 是一个约 90 000 行非测试代码的 crate，压在三百四十来个
依赖单元上，而在改动任何东西之前，它是被量过的 —— 就像
`docs/architecture/memory.md` 量 RSS 那样 —— 因为那些直觉上的修法（更快的链接
器、一个 `codegen-units` 旋钮）最后发现都不是时间花掉的地方。这一页是那次测量、
它改了什么、刻意没改什么，以及怎么重新测。

## 测量

三个探针，全在同一台机器上（4 核、15 GB、rustc 1.94.1 —— 钉住的工具链，它在
`x86_64-unknown-linux-gnu` 上的默认链接器已经是自带的 `rust-lld`，所以没有链接
器可换）：

- **`cargo build --release --timings`** —— cargo 自己的逐单元报告，由
  `scripts/build_timings.py` 排名（`target/cargo-timings/` 下最新的那份，或者你
  点名的那份）：每个单元的时长和起始时间，然后是串行总和对照墙钟时间，所以机器
  实际达到的并行度也是一个数字。
- **一次热重建** —— `touch src/app/mod.rs && cargo build --release`：依赖缓存好
  之后，开发者每次编辑要付的钱，那个塑造一整天的数字。
- **`RUSTC_BOOTSTRAP=1 cargo rustc --release --lib -- -Ztime-passes`** —— rustc
  自己逐 pass 给这个库计时，它把 crate 自身的编译拆成前端（解析、类型检查、借用
  检查、元数据）和 LLVM 后端（codegen、优化、那一趟 thin-local LTO）。
  `RUSTC_BOOTSTRAP` 为了测量在稳定工具链上解锁了一个 `-Z` 标志；它也会给每个构
  建脚本和 proc macro 重新打指纹，所以要为两侧的重建留出预算。

运行时用 `cargo run --release --example perf_probe` 检查过：它的整渲染那一列
（`ui::message_lines` 处理一段 50 到 1600 行的代码回复，每个尺寸五次取平均）是
markdown + 语法高亮路径的一个公平的 CPU 代理，而下面有两个旋钮是拿代码质量换编
译时间。

## 时间花在哪儿

冷构建，任何改动之前：

| | |
| --- | --- |
| 墙钟 | **3 分 00 秒** |
| CPU（`user`） | 9 分 41 秒 |
| 单元 | 342（串行总和 568 秒，4 核上并行度 3.15×） |
| 这个 crate | 库 **43.9 秒**（从 127 秒开始），二进制 **8.7 秒**（从 171 秒开始） |
| `touch src/app/mod.rs` 之后的热重建 | **52.7 秒**（库 43.8 秒 + 二进制 8.7 秒，一个核忙着） |

依赖单元，按它们在图里被谁持有分组（CPU 秒，取自报告）：

| 族 | 秒 | 是什么 |
| --- | --- | --- |
| TLS：`rustls`、`ring`（一个 C + 汇编的构建脚本）、`webpki-roots` | 46 | 带 `rustls-tls` 的 `reqwest` |
| `image` + 各编解码器（单 `image` 就 27.8） | 44 | Ctrl+V 粘贴、inline 图片 |
| Wayland 那一套：`wayland-protocols`（13.0，每个协议都经 `wayland-scanner` 生成）、`wl-clipboard-rs`、`tree_magic_mini`、…… | 44 | `clipboard::linux`、arboard 的 `wayland-data-control` |
| `moxcms`（24.3）+ `pxfm`（16.9） | 41 | 色彩管理，`image` ≥ 0.25.7 的硬依赖 |
| 正则引擎：`regex-automata`（19.6）、`regex-syntax`、`aho-corasick`、`fancy-regex` | 41 | 各 hook 的匹配器（以及测量当时 token 计数器的 o200k 模式 —— `fancy-regex` 后来随它一起走了，`docs/architecture/token-estimate.md`） |
| sixel 量化器：`quantette`、`palette`、`wide`、`rand 0.10`、`bytemuck_derive` + `syn 3`、…… | 34 | `ratatui-image` → `icy_sixel`，无条件 |
| `toml` + `toml_edit` + `winnow` | 16 | `providers.toml` |
| `x11rb-protocol`（11.6）+ `x11rb` | 14 | X11 剪贴板 |
| `reqwest` | 14 | HTTP client |
| `onig_sys`（一个 C 构建，9.8）+ `onig` | 11 | 高亮器的正则引擎 |
| `syntect` 的文件加载器：`plist`、`quick-xml 0.41`、`yaml-rust`、…… | 10 | `default-onig`，**没用到** |
| `tokio` | 10 | 事件循环、reqwest 的运行时 |
| `arboard` | 10 | `/copy`，非 Linux 的粘贴 |
| 经 `url` → `idna` 的 ICU4X：二十个小 crate | 9 | `reqwest` 反正都要 `url` |
| `time` 那一族 | 6 | `ratatui` 的日历 widget 和 `plist`，**没用到** |

由此显出三件事。

**依赖跑满所有核；这个 crate 只跑一个核。** 依赖单元在约 130 秒处结束，机器忙
着约 3.2×，然后库大约独自编译 44 秒 —— rustc 的前端是单线程的，只有 codegen 会
展开 —— 之后二进制再花 9 秒。Cargo 会把库 crate *流水线*起来（一个依赖方在
codegen 完成之前就从 `.rmeta` 开始）但从不让一个二进制对着它自己的库这么做：一
个要链接的目标需要成品 rlib（`requires_upstream_objects`），所以 `src/tui/` 那
约 11 500 行严格排在库的约 80 000 行之后编译。在四个核上，这个 crate 占冷构建
的 30%，却占热构建的全部；在十六个核上，它也会是冷构建的大部分。

**库的时间里有五分之二是 thin-local LTO。** rustc 自己给这个库的各 pass
（`-Ztime-passes`，墙钟秒）：

| pass | 秒 |
| --- | --- |
| `LLVM_thinlto` | **16.7** |
| `LLVM_passes` | 11.9 |
| `generate_crate_metadata` | 5.5 |
| `MIR_borrow_checking` | 3.3 |
| `type_check_crate` | 2.8 |
| `codegen_to_LLVM_IR` | 2.4 |
| `monomorphization_collector_graph_walk` | 1.9 |
| 展开、解析、coherence、lint、…… | ~2 |
| `link`（rlib） | 0.1 |
| 合计 | 42.3 |

前端是其中的约 16 秒。`lto` 不设时，一次 release 构建仍然跑 **thin LTO 跨这个
crate 自己的十六个 codegen 单元**（cargo 的 `lto = false` 意思是「局部
thin」，不是「没有」），而对这么大的一个 crate，仅那一趟 pass 就要多花 16.7
秒，压在优化器花在单元本身上的 11.9 秒之上 —— 而一次非增量构建会为了改一行把
这一切从头重做，包括前端。

**有三个族是被没人用的 feature 拖进来的。** `syntect` 的 `default-onig` 带来
了 `plist`、`yaml-rust` 和第二个 `quick-xml`，用来从*文件*加载语法和主题 —— 这
个 crate 从不这么做，它的语法是 `two-face` 预编译的转储（`highlight`）。
`ratatui` 的默认 `all-widgets` 带来了日历 widget 和随之而来的 `time` 族，而
`toml` 默认的 `display` 那一半，为一个只被读取的文件编译了序列化器。

## 改了什么

### 依赖 feature（行为不变）

| crate | 原来是 | 现在是 | 去掉 |
| --- | --- | --- | --- |
| `syntect` | `default-onig` | `parsing`、`regex-onig` | `html`、`plist-load`（`plist`、`quick-xml 0.41`）、`yaml-load`（`yaml-rust`、`linked-hash-map`）、内置的默认语法/主题转储 |
| `two-face` | `syntect-default-onig` | `syntect-onig` | 同上 —— 它只是转发 `syntect/default-onig` |
| `ratatui` | 默认 | `crossterm`、`underline-color`、`layout-cache` | `all-widgets` → `widget-calendar` → `time`、`time-macros`、`deranged`、……；`macros` → `ratatui-macros` |
| `toml` | 默认 | `parse` | `toml` 和 `toml_edit` 的 `display` 那一半 |

`parsing` 本来就蕴含 `dump-load`（以及 `flate2` + `bincode`），`two-face` 内嵌
的转储正是经它反序列化的，而 `two-face` 自己也只向 syntect 要
`parsing` + `dump-load`；`syntect-onig` 是让它选择 **onig 编译版** 转储而不是
fancy-regex 版的那个（两者是不同的二进制，引擎编译语法的方式不同），所以
`Cargo.toml` 和 `highlight` 里关于高亮器内存的那套理由原样不动。
`underline-color` 在这里不是可选的 —— `links` 和 `images` 把
`underline_color` 通道当作它们的载体 —— 而 `layout-cache` 留着，因为去掉一个
不花什么钱的默认值什么都换不来。

十四个 crate 离开了 release 图（343 → 329 个单元；`Cargo.lock` 少了 71 行），
`syntect` 自己从 11.1 秒降到 7.6 秒编完，`toml_edit` 卸掉了它的序列化器。一次
冷构建测得 **2 分 54 秒，对 3 分 00 秒**，串行总和降了 27 秒（568 → 541 秒）：
一点不大的节省，因为走掉的是那些和上面几个重族并行编译的小单元，也因为下面的
profile 把其中几秒花回了这个 crate 自己的首次构建上。这些裁减留着是为了它们本身
—— 少编译、少链接、什么都没丢 —— 不是为了墙钟。

### `[profile.release]`

```toml
[profile.release]
incremental = true
```

热重建是塑造一整天的那个数字，而它曾经是改一行 52.7 秒 —— 库的 44 秒（前端和
LLVM 都从零来）加上二进制的 9 秒。增量编译打开后，rustc 在两次运行之间保留依赖
图和查询结果，并复用每次编辑没触到的每一个 codegen 单元的机器码（为此它划分得
更细，最多 256 个单元，并按单元缓存 thin-LTO 的 import），所以只有改动的单元和
它们的 importer 被重新优化。同样的编辑、依赖已缓存，实测：

| 重建 | 之前 | 之后 |
| --- | --- | --- |
| crate 的首次构建（写缓存） | 52.7 秒 | 59.3 秒 |
| `touch src/app/mod.rs`，什么都没改 | 52.7 秒 | 4.4–5.8 秒 |
| 给 `ui::wrap_text` 加一条语句（每个渲染器都调它） | 52.7 秒 | **9.1 秒** |
| 给 `App::on_key` 加一条语句 | 52.7 秒 | **10.0 秒** |
| 撤销其中任意一个 | 52.7 秒 | 8.9–10.4 秒 |

约 5 秒的地板是一次增量构建跳不掉的工作 —— 重新解析这个 crate、重新编码它的元
数据、链接那个 23 MB 的二进制 —— 而这正是拆 workspace（见下）会去攻的那部分。

它的代价。首次构建慢约 10%（要写缓存；冷构建里这个 crate 花 46 秒而不是 44
秒）。带缓存的 `target/` 从 792 MB 长到 1.4 GB，二进制大了 0.3%（22.76 →
22.83 MB）。而交付出去的代码**在噪声范围内没有变化**：同一条 `perf_probe` 整渲
染列，普通构建对增量构建 ——

| 代码行数 | 普通 | 增量 |
| --- | --- | --- |
| 50 | 32.9 ms | 33.1 ms |
| 100 | 61.6 ms | 62.2 ms |
| 200 | 123.8 ms | 123.9 ms |
| 400 | 251.4 ms | 247.7 ms |
| 800 | 526.8 ms | 506.1 ms |
| 1200 | 750.5 ms | 770.2 ms |
| 1600 | 983.4 ms | 1011.3 ms |

—— 每个尺寸五次取平均，每一处差别都在 ±3% 之内且两个方向都有，这正是跨更细单
元的 thin LTO 换回来的东西。这个设置只作用于**这个 crate**：cargo 从不增量编译
registry 里的依赖（在它之下的首次构建重编了一个包，依赖单元与之前逐字节相同），
这也是 `cargo install --path .` 只付首次构建那点开销的原因。一次无论如何都想要
普通布局的构建 —— 一个 CI 任务、一个只构建一次的发行版软件包 —— 在命令行上要
它就行，不用改 manifest：

```bash
cargo build --release --config profile.release.incremental=false
```

发布用的归档本身是在它们自己的 profile 里构建的，见下。

**`lto = "off"` 量过了，被否决了。** cargo 对 release 的默认不是「不做 LTO」，
而是跨每个 crate 自己那些 codegen 单元的 thin LTO，而关掉它是唯一会碰到*依赖*
编译的旋钮：

| | `lto` 不设 | `lto = "off"` |
| --- | --- | --- |
| 冷构建 | 3 分 00 秒（CPU 9 分 41 秒） | **2 分 22 秒**（CPU 7 分 49 秒） |
| 库 | 43.9 秒 | 33.9 秒 |
| 热重建，非增量 | 52.7 秒 | 42.4 秒 |
| 热重建，增量，一次编辑之后 | 7.3 秒 | 5.9 秒 |
| `perf_probe`，50 → 1600 行 | — | **+11.6%、+18.8%、+9.8%、+7.2%、+1.9%、+10.5%、+9.5%** |

冷构建省下五分之一，而渲染路径在每个尺寸上都慢 7–19% —— 每个 crate 都失去了它
那趟 thin LTO 提供的跨单元内联，而 syntect + 换行这条路正是最能察觉这件事的代
码。对一个承诺轻快的 TUI 来说，冷构建是这笔交易错误的那一边；数字记在这里，免得
谁再去量第二遍。

### `[profile.dist]` —— 交付构建不是这一页说的那个构建

发布归档不再在 `release` 里构建。`scripts/release.sh
build` 在一个它自己的 profile 里编译它们 —— `release` 加上
`lto = "fat"` 和 `codegen-units = 1`，绝不增量 —— `docs/ops/binary-size.md` 为
它交付的东西量过这个 profile：二进制小 27%，渲染路径*快* 4–13%，把上面
`lto = "off"` 拿走的跨单元内联、按整个程序给出了。它花的正是这一页讲的那个东
西。在同一台 4 核机器上，一次冷构建花了 **4 分 26 秒**，而 `release` 是 2 分
17 秒，它最大的进程约 1.9 GB，依赖做完之后整个程序的 LTO 在一个核上作为一个任
务跑。而它不可能是增量的：fat LTO 每次链接都重新优化整个程序，所以若作为
`release` 的设置，一次编辑后那约 10 秒的重建会变回几分钟。这就是要两个 profile
而不是一个的原因：`release` 是开发者的，每次编辑重建；而 `dist` 是发布流程的，
每个发布每个目标构建一次。

## 什么留着，为什么

报告里其他几个大族各自由应用需要的东西钉在原地，所以这里把它们记下来而不是裁
掉：

- **`moxcms` + `pxfm`（41 秒 CPU）** —— `image` ≥ 0.25.7 无条件依赖这个色彩管
  理 crate，花时间的是它的 SIMD shaper 路径；feature 是叠加的，所以依赖方关不掉
  它们。唯一不带它的 `image`，0.25.6，要 `png 0.17`，而 `images::fitted` 是对着
  `png 0.18` 解码器写的 —— 钉回去会编译两个 `png`，也毁掉 `Cargo.toml` 给直接
  依赖 `png` 的那套「已经在树里了」的理由。
- **sixel 量化器（约 34 秒 CPU）** —— `ratatui-image` 无条件依赖 `icy_sixel`
  （一直到 11.0.8，没有能去掉它的 feature），而 sixel 是
  `docs/ui/images.md` 支持的三种图形协议之一。它也是把第二个 `rustix`（0.38，4
  秒）和一个 `thiserror 1` 留在图里的东西。
- **Wayland 那一套（44 秒 CPU）** —— `clipboard::linux` 流式的 `image/png` 读
  取直接建在 `wl-clipboard-rs` 上，而 arboard 的 `wayland-data-control` 是让
  `/copy` 在 Wayland 桌面上能用的东西。
- **TLS（46 秒 CPU）** —— 这个 crate 的纯 Rust 立场（`reqwest` 配
  `rustls-tls`）；`native-tls` 编译起来更便宜，却会带来一个系统 OpenSSL。
- **正则引擎（41 秒 CPU）** —— 测量时 token 计数器的 o200k 切分模式还需要带完
  整 Unicode 表（`\p{L}`、`\p{N}`）的 `fancy-regex`。现在计数器是一个免词表的
  估算（`docs/architecture/token-estimate.md`），`fancy-regex` 也走了，但每一个
  `unicode-*` feature 仍然是一张 `regex-automata` 为各 hook 匹配器的 `regex`
  编译的表 —— 那个 `regex` 是同一个引擎上的一层薄包装。
- **`x11rb-protocol`（12 秒）** —— 核心 X11 协议作为生成的代码；没开任何扩展
  feature。
- **`onig_sys`（10 秒，一个 C 构建）** —— 刻意的 oniguruma 选择（`highlight`：
  fancy-regex 时 RSS 187 MB，对约 23 MB）。
- **经 `url` 的 ICU4X（二十个小 crate 共 9 秒）** —— `reqwest` 反正要 `url`；
  文档里写的 `idna_adapter = "=1.1.0"` 钉法会把 ICU 换成
  `unicode-normalization`，但为了九个 CPU 秒 —— 还分散在那些与重族并行编译的
  crate 上 —— 对一个传递依赖钉死精确版本，不是一笔值得做的交易。

## 结构上的那个选项

剩下的杠杆是这个页面没有拉的那一根：库是单个 crate，所以它的前端 —— 44 秒里的
约 16 秒 —— 在一个核上串行，另外三个核闲着，而哪怕一次增量重建也要重跑整个
crate 的解析、元数据和链接（下面那约 5 秒的地板）。**拆 workspace** 会让独立模
块的前端并排跑，也会让一次编辑只重编它自己的 crate：`tokens`、`markdown` +
`highlight`、`textarea`、`file_search`、`session` + `history`，以及 `llm` 的各
线格式，每一个都只能通过 `tests/api_surface.rs` 锁住的 `crate::` 路径到达，所
以每一个都是候选叶子。这是一个设计改动，不是一条构建设置 ——
`docs/architecture/module-layout.md` 里的模块地图、每份文档都点名的那些
`crate::app::X` 路径，以及 API 表面测试，都会随它一起动 —— 所以它记在这里作为
下一步，而不是现在就做。

## 重新测

```bash
cargo build --release --timings && scripts/build_timings.py           # cold: rank the units
touch src/app/mod.rs && time cargo build --release                     # warm: the per-edit cost
RUSTC_BOOTSTRAP=1 cargo rustc --release --lib -- -Ztime-passes 2>&1 \
  | grep '^time:' | sort -t: -k2 -rn | head                            # the library's passes
cargo run --release --example perf_probe                               # the runtime check
time scripts/release.sh build                                          # the shipped build (docs/ops/binary-size.md)
```
