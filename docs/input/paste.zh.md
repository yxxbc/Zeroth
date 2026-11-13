# 粘贴内容的占位符

把一大段文本粘进 composer，以前是把整段倒进输入框 —— 几千个字符折行成几十行，把
框和光标都埋起来。本文讲的是那个修法：openai/codex 大段粘贴处理的一次聚焦移植。
超过阈值的一次粘贴会在 composer 里被替换成一个紧凑的占位符 ——

```
[Pasted Content 3907 chars]
```

而真正的文本记在旁边，在消息发送时拼回去。小段粘贴仍然内联插入，和以前完全一样。

> 注意：`paste` 模块有**两个**不相干的活，都叫「paste」。[`PasteBurst`
> ](../../src/paste.rs) 是*重绘合并*的突发检测器（快速输入 → 一次延迟绘制；见
> `docs/architecture/async-rewrite.md`）。这里的占位符逻辑是另一回事：它依据一个
> 真正的 **bracketed-paste** 事件，而不是一个检测到的突发。

## 一次粘贴从哪来

TUI 现在启用 **bracketed paste**（`term::init` 里的 `EnableBracketedPaste`、
`restore` 和 panic hook 里的 `DisableBracketedPaste`）。处在这个模式下的终端会把
一次粘贴包在 `ESC[200~ … ESC[201~` 里，crossterm 的 `EventStream` 把它作为单个
`Event::Paste(String)` 送达 —— 与普通打字（以及 `tmux send-keys -l`）产生的
`Event::Key` 流不同。所以：

- **一次真粘贴** → 一个 `Event::Paste` → 下面的占位符路径。
- **快速打字 / 一串按键** → 很多 `Event::Key`，逐字输入，由 `PasteBurst` 合并成
  一次重绘（没变；`smoke.sh` Phase 6）。检测器跑在紧急帧选择之前，所以突发的字符
  绝不会走按键那条 2 ms 路径（`docs/architecture/input-latency.md`）。

`main.rs` 把 `Event::Paste(text)` 路由到 `App::on_paste(&text)`（只在会话视图 ——
Ctrl+O overlay 没有 composer，就像在那里打字一样），然后排一帧并重跑 `@` token
的文件搜索派发（一次小的内联粘贴可能落在某个 `@token` 里面）。

## 模型

```rust
// App
pub pasted: Vec<(String, String)>,   // (placeholder, real text), insertion order
```

`App::on_paste`：

1. 归一化换行（`\r\n` 和单独的 `\r` → `\n`）—— iTerm2 这类终端在粘贴时发
   CR；计数和存下的文本都用归一化后的形式。
2. `char_count = text.chars().count()`（字符，不是字节 —— 与 codex 一致）。
3. 如果 `char_count > LARGE_PASTE_CHAR_THRESHOLD`（**1000**）：造出占位符
   （`next_paste_placeholder`）、`input.insert_str(&placeholder)`，并把
   `(placeholder, text)` 推到 `pasted` 上。
4. 否则内联插入文本（`input.insert_str(&text)`）—— 一次小粘贴和把它打出来
   无法区分。
5. 无论哪条路，重新推导斜杠命令 palette / shell 模式 / `@` picker 状态（每次编辑
   都跑的那三样），好让一次粘贴 `/`、`!` 或 `@…` 的小操作表现得像打字。

### 占位符字符串

`next_paste_placeholder(char_count, &pasted)`（纯的，在 `paste.rs` 里）对应
codex 的 `next_large_paste_placeholder`：

- 基础形式 `[Pasted Content {char_count} chars]`；
- 如果那个确切基础的占位符已经待定，就用一个 ` #2`、` #3`、…… 后缀消歧（已有最大
  后缀 + 1），好让两次同样大小的粘贴永远不会撞。

### 发送时展开

占位符是给 *composer* 的便利；模型必须收到真正的文本。草稿离开 composer 的每个地
方都经由 `App::take_input()`，而不是裸的 `self.input.take()`：

```rust
fn take_input(&mut self) -> String {
    let text = self.input.take();
    let expanded = paste::expand_pastes(&text, &self.pasted);
    self.pasted.clear();            // the composer is now empty
    self.redact_typed(expanded)     // a stored secret's value → its placeholder
}
```

脱敏跑在拼接之后，所以一段大粘贴里的机密值会像手打的一样，以它的占位符到达对话
（`docs/state/secrets.md` *What the user types*）。

`expand_pastes(text, pastes)`（纯的）在*原始*文本里找到每个占位符的位置，然后一
次性重建字符串，把每个占位符替换成它的真实内容（用户删掉的占位符就是找不到，所以
它的内容被丢掉）。搜索原始文本 —— 而不是就地 `replace` —— 意味着恰好包含另一个占
位符字符串的真实内容不会被二次展开。

那些地方：普通的 **Enter** 提交、`!command` 的 **shell** 提交，以及回合中途的**
队列**（`queue_draft` / `queue_shell`）全都拿*展开后*的文本，所以模型 —— 以及你
发了什么的 scrollback/历史记录 —— 拿到完整内容（用户选的行为：对话显示真实文本，
不是占位符）。Ctrl+C 清空也经过展开，所以召回的草稿永远不会带着一个悬空的占位
符。`pasted` **不**被 `/clear` 清掉（输入草稿在 `/clear` 中存活，所以它的粘贴也
存活）。

### 原子删除

一个占位符作为**一个单位**删除：一次 Backspace（或 Delete）删掉整个
`[Pasted Content N chars]`，而不是其中一个字符，并丢掉匹配的 `pasted` 条目。这住
在 `App` 里，不是 textarea —— `Backspace`/`Delete` 先向
`paste::placeholder_to_delete(text, cursor, &pasted, backward)` 要光标所在占位符
的字节跨度；有就 `input.replace_range(span, "")` 把它剪掉并丢掉它的条目，没有那
次按键就落到正常的逐字素 `delete_backward`/`delete_forward`。光标规则镜像 codex
的原子边界：Backspace 在占位符的末尾或它内部的任何位置触发（光标在它的*开头*时改
为删前面那个字）；Delete 在开头或内部触发。占位符按最长优先匹配（与
`expand_pastes` 共用），所以一个基础占位符无法遮蔽它的 `… #N` 扩展。

## 这里刻意*没有*的东西（范围）

与 `docs/input/textarea.md` 一致，占位符在 composer 里是**纯文本**，不是 codex
那种原子 `TextElement` —— 上面的删除是一个聚焦的、只针对粘贴的捷径，不是一个通用
元素模型。所以：

- 光标**没有被守住**不走进占位符，在占位符*内部*打字也不被拦（codex 的完整元素模
  型两者都防）。这样被拆开的占位符在删除或展开时就是匹配不上 —— 它的内容被丢掉。
  原子 Backspace/Delete 覆盖常见情形（粘进来，然后删掉它），而不需要那套簿记。
- 占位符**在发送时展开**，所以它永远不会作为折叠元素留在历史里；transcript 显示
  真实文本。（Codex 保留一个折叠元素；那需要历史条目携带隐藏载荷。）

两者都是可加的：任何一个都可以以后叠上去，而不改变上面的模型。

## 测试

- `paste.rs` 的单元测试覆盖 `next_paste_placeholder`（基础 + `#N` 消歧）、
  `expand_pastes`（替换、多次粘贴、一个被删掉的占位符、空时无操作），以及
  `placeholder_to_delete`（Backspace 对 Delete 的光标规则，包括一个基础/`#2` 前
  缀对）。
- `app/tests/composer.rs` 的单元测试覆盖 `on_paste`（大的 → 占位符 + 记录的那一
  对、小的 → 内联、`\r\n` 归一化、在光标处插入）、往返（一次大粘贴然后 Enter 产
  出 `Action::Submit(full_text)`），以及原子删除（一次 Backspace/Delete 清掉整个
  占位符并丢掉它的粘贴；紧挨在占位符前面的一次 Backspace 仍然只删前面那个字）。
- `smoke.sh` Phase 26 通过 tmux 驱动一次真正的 bracketed paste（`set-buffer` +
  `paste-buffer -p`），断言 composer 显示 `[Pasted Content N chars]` 而不是原始
  文本，然后发一次单独 Backspace 并断言占位符一次按键就没了（I/O 边界 —— `term`/
  `main` —— 没有单元测试）。
