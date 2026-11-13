# Telemetry

Zeroth 每天发送**一次匿名 ping**，好让项目能回答两个问题：有多少人在用它，以及
大致在哪里。这就是全部目的。它默认开启，在第一次将发送它之前的那次启动时于应用
内披露，而它由一条设置、一个环境变量，或 `DO_NOT_TRACK` 约定即可关掉。

本文档是离开你机器的东西的完整陈述。实现在 `src/telemetry.rs` 和
`src/tui/telemetry.rs` 里；这里没有一样东西与第三方分析服务通信。

## 发送的是什么

一次约一百二十字节的 `POST`，每个 UTC 日一次——在你更新应用的那天再多一次，因为
版本是字段之一：

```json
{"v":2,"id":"6f1c2a4d9e0b7c3a5f8e1d2c4b6a7980","version":"0.1.0","os":"linux","arch":"x86_64","distro":"ubuntu","os_version":"24.04"}
```

| 字段 | 它是什么 |
|---|---|
| `v` | 载荷的形状版本，好让一个后来的形状能与这个区分开 |
| `id` | **install id**：16 个随机字节的十六进制，生成一次并保存在 `telemetry.json` 里。它取自操作系统的随机源，不是从你的机器推导来的——没有 MAC 地址、没有主机名、没有用户名——所以它变不回来你。删掉这个文件，下次启动就生成一个新的 |
| `version` | 应用版本，好让我们知道哪些 release 还在被用 |
| `os`、`arch` | `linux`/`macos`/`windows` 与 `x86_64`/`aarch64`，好让我们知道该为什么构建 |
| `distro` | **仅 Linux**：`/etc/os-release` 里发行版的 `ID`——`ubuntu`、`arch`、`fedora`、`nixos`——好让我们知道该为哪些发行版测试和打包。在 macOS 和 Windows 上这个字段根本不发送 |
| `os_version` | 那个平台的版本：Linux 上是发行版自己的 `VERSION_ID`（`24.04`），macOS 上是系统的 `ProductVersion`（`15.3.1`）。平台不点名版本时不发送——比如 Arch 这样的滚动发行版——在 Windows 上也不发送 |

### 关于 `distro` 和 `os_version`

它们一起说出 `ubuntu 24.04` 或 `macos 15.3.1`，而这正是它们的全部意义：一个在
某个发行版的 glibc 上能用的构建，可能在另一个上不能用，而知道人们实际在哪些版本
上，是「猜」与「知道该针对什么构建」之间的差别。

它们来自**两个文件里的两行**，那两个文件里别的一概不读：

| 你的系统 | 从哪里读 | 取什么 |
|---|---|---|
| Linux | `/etc/os-release`（其次 `/usr/lib/os-release`） | `ID=` 和 `VERSION_ID=` 两行 |
| macOS | `/System/Library/CoreServices/SystemVersion.plist` | `ProductVersion` 这个键 |
| Windows | 什么都不读 | 两个字段都不发送 |

那些文件里别的一切都留在原处。不是 `PRETTY_NAME`，不是 `BUILD_ID`，不是
`VARIANT`，不是 `HOME_URL`；macOS 上不是 `ProductBuildVersion`（`24D70`），
不是序列号、硬件型号，也不是关于这台机器的任何别的东西——那份 plist 里没有这些，
而这里只从中读一个键。所以是 `ubuntu` 和 `24.04`，绝不是
`Ubuntu 22.04.3 LTS (Jammy Jellyfish)`。

没有一个进程被跑去查这些：两者都是文件读取，所以没有 `sw_vers`、没有
`lsb_release`、没有任何形式的子进程。

有些系统点名的没有这么多，而 ping 会照实说，而不是编一个值：

- **滚动发行版**——Arch、Void、Debian sid——没有 `VERSION_ID`，所以不发送
  `os_version`。单独一个 `arch` 比给 `arch` 挂一个数字更诚实。
- **没有 `ID`，或根本没有 os-release 文件**（一个精简容器）读作 `linux`，这正是
  [os-release
  spec](https://www.freedesktop.org/software/systemd/man/os-release.html)
  说该默认的。
- **Windows** 两个都不发送：读它的版本需要一个 registry crate 或一个子进程，而
  为这件事加哪个都不值得。

而如果某个值不合收集端接受的形状，那个字段被丢掉，ping 的其余部分不带它发送。

两个 header 随行：`Content-Type: application/json` 和
`User-Agent: zeroth/{version}`。回复被忽略。

## 永不发送的东西

没有 prompt。没有回复。没有文件名、路径或内容。没有工作目录。没有模型、provider
或 API key。没有主机名、用户名、locale、终端或 session id。没有计时，没有你用过
哪些命令或特性的记录。你的对话、你的 rollout 文件和你的输入历史永不离开这台机器。

有一个函数可读，如果你想核对而不是信我这句话——`src/telemetry.rs` 里的
`Ping::to_json` 就是整个请求 body，而
`the_payload_carries_exactly_the_seven_fields` 这个测试会在哪天加了第八个字段时
让构建失败。

## 国家，以及你的 IP 地址

ping 本身不携带任何位置。收集端在边缘从连接上读国家——Cloudflare 自己对传入请求
的查询——并且**只存那个两字母代码**（`PH`、`DE`、`US`，没有时是 `ZZ`）。你的
IP 地址从不存进数据库，也从不写进日志；收集端正是为此关掉了请求日志。

这样做是刻意的：另一种做法是让你的机器去算出它在哪，那就意味着要么带上一个地理
位置数据库，要么发送一个本不该发送的地址。

为减缓滥发，收集端还会在内存里使用连接的地址，对一个 IPv4 地址或 IPv6 网络前缀
（`/64`）施加**每分钟 60 次 ping 尝试**的限制。同一网络后面的人共用这份预算。
Cloudflare 的执行是近似的，并在每个边缘位置各自施加。

限流器只收到一个按 UTC 日变化的键控密码学摘要。收集端不保留地址，也不保留指回
它的映射，不把那个摘要存进 telemetry 数据库，不把它与 install id 关联，也不把
它发给应用。这是独立的防滥用状态，不是你的每日 ping 里多一个字段，也不是一条新
的位置记录。超出的请求在被加进一行之前就被拒绝；应用对一次被拒的 ping 保持沉默，
正如它对收集端不可用时一样。

## 什么时候发送

启动时，在第一帧绘出之后，在一个后台线程上。应用从不等待它——一个慢或不可达的
收集端不花你什么，而一次失败的 ping 是沉默的，因为没有什么你能动手的。

在你开始一个回合时也会检查，于是一个跨午夜开着的 session 算在新的一天，而不是只
算在你启动它的那天。两种情况下都是每天一个 ping：启动应用五十次，或者开着它发
一百条消息，这一天仍然只产出正好一个。

唯一的例外是你更新那天。ping 点名应用版本，所以 `zeroth update` 之后的第一次启动
用新的版本再发一次当天的 ping——在同一个 install id 之下，所以仍是一个用户，只是
在新版本上；收集端替换当天那一行，而不是再加一行。

哪一天——以及那次 ping 携带的是哪个版本——只有在收集端真的接受了这次 ping 之后才
被记下，所以离线时启动不会烧掉这一天；下一次连通的启动才算。一个一直失败的收集端
每天试一次，不是每条消息试一次。

## 关掉它

下列任何一个都够：

| 方式 | 范围 |
|---|---|
| **`/settings` → Telemetry** | 永久的，对你的用户，在每个目录里 |
| **`ZEROTH_TELEMETRY=0`** | 那一次运行（`0`/`false`/`no`/`off` 都行；`=1` 为一次运行打开它） |
| **`DO_NOT_TRACK=1`** | 那一次运行——[跨工具约定](https://consoledonottrack.com)，而它压过上面那个变量 |

当任一变量关掉 telemetry 时，`/settings` 的那一行也随之而去：它读作
`false (unavailable)` 且不会循环，所以应用里跑着的任何东西都不能悄悄把你重新
打开。从那一行*关掉*它总是有效，不管环境说什么。

```bash
ZEROTH_TELEMETRY=0 zeroth        # this run
export DO_NOT_TRACK=1                    # every tool that honours it, including this one
```

还有一种情形它完全不跑：**没有 config home**。没有 `~/.zeroth`（或
`ZEROTH_CONFIG_DIR`）就没有地方留一个 install id，而每次启动都编一个新的会把
一个人算成很多个——所以什么都不发送，而 `/settings` 的那一行读作
`false (unavailable)`。

关掉它停止发送；它不删除已经被算过的内容，因为一行
`(day, random id, country)` 里没有能认出那个会来要求删除的人的东西。

Telemetry 行是唯一**不**按工作目录的 `/settings` 旋钮。其他每一个都是关于一个
项目的事实；这个是关于你的事实，而一个悄悄只作用于你碰巧所在文件夹的退出，比
没用更糟。

## 首次运行的通知

在这个通知出现过之前，从不发送任何 ping——不在启动时，不在你把那一行打开时。
第一次将发送它的启动会说一次，在横幅下面：

```
  ╭─ Telemetry ──────────────────────────────────────────────────────────────╮
  │  Zeroth sends one anonymous ping a day to count active users.        │
  │  Shares App version, OS and connection country.                          │
  │  Never Your prompts, files, keys or IP address.                          │
  │                                                                          │
  │  Opt out /settings → Telemetry or ZEROTH_TELEMETRY=0                 │
  ╰──────────────────────────────────────────────────────────────────────────╯
```

一次不会 ping 的运行——变量或你保存的选择说关——什么都不显示，因为没有东西要
披露。

## 你机器上留着什么

`~/.zeroth/telemetry.json`，欢迎你读它、改它：

```json
{
  "enabled": true,
  "install_id": "6f1c2a4d9e0b7c3a5f8e1d2c4b6a7980",
  "last_ping_day": "2026-09-06",
  "last_ping_version": "0.1.0",
  "notice_shown": true
}
```

手工设 `"enabled": false` 与循环 `/settings` 那一行相同。删掉这个文件就重置了
安装：一个新的 id，以及通知再来一次。

## 服务器上留着什么

每个安装每天一行——即主键——而当天来自同一个安装的第二次 ping 更新那一行（更新
发出的那次 ping 把它挪到新版本上），而不是再加一行：

```sql
CREATE TABLE pings (
  day     TEXT NOT NULL,  -- the UTC date the ping arrived, by the server's clock
  id      TEXT NOT NULL,  -- the anonymous install id
  country TEXT NOT NULL,  -- two-letter code from the edge; 'ZZ' when unknown
  version TEXT NOT NULL,
  os      TEXT NOT NULL,
  arch    TEXT NOT NULL,
  distro  TEXT NOT NULL DEFAULT '',  -- Linux only; '' elsewhere
  os_version TEXT NOT NULL DEFAULT '',  -- '' when the platform names none
  PRIMARY KEY (day, id)
);
```

没有限流表，也没有请求日志。Cloudflare 的限流器保留独立的计数器，以上面描述的那个
每日摘要为键。一个每日任务删除早于 400 天的 telemetry 行，所以数据库是一个滚动
窗口，而不是一份永久历史。

## 自己核对

把应用指向一个你自己控制的收集端，看它到底发了什么：

```bash
# In one terminal — a server that prints whatever arrives:
python3 -c "
from http.server import BaseHTTPRequestHandler, HTTPServer
class H(BaseHTTPRequestHandler):
    def do_POST(self):
        print(self.rfile.read(int(self.headers['content-length'])).decode())
        self.send_response(204); self.end_headers()
HTTPServer(('127.0.0.1', 8787), H).serve_forever()"

# In another — a fresh config home, so this is a 'first install':
ZEROTH_TELEMETRY_URL=http://127.0.0.1:8787/v1/ping \
ZEROTH_CONFIG_DIR=/tmp/az-telemetry-check zeroth
```

第一个终端打印应用所发的那一行，别的什么都没有，只要你不关它。`scripts/smoke.sh`
Phase 115 就是同样的检查，自动化版。

## 把 ping 指去别处

`ZEROTH_TELEMETRY_URL` 为一次运行覆盖端点，而 `src/telemetry.rs` 里的
`telemetry::DEFAULT_ENDPOINT` 为一个构建改它。这个仓库不附带收集端。

## 更新检查

还有一个请求会自行离开你的机器：**每天一次**，在第一帧之后，应用问有没有更新的
release，好在横幅下面说出来并把你指向 `zeroth update`。它是一个单独的特性、带
单独的开关，描在 [`docs/ops/update.md`](docs/ops/update.md) 里，而它属于本文档，
因为本文档承诺是完整清单。

整个请求就是对仓库 releases 页面的一个 `HEAD`——

```
HEAD https://github.com/yxxbc/zeroth/releases/latest
User-Agent: zeroth/0.1.0
```

——而回答是一个指向 `…/releases/tag/vX.Y.Z` 的重定向，那个 tag 就是版本。**没有
body、没有 install id、没有 query string**：没有任何东西标识这个安装，也没有任何
东西被本项目记录。GitHub 看到这个连接的方式，与看到你在浏览器里打开那个页面一样，
不多也不少。它不是 GitHub API，也什么都不下载；只有 `zeroth update`，在你运行它
时，才取安装器和一个发布归档——而那条命令是你自己的请求，所以不管下面的开关怎么
说它都会跑。

它不由 `DO_NOT_TRACK` 覆盖，因为它不是追踪：那个约定是关于分析的，而这个什么都
不测量。它改为有自己的开关，因为一个你没敲过的、发往 github.com 的请求，仍然可能
是你不想要的：

| 方式 | 范围 |
|---|---|
| **`/settings` → Update check** | 永久的，对你的用户，在每个目录里 |
| **`ZEROTH_UPDATE_CHECK=0`** | 那一次运行（`0`/`false`/`no`/`off`；`=1` 为一次运行打开它） |

与 Telemetry 一样，该变量撤回 `/settings` 那一行（`false
(unavailable)`），好让应用里没有任何东西能悄悄把它重新打开，而没有 config home
时它完全不跑。留存的是 `~/.zeroth/update.json`：那个开关、上次尝试检查的那一天、
上次检查找到的最新版本，以及上次显示通知的那一天。`ZEROTH_UPDATE_URL` 把检查
指向另一个仓库——你自己的一个 fork——而 `scripts/smoke.sh` Phase 116 对着一个本地
替身服务器驱动整件事，所以你可以看着它发出的那一个请求。

## 为什么是 opt-out

一个 opt-in 的计数器测量的是那些会去找一个设置的人，而那并不是任何人真正想知道
的数字。项目以此换来的妥协，就是本文档的其余部分：载荷是五个字段、里面没有任何
私人信息，披露在应用里而不是埋在这里，关闭开关是一次按键、并尊重一个你可能早就在
全局设过的约定，而系统的两半都在这个仓库里，你可以读它们。
