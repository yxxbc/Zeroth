# 安全策略

## 受支持的版本

Zeroth 还在 1.0 之前：**只有最新发布**会得到安全修复。`CHANGELOG.md` 记录着本页
最近一次核对时的版本；更老的 tag 不再修补，修复随新版本发布。

| 版本 | 受支持 |
| --- | --- |
| 最新的 `vX.Y.Z` 发布 | 是 |
| 更早的一切 | 否 —— 先升级，如果仍然复现再报告 |

## 报告漏洞

**不要开公开 issue。** 使用 GitHub 的私有漏洞报告：

1. 打开 <https://github.com/yxxbc/Zeroth/security/advisories/new>（或仓库的
   **Security → Report a vulnerability**）。
2. 说明你看到了什么、怎么复现，以及你认为攻击者借此能做到什么。

维护者读到时会回复一句确认；这里没有赏金，也没有响应时限承诺，但针对权限闸门
或项目配置信任闸门的报告会被当作紧急处理，即使你不确定它是否可被利用。

## 什么算安全问题

Zeroth 在用户的机器上运行命令、写文件，所以它的攻击面比一个库大。以下在范围内：

- **绕过权限闸门。** 按模式和允许清单该询问的 `bash`/`write`/`edit`/`agent`
  调用，却没问就运行或写入（[`docs/state/permissions.md`](docs/state/permissions.md)）。
- **绕过工作区监狱。** auto/edit 模式没注意到的、落在会话工作区之外的文件改动。
- **不可信的项目配置。** 仓库的 `hooks.json`、skills、agent 定义或 `.zeroth`
  层在被信任之前就运行（[`docs/state/config-review.md`](docs/state/config-review.md)、
  [`docs/state/project-config.md`](docs/state/project-config.md)）。
- **有实际效果的提示注入。** 模型写出的内容导致用户没要求的特权动作 —— 最有用的
  是带一份能绕过 auto 模式 classifier 的复现。
- **凭据处理。** 能读到 `~/.zeroth/.env`、密文存储、登录 token，或脱敏占位符的
  途径（[`docs/state/secrets.md`](docs/state/secrets.md)、
  [`docs/providers/credential-lifecycle.md`](docs/providers/credential-lifecycle.md)）。
- **用户没要求的对外请求** —— 预览抓取、重定向、通过模型写的 URL 做的 SSRF
  （[`docs/tools/web-search.md`](docs/tools/web-search.md)）。
- **更新路径。** `zeroth update` 或发布流水线安装了它没验证过的、非发布产物
  （[`docs/ops/update.md`](docs/ops/update.md)、
  [`docs/ops/release.md`](docs/ops/release.md)）。

不在范围内：模型说了错话、用户批准过的命令按其字面执行、需要攻击者已经拿到用户
shell 的问题，以及第三方 provider 自己服务里的一切。

## 一份好的报告包含

- Zeroth 版本（`zeroth --version`）与 OS/终端，
- 使用的权限模式（页脚右边缘那个标签），以及该目录的规则是否已保存，
- 最小的复现序列 —— 一条 prompt、一个配置文件、一个 URL，
- 你期望什么、实际发生了什么。

崩溃类的报告附上 `RUST_BACKTRACE=1` 的 backtrace 会有帮助。

## 披露

请给修复一个发布的机会再写出去。报告到达后，修复与 advisory 一起准备，advisory
随带修复的那个版本一起发布；除非报告者要求匿名，否则会署名致谢。
