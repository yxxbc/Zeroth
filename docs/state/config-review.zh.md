# 在 hook 运行前审查

hook 会在事件触发时自行运行 shell 命令，不经过权限提示。用户和项目的 hook
都要先经用户审查才能运行。启动审查取代了旧的 `/trust` 闸门；项目发现与合并
规则见 `docs/state/project-config.md`。

## 什么需要决定

| 什么 | 从哪里读取 | 决定 | 记录方式 |
| ---- | ---------- | ---- | -------- |
| 每个可以运行的 hook | 用户的 `hooks.json`（`~/.zeroth/hooks.json`，或 `ZEROTH_HOOKS_FILE`）**以及**项目的 `{root}/.zeroth/hooks.json` | 信任与否 | 按文件记录 hook 本身 |

可以运行的 hook 是运行器实际会选择的处理器：有命令和受支持超时的 `command`
处理器，位于已建模事件下，没有 `if` 条件，也不使用 `powershell` shell
（`hooks::HookHandler::runnable_command`、
`hooks::HookHandler::runnable_timeout_secs`）。其他处理器无论信任与否都会带警告
跳过，因此不进入审查。skill 和 subagent 定义在加载时只是静态 markdown，不在此审查中。

## 存储：`trust.json`

`{config_home}/trust.json` 按来源文件的绝对路径保存批准，绝不放在项目里：
checkout 不能批准自己。用户文件同样使用绝对路径（`config::user_hooks_path`），
以免相对的 `ZEROTH_HOOKS_FILE` 在不同工作目录之间意外共享信任。

```json
{
  "hooks": {
    "/home/me/.zeroth/hooks.json": [
      { "event": "PreToolUse", "matcher": "bash", "command": "./guard.sh" }
    ]
  }
}
```

**只有该文件的记录中列出了完全相同的身份，hook 才受信任：**事件、matcher、
命令以及解析后的超时。缺省、空字符串和 `*` matcher 等价；其余 matcher
**保持运行器读取时的原样**（`hooks::constraint`），不去除空格。
` Bash` 与 `Bash` 选择不同命令；对话框会给带空格的 matcher 加引号
（`PreToolUse · " Bash"`）。

解析后的 `timeout` 决定 hook 能保护调用多久才失败放行，因此修改后必须重新审查。
旧记录未包含 `HookIdentity::timeout_secs` 时使用运行器的历史默认值，写入时
省略默认值。非默认超时会出现在条目标题中。`statusMessage` 仅影响显示，
不影响信任。在文件内移动 hook 不会重新提问。

同一事件和 matcher 下的已记录 hook 从文件中消失时，新身份会被标为**已更改**；
每个消失的身份按顺序与一个新身份配对。对话框显示原命令（`was: cargo fmt`）。
用户拒绝该修改后，记录同文件中的其他选择必须保留**真正的旧身份，包括旧超时**。
不能拿新超时重建旧命令，否则仅修改超时就会在未经同意时获得批准。
接受修改后才用新身份替换旧身份。

每次写入都对**做决定时磁盘上的** `trust.json` 执行读-改-写
（`trust::update_store`、边界的 `persist`），更新一个文件的 hook，不替换
其他文件的批准。移除的 hook 会被丢弃，但尚未接受修改的旧身份会保留；
因此移除后重新添加的命令需要再次审查。

有效配置部分格式错误时，存储**失败即关闭**：不信任 hook，用红色提示点名文件，
且绝不覆盖。期间的决定只在本次会话有效
（`trust.json: … — not saved, so this choice lasts this session only`）。
过时的顶层 `mcp` 键是明确的兼容例外：不解析 server 记录，直接忽略其内容，
并在下次成功写入时省略。它不能授予或撤销 hook 信任。其他未知字段、
格式错误的 hook 记录和无效 JSON 仍然报错。

### 迁移 `/trust` 批准

旧存储可能含有 `projects`：项目根 → 文件 → 批准字节的指纹。启动时，
`TrustFile::migrate_legacy` 只迁移字节仍与批准一致的项目 hook 文件，
将其 hook 记为受信任，然后删除该文件的旧条目。改过的文件和其他项目的条目保持不变。
已移除配置功能的旧哈希只是惰性数据，绝不会被当作 hook 批准读取。
用户自己的 hook 不在旧闸门下，必须单独审查。

## 审查对话框

启动横幅排队后、运行 hook 前，只要有可运行的新 hook 或修改过的 hook，
会话便打开一个 hook 页面。它替换输入框，使用 hook 选择器的边框，接管所有按键，
将隐藏光标放在高亮行上。

```
──────────────────────────────────────────────────────────────────────────
  2 hooks to review · 1 new, 1 changed
  Hooks run shell commands on their own whenever their event fires, outside
  the permission prompt. Only hooks you trust will run.

  User · ~/.zeroth/hooks.json
  ❯ [✔] Stop  changed
        cargo fmt && cargo clippy --all-targets -- -D warnings
        was: cargo fmt

  Project · ~/repo/.zeroth/hooks.json
    [✔] PreToolUse · bash
        ${ZEROTH_PROJECT_DIR}/.zeroth/hooks/guard.sh

    Trust selected and continue
    Continue without trusting

  Space to select · Enter to confirm · Esc to continue without trusting
──────────────────────────────────────────────────────────────────────────
```

每个 hook 原样显示完整命令，自动换行而不截断：危险的尾部尤其重要。
条目按来源文件分组；修改过的条目带琥珀色徽标和原命令。默认全部选中。

按键：↑/↓ 在条目和两个操作行间循环；Home/End 跳转；**Space** 切换条目；
**Enter** 在条目或首个操作上确认，在第二个操作上拒绝；**Esc** 拒绝。
启动审查中的 **Ctrl+C 退出**且不作答。在 `/settings` 中主动启用 **Hooks**
打开的审查中，Ctrl+C 则关闭页面而不作答，会话继续（`ConfigReview::requested`）。

确认会将选中的 hook 记为受信任并立即启用；未选中的仍不受信任。
拒绝不记录任何内容，也不运行所列 hook。下次启动会再次询问未信任的 hook；
可以信任、移除它们，或为此目录关闭 **Hooks** 来停止询问。

[herdr](../architecture/herdr.md) 面板将审查报告为阻塞，消息例如
`Trust hooks: Stop, PreToolUse · bash`。会话暂缓启动 hook
（`ModelSession::hold_session_start`），直到审查关闭，再用已批准的 hook
释放 `SessionStart`，并提交暂存的命令行 `[PROMPT]`。

### 页面适配

首帧审查不能流入回滚区：关闭流出的页面会清除重建，从而擦掉启动前的终端回滚内容
（`docs/ui/view-flow.md`）。页面按以下顺序逐步精简：

1. 完整页面；
2. 去掉标题下方的说明；
3. 除高亮条目保留完整命令外，其余条目折叠成单行标题和命令；
4. 缩小高亮附近的窗口，显示溢出数量，并在高亮详情中标注文件来代替分组标题；
5. 高亮详情限制为三行，再限制为一行；
6. 去掉标题，最后将提示限制为一行。

两个操作和按键提示在每一步都保留。

## 启动之后

- **`/hooks`** 显示所有已配置的 hook。未信任的显示 `⚠ not trusted` 且不运行；
  在详情页按 Enter 可以信任或撤销信任。决定会持久化并重新绑定后端 hook；
  活动轮次仍使用原有集合直到结束。
- **`/settings` Hooks** 在有 hook 等待信任时打开，会显示本审查。

## 关闭情况

- `ZEROTH_HOOKS=0`，或此目录的 **Hooks** 关闭：不审查，不运行 hook。
- `ZEROTH_PROJECT_CONFIG=0`：不加载项目层；用户 hook 仍需审查。
- 没有配置目录（无 `$HOME`、无 `ZEROTH_CONFIG_DIR`）：批准只在本次会话有效。

## 不做什么

- 信任固定的是命令和超时，不是命令读取的内容。修改受信任脚本的内容不会使其命令批准失效。
- 不逐轮扫描：hook 在启动时读取，因此修改后需要重新启动。

## 模块拆分

纯逻辑，有单元测试：

- `src/trust.rs`：`HookIdentity`、hook 审查和记录、`trusted_only`、旧批准迁移、
  项目根辅助函数和失败即关闭的存储更新。
- `src/app/config_review.rs`：对话框状态和按键。
- `src/ui/config_review_view.rs`：渲染和逐步精简布局。
- `hooks::HooksOverview`：供浏览器使用的已配置 hook 及其信任状态。
- `herdr::activity`：审查阻塞状态。

边界：

- `src/tui/trust.rs`：读取 hook 层、迁移批准、构造审查、基于当前存储记录决定、
  重新绑定 hook，以及释放启动 hook 和暂存提示。
