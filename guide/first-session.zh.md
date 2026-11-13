# 你的第一个 session

你装好了 Zeroth 并启动了它。这一页讲第一屏，以及把你从离线 demo 带到真模型的三
条命令。

## 第一屏

```
 ▙▄▙▄▟▄▟   Zeroth (vX.Y.Z)
▝▜▄███▄▛▘  ~/projects/site
  ▘▘ ▝▝    /login   /model   /resume
```

随着对话增长，这个横幅会滚走。它下面是 composer，再下面是页脚：左边是当前模型和
你的目录，右边钉着权限模式。

```
dummy_model_name · ~/projects/site      auto
```

在接上一个模型之前，回复来自内置的离线 demo —— 一些脚本化的 turn，驱动界面而不
碰网络。它的回答结尾会指向 `/login` 和 `/model`，那就是下一步。

## 说点什么

敲一条消息，按回车。composer 上方的状态行显示这个 turn 在跑（`esc to
interrupt` 会停下它），每次 tool call 作为一个折叠的 `●` cell 出现，输出在它
下面。

- `Ctrl+O` 打开完整 transcript，tool 输出是展开的。
- `Ctrl+D` 显示到底有什么被发给了模型。
- `esc` 打断一个运行中的 turn；部分回答和 tool 结果会留着。

默认权限模式是 `auto`：文件改动和只读命令不询问就跑，其他命令交给一个安全
classifier，它在拿不准的时候问你。真的出现提示时，回车接受，Esc 取消，而第二个
选项更宽 —— 允许本次 session 里所有的编辑，或者为这个目录记住一条命令规则。

## 接上一个模型

运行 `/login`。它会问你想怎么登录：

| 那一行 | 什么时候用它 |
| --- | --- |
| **Sign in with an account** | 你有订阅或控制台账号 —— ChatGPT、Anthropic Console、GitHub Copilot、Cline、Google Antigravity。有些会打开浏览器；有些显示一个要到某个 URL 上输入的码。 |
| **Sign in with an API key** | 你有一个 provider 的 key。输入时它被遮住，并保存到 `~/.zeroth/.env`，在你的仓库之外。 |
| **Sign in with a local model** | 你跑 Ollama 或 LM Studio。保存之前页面会先测这个地址。 |

每个 provider 行显示 `✔ configured` 或 `◯ unconfigured`；`Esc` 退出去，什么都不
改。

然后运行 `/model` 选一个模型。picker 列出那个 provider 提供的东西；打字搜索，
`↑`/`↓` 移动，回车选中。选择会为这个目录记住，页脚立刻更新。

## 干点真活

要一件具体的事：「解释这个项目是做什么的」、「跑测试并修掉第一个失败」，或者
「追踪这里的认证是怎么工作的」。agent 按需读文件、跑命令、做编辑；每一步都在屏幕
上，都可以打断。

- `/` 列出命令；`?` 列出键盘快捷键。
- `Shift+Tab` 循环权限模式：`manual`、`edit`、`auto`（默认）、`master`。页脚右
  边缘的那个词就是当前的。
- turn 运行时按回车会把这条消息交给那个 turn；`Tab` 则把它排成一个后续 turn。

## 离开，再回来

`Ctrl+C` 先清草稿；composer 空着时会退出程序。`/quit` 直接退出。

每段对话都有记录。退出时 app 会打印：

```
Resume this session with: zeroth --resume {id}
```

- `zeroth --continue` 重开这个目录里最新的对话。
- `zeroth --resume <id>` 按 id 重开一个；裸 `--resume` 打开 picker。
- 在 session 里，`/resume` 做同样的事。

## 去哪里多读一点

- [`docs/llm.md`](docs/llm.md) —— 登录、模型选择，以及每一个 provider。
- [`docs/permissions.md`](docs/permissions.md) —— 那四种模式以及批准提示。
- [`docs/resume.md`](docs/resume.md) —— session、rollout 和 picker。
