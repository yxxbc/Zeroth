# 二进制大小 —— 一个 release 发布的那个二进制

每次安装都下载同一个文件：`install.sh` 解开发布归档，而那归档是
`scripts/release.sh build` 造出来的（`docs/ops/release.md`）。它的大小是每次安装
的一次下载、每台机器的一份磁盘开销，所以在改动任何东西之前先测了它，像
`docs/ops/build-time.md` 测编译时间、`docs/architecture/memory.md` 测 RSS 那样：
这些字节是什么、哪些旋钮会移动它们、以及每个旋钮在运行时花掉什么。这一页就是那次
测量、它改了什么、以及它拒绝了什么。

## 测量

这里的一切都跑在一台 x86_64 Linux 机器上（4 核、15 GB、rustc 1.94.1，即钉住的
工具链，用它自带的 `rust-lld` 链接）。基线是 `scripts/release.sh build` 以前所
构建的东西：`cargo build --release --locked --config
profile.release.incremental=false`。

- **文件与它的下载。** 二进制的字节、它的 `gzip -n -9`（`package_dist` 压缩
  资产所用的），以及——为前后对比——发布归档本身，由 `package_dist` 连同它的
  `LICENSE`、`README.md` 和 `CHANGELOG.md` 打包。
- **这些字节在哪。** `readelf -S`，分组：代码（`.text`）、只读数据
  （`.rodata`）、unwind 表（`.eh_frame`、`.eh_frame_hdr`、
  `.gcc_except_table`）、重定位数据（`.data.rel.ro` 以及一个位置无关可执行文件
  在装载时修正它所必需的 `.rela.dyn` 重定位），以及符号表（`.symtab`、
  `.strtab`）。
- **它是谁的代码。** `cargo bloat --release --crates`，它按符号名归属，所以
  被内联进另一个 crate 的代码算在那个 crate 头上。
- **一个旋钮在运行时花掉什么。** `perf_probe` 的整渲染那一列
  （`ui::message_lines` 跑在一个 50 到 1600 行的代码回答上，每个尺寸五次运行取
  平均），即 `docs/ops/build-time.md` 用来判定 `lto = "off"` 的那个代理指标。
  每个构建的探测都与其他的一起轮换跑在一台否则空闲的机器上，七轮，而表里给的是
  每个尺寸的中位数。

每个旋钮都在命令行上定义成一个 profile 来试，所以在数字到手之前 Cargo.toml 没有
改动：

```bash
cargo build --locked --profile x --bin zeroth \
  --config 'profile.x.inherits="release"' --config 'profile.x.incremental=false' \
  --config 'profile.x.lto="fat"' --config 'profile.x.codegen-units=1'
```

## 这些字节原先在哪

基线二进制，25,560,432 字节：

| | 字节 | 占比 |
| --- | --- | --- |
| 代码 | 14,081,248 | 55.1% |
| 只读数据 | 2,828,956 | 11.1% |
| unwind 表 | 2,433,028 | 9.5% |
| 重定位数据 | 1,324,944 | 5.2% |
| 符号表 | 4,811,923 | 18.8% |
| 其余一切 | 80,333 | 0.3% |
| `gzip -9` | 10,012,965 | |
| 发布归档 | 10,081,762 | |

按 crate 的代码（`cargo bloat`，13.4 MiB 的 `.text`）：

| crate | 大小 | 占比 |
| --- | --- | --- |
| `zeroth` | 6.4 MiB | 47.4% |
| `std` | 1.7 MiB | 12.5% |
| `reqwest` | 465 KiB | 3.4% |
| `rustls` | 440 KiB | 3.2% |
| `image_webp` | 386 KiB | 2.8% |
| `image` | 347 KiB | 2.5% |
| `regex_automata` | 317 KiB | 2.3% |
| `ring` | 286 KiB | 2.1% |
| `arboard` | 241 KiB | 1.8% |
| `regex_syntax` | 217 KiB | 1.6% |
| `aho_corasick` | 189 KiB | 1.4% |
| `wl_clipboard_rs` | 144 KiB | 1.0% |
| `zune_jpeg` | 135 KiB | 1.0% |
| `tokio` | 132 KiB | 1.0% |
| `toml_edit` | 131 KiB | 1.0% |

由此掉出三件事。

**一半代码是应用自己的。** 没有哪个依赖是问题：最大的 `reqwest` 和 `rustls`
各约占代码的 3%。所以够得着最多字节的杠杆是整个程序怎么优化，而不是某个依赖用
什么特性构建——而 `docs/ops/build-time.md` 已经做过的那些裁剪（`syntect` 的文件
加载器、`ratatui` 的日历、`toml` 的序列化器）把明显的未用特性都拿掉了。

**文件五分之一是符号名，而其中很多是程序被切成 codegen unit 的方式。** 26,147
个函数符号，名字中位数 91 字节；其中 3,566 个带 `.llvm.<hash>` 后缀，那是 thin
LTO 为了能让另一个单元调用而提升一个函数时加上的；7,342 个匿名常量以同样方式
各自拿到了符号（`anon.<hash>.<n>.llvm.<hash>`）；还有 15,839 个
`GCC_except_table` 标签标记着它们后面那些函数的 unwind 数据。

**hook 匹配器捎带了一整个 regex 引擎的快路径。** `regex-automata`、
`regex-syntax` 和 `aho-corasick` 合计 723 KiB 代码，为一个对着工具名试匹配的
匹配器（`src/hooks/matcher.rs`，本 crate 唯一的 regex），而其中许多是快速搜索长
文本的机器。

不需要看的：用 `include_str!` 嵌入的文件（prompts、内置 skills、
`providers.toml`）合计 146 KB，而 `two-face` 两份 0.98 MB 的语法转储里，只有
高亮器所加载的换行模式那一份被链接——没有东西调 `extra_no_newlines`，所以链接器
已经把它的转储丢掉了。

## 改了什么

### `[profile.dist]`：release 构建得到单个 codegen unit 上的 fat LTO

```toml
[profile.dist]
inherits = "release"
incremental = false
lto = "fat"
codegen-units = 1
```

`release` 逐个 crate 优化，每个切成十六个 codegen unit 由 thin LTO 连接。单个
unit 上的 fat LTO 把整个程序——每个 crate，`std` 在内——作为一个模块交给 LLVM，
于是没有调用者够得着的代码不管在哪个 crate 里都会被丢掉，被许多单元用到的泛型
不再复制进每一个，而内联跨越每一条 crate 边界。试过的 profile：

| profile | 文件 | `gzip -9` | 代码 | unwind | 重定位 | 符号 | 冷构建 | 峰值内存 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `release`（基线） | 25,560,432 | 10,012,965 | 14,081,248 | 2,433,028 | 1,324,944 | 4,811,923 | 2 分 17 秒 | 1.7 GB |
| 跨 crate 的 thin LTO | 25,195,792 | 10,026,631 | 14,264,384 | 2,233,224 | 1,273,864 | 4,523,717 | 2 分 9 秒 | 1.7 GB |
| fat LTO，16 单元 | 19,978,968 | 8,932,170 | 12,720,960 | 1,519,312 | 878,792 | 2,096,937 | 3 分 41 秒 | 2.0 GB |
| **fat LTO，1 单元** | **19,167,696** | **8,750,011** | **12,338,112** | **1,367,172** | **859,632** | **1,851,041** | 4 分 26 秒 | 1.9 GB |

（一次冷构建是从空 target 目录起的所有依赖加本 crate；峰值内存是单个进程的最大
值，取自 `getrusage`。基线的这两项在最终 manifest 上重测过，因为它第一次构建有
一段时间与另一个构建共用机器；`dist` 本身两种情况下都构建于同样的 4 分 26 秒。）

每一段都缩了，不只是代码。更少更大的函数意味着更少的 unwind 条目（−44%），而
符号表降了 62%：thin-LTO 的后缀和被提升的常量都没了，而内联留下了一半数量的函数
要命名——从 26,147 降到 12,857，留下的每一个仍有名字。跨 crate 的 thin LTO——
另一个常见设置——几乎什么都没挪动，而单个 codegen unit 值它比十六个多花的 45 秒：
0.81 MB。

而且它更快，不是更慢，这与 `docs/ops/build-time.md` 从另一边得出的发现相同：把
`release` 的 thin-local LTO 关掉让渲染路径慢了 7–19%，因为跨单元的内联随之而去，
而整程序 LTO 是那种内联的更多份：

| 代码行数 | `release` | `dist` |
| --- | --- | --- |
| 50 | 19.1 ms | 18.2 ms（−4.5%） |
| 100 | 35.3 ms | 34.0 ms（−3.8%） |
| 200 | 74.8 ms | 65.1 ms（−12.9%） |
| 400 | 146.0 ms | 135.9 ms（−6.9%） |
| 800 | 305.6 ms | 270.6 ms（−11.5%） |
| 1200 | 447.1 ms | 408.6 ms（−8.6%） |
| 1600 | 580.9 ms | 543.3 ms（−6.5%） |

（在下面那次 `regex` 裁剪之前测得，那次裁剪在渲染路径上什么都没够到。）

**为什么它自成一个 profile。** fat LTO 每次链接都重新优化整个程序，所以它不可能
增量：对 `src/update.rs` 改一个函数，在 `dist` 里重建要 3 分 41 秒。`release`
的存在是为了在一次编辑之后 ~10 秒重建（`docs/ops/build-time.md`），而每个
`cargo run --release --example …` 探测都在它里面构建；有了 fat LTO，每一个都要
付那几分钟。一个 release 每个 target 只构建一次，所以多出来的两分钟落在发布
workflow 上，不落在开发者身上。于是 `release` 仍是开发者的 profile，`dist` 是
发布的那一个，而 `scripts/release.sh build` 就是
`cargo build --profile dist --locked --target T`。二进制落在
`target/{T}/dist/`（没有 `--target` 时是 `target/dist/`），这也是
`update::is_cargo_build_dir` 对一个构建目录所知道的，所以 `zeroth update`
拒绝覆盖它（`docs/ops/update.md`）。

这些交叉编译不需要任何新东西：rustc 自己在链接之前跑 LTO，所以交给
`aarch64-linux-gnu-gcc` 和 Apple 链接器的是普通的 object 文件，而 C 代码
（`ring` 的、oniguruma 的）像以前一样以同样的静态归档到达。macOS 的大小没有在
这里测；profile 在每个 target 上都一样。

### 去掉 `perf` 的 `regex`

```toml
regex = { version = "1", default-features = false, features = ["std", "unicode"] }
```

`perf` 是六个特性——lazy DFA、one-pass DFA、有界回溯器、字面前置过滤
（`aho-corasick` 和 `memchr`）、缓存和内联——而每一个都只是让搜索更快。每个构建
都保留的 PikeVM 找到同样的匹配：那正是 `regex` crate 对它各特性的自家保证，而
一个匹配器是对着几字节长的工具名试的，那里 PikeVM 用微秒回答。`unicode` 保留，
所以 `\w`、`\d` 和 `(?i)` 的含义与以前完全一样。依赖图里没有别的东西打开
`perf`，所以这次裁剪是真的：在 `dist` 构建上，19,167,696 →
**18,599,080 字节**（−0.57 MB，其中 403 KB 是代码），`gzip -9` 从 8,750,011 →
8,532,140。`hooks::matcher` 的测试就是那个检查：匹配器选中的还是它以前选中的。

### 这把 release 留在哪里

| | 之前 | 之后 | |
| --- | --- | --- | --- |
| 二进制 | 25,560,432 | 18,599,080 | −27.2% |
| 代码 | 14,081,248 | 11,934,720 | −15.2% |
| 只读数据 | 2,828,956 | 2,698,588 | −4.6% |
| unwind 表 | 2,433,028 | 1,322,748 | −45.6% |
| 重定位数据 | 1,324,944 | 831,832 | −37.2% |
| 符号表 | 4,811,923 | 1,769,014 | −63.2% |
| 发布归档 | 10,081,762 | 8,601,400 | −14.7% |
| 冷构建，4 核 | 2 分 17 秒 | 4 分 26 秒 | +2 分 9 秒 |

归档缩得比二进制少，因为先走的是压缩得最好的那些：符号名和 unwind 表是重复的
文本，留下的代码不是。

## 测过并拒绝的

### strip 掉符号表：−1.77 MB，归档 −0.44 MB

release 刻意保留它的符号表：正是它把用户的 `RUST_BACKTRACE=1` 报告变成函数名
（`docs/ops/release.md`）。在这次改动之前，strip 是最便宜的节省——4.8 MB，文件
的五分之一——而 fat LTO 在没有丢掉一个名字的情况下拿走了其中大部分：剩下的是
二进制的 1.77 MB 和下载的 442 KB（strip 后 16,829,912 字节，gzip 后
8,089,697）。backtrace 里的名字值这个价。构建行上的 `--config
profile.dist.strip=true` 就是那个旋钮，如果哪天这更要紧。符号表从不被映射进
内存，所以它只花磁盘和下载，从不花 RSS。

### `panic = "abort"`：−2.4 MB，以及在原本有错误行的地方来一次崩溃

在 fat LTO 之上测，它是下一个最大的节省：16,792,288 字节（−2.38 MB，其
`gzip -9` −0.98 MB），主要是 landing pad 和描述它们的 `.gcc_except_table`。但
应用是刻意捕获 panic 的。hook 通知通道让每次派发跑在 `catch_unwind` 之下，好让
一个 panic 的 handler 不会把它之后每一个通知都搁浅（`src/llm/hooks.rs`）；后端的
转发线程、挂住的 `SessionStart` 运行和一个 MCP 服务器的登录探测，以
`unwrap_or_default` 和 `unwrap_or(false)` join，其中一次 panic 只花掉那一个结果；
而 Git diff 加载器把死掉的读线程变成 `/diff` 页上的一条消息。每一个都把 worker
里的一个 bug 变成降级的功能。在 `abort` 之下，每一个都结束进程、连同 session
（终端仍会被恢复——panic hook 在 abort 之前跑——但正在飞的回合没了）。一个更小的
文件，不值得在原本有一行错误的地方来一次崩溃。

### `opt-level = "s"`：再 −3.7 MB，以及更慢的渲染路径

在有 fat LTO 和单个 unit 的情况下，为大小优化把二进制带到 15,455,168 字节
（`gzip -9` 到 7,154,710）——光代码就从 12.3 → 8.6 MB。它也是 `lto = "off"`
那笔交易再来一遍，对 `release` 是这样，对 `dist` 更糟：

| 代码行数 | `release` | `opt-level = "s"` |
| --- | --- | --- |
| 50 | 19.1 ms | 21.3 ms（+11.4%） |
| 100 | 35.3 ms | 38.3 ms（+8.4%） |
| 200 | 74.8 ms | 79.4 ms（+6.3%） |
| 400 | 146.0 ms | 158.4 ms（+8.5%） |
| 800 | 305.6 ms | 320.3 ms（+4.8%） |
| 1200 | 447.1 ms | 490.2 ms（+9.7%） |
| 1600 | 580.9 ms | 661.0 ms（+13.8%） |

对一个承诺轻快的 TUI 来说，渲染路径是这笔交易错误的一边，正如对 `lto = "off"`
本会省下的编译时间那样。`"z"` 沿同一方向走得更远。按 crate 给远离渲染路径的依赖
（剪贴板和 Wayland 那几套、`toml`、`url`）设 `opt-level = "s"`，是如果大小哪天
比这更要紧时留下的杠杆；它没被采用，因为依赖只是代码的少数，每个覆盖都是对哪些
路径是冷的猜测，而好几个（图片解码、sixel 编码器、TLS）有 `perf_probe` 看不见的
代价。

### 链接设置

- **`-z pack-relative-relocs`**（`DT_RELR`）会把 `.rela.dyn`——LTO 之后 0.48 MB
  的重定位——压成几 KB，但 glibc 只从 2.36 起懂 `DT_RELR`，而 Linux 的 release
  承诺 glibc ≥ 2.35：它正是为那个底线在 Ubuntu 22.04 上构建的
  （`docs/ops/release.md`）。
- **一个非 PIE 可执行文件**（`-C relocation-model=static`）完全不需要装载时
  重定位，也得不到对它自己代码的地址空间随机化——一个安全工具不该为 0.48 MB
  拿这个加固去换。
- **`--icf=all`** 折叠相同的函数，而在 fat LTO 之后 LLVM 自己的函数合并已经在
  整个程序视野下做过了；只有 `lld` 有它，而 aarch64 的交叉构建经 GNU `ld` 链接。

### 稳定工具链做不到的

`-Zbuild-std` 加上 `panic_immediate_abort`、`-Zlocation-detail=none` 和
`-Zfmt-debug=none` 会缩小 `std` 本身，而每一个都是 nightly-only。工具链钉在一个
稳定版上（`rust-toolchain.toml`），而一个 `RUSTC_BOOTSTRAP` 构建不是 release
该做的。

### 留下的依赖

- **`image-webp`（386 KiB）** —— Ctrl+V 粘贴和 `read` 工具接受 WebP
  （`docs/input/image-paste.md`）。
- **`rustls` + `ring` + `webpki-roots`** —— 纯 Rust TLS 的立场（`Cargo.toml`）；
  系统 OpenSSL 是更小的二进制，却是一个 release 本没有的运行时依赖。
- **`arboard`、`x11rb`、Wayland 那套** —— 剪贴板，两个方向。
- **`toml_edit`（131 KiB）** —— `providers.toml` 的解析器。`toml` 0.9 不用它也能
  解析，但用的是一个新的解析器，其错误消息被启动 toast 逐字引用
  （`docs/providers/user-providers.md`）；那次升级是一个单独的改动。
- **`two-face` 的语法转储（0.98 MB 只读数据）** —— ~250 个语法，已经压缩过；
  更少就意味着高亮的语言更少。

## 它够不着什么

`cargo build --release` 和 `cargo install --path .` 像以前一样构建 `release`；
`cargo install --path . --profile dist` 安装发布的那一个。

## 重测

```bash
scripts/release.sh build                                    # the shipped binary, into dist/
B=target/$(rustc -vV | sed -n 's/^host: //p')/dist/zeroth
ls -l "$B" dist/*.tar.gz && gzip -n -9 -c "$B" | wc -c      # the file and its download
readelf -S -W "$B"                                          # where the bytes are
CARGO_PROFILE_RELEASE_INCREMENTAL=false cargo bloat --release --crates -n 30   # whose code
cargo run --profile dist --example perf_probe               # the render path, against --release
```

`cargo bloat` 是 `cargo install cargo-bloat`；它构建给它的那个 profile，而
`--profile dist` 在那里也能用，只是在 fat LTO 之下它的归属更粗。
