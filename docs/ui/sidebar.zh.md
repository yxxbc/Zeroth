# 侧栏

拥有屏幕的模式**右边**那一列：品牌、会话事实，以及正在跑的东西 —— 这些在内联
viewport 里分散在 banner 和页脚之间。它存在的理由和这个模式的其余部分一样：内联画完的一行归终端
所有，那里没有一列可以放东西。

```
❯ why does the parser reject this line               │Zeroth v0.21.0
                                                      │a coding agent that lives in
● The lexer peeks one token ahead, and the tag it     │your terminal
  records is the one the parser sees later.           │
                                                      │session
                                                      │model deepseek-v4 · openrouter
                                                      │cwd ~/Projects/zeroth
                                                      │context ████░░░░ 12.4k/160k
                                                      │mode manual
                                                      │
                                                      │running
                                                      │▸ add the sidebar
                                                      │· write its docs
                                                      │◯ explore: find the callers
                                                      │⌁ 1 running
```

**它是按需打开的，默认关着。** 这一列是 `/settings` 里的一行（**Sidebar**）——
一次运行可以用 `ZEROTH_SIDEBAR=1` 打开 —— 因为新会话应该先把每一列都给对话，直到
读者自己要那块额外的面板。这一行关着时，外框的布局和这个功能存在之前完全一样：没
有一列、没有接缝，品牌坐在 banner 上。

**它在右边**，这样对话 —— 以及输入框的光标 —— 从**第 0 列**开始，也就是内联 composer
和 shell 自己的提示符本来就在的地方。放在左边会把整个 composer 推进三十列：光标停在
提示符不在的位置，退出后的 shell 提示符也不再和对话对齐。


## 三档响应式

这一行打开之后，这一列遵循每个终端侧栏都守的惯例 —— 宽屏给全尺寸，中间档收窄，
对话更需要空间时直接消失。宽度是**整列**，连它那一格接缝算在内：

| 终端宽度 | 侧栏 |
| --- | --- |
| ≥ 112 列 | 30 列 |
| 96–111 | 24 列 |
| < 96 | 不显示 |

这些档位是 [`src/ui/sidebar.rs`](../../src/ui/sidebar.rs) 里的常量
（`WIDE_SCREEN_COLS`、`MIN_SCREEN_COLS`、`WIDE_COLS`、`NARROW_COLS`），通过两个函数
（[`ui::sidebar_width`](../../src/ui/sidebar.rs) 是档位策略，`ui::sidebar_columns` 是
「这些档位，除非那一行关着」）读取 —— 画面和对话的换行宽度都调用它，所以一列不可
能在行没有按它重建的情况下出现。这一点要紧，因为换行宽度是帧缓存的
键的一部分：一个自己冒出来的侧栏，会把读者正在读的每一行重新折行。

隐藏的侧栏对对话**没有任何代价**：中间档是收窄这一列而不是裁掉它，这正是它存在、
而不是让一列一路缩到零的理由。

接缝在这一列的**左边**，一条压暗的竖线把它和对话隔开；各小节的行排在剩下的宽度里
（`SIDEBAR_SEAM_COLS`）。

## 小节是一个注册表

模块化就是重点。一个小节是 `SECTIONS` 里的一条：

```rust
pub struct Section {
    /// Stable id — what a settings row, a test or a log names it by.
    pub id: &'static str,
    /// The rows for a `width`-column sidebar; empty means "nothing to say".
    pub rows: fn(&App, u16) -> Vec<Line<'static>>,
}
```

堆叠、间距和截断都在一处（[`sidebar::lines`]）；一个小节只回答「我的行是什么」。
加一个模块 —— 一份 diff 摘要、一个 git 状态、一份 token 账 —— 就是新增一个 `fn`
加一条注册，永远不用改布局。没什么要说的小节返回空行，并被**整节**丢掉，连标题一起，
所以这一列永远不会显示一个空标题。

栈按重要程度排序，并按屏幕高度从顶部开始裁剪：品牌是最后才会消失的东西。

## 今天每个小节装什么

- **`brand`** —— 终端能显示图形时是 mascot 的图像（用 banner 自己的 placement，按这一列
  的尺寸缩放；终端没有图形时宁可丢掉也不降级成乱码），然后是名字、版本、一行 slogan，
  以及在离线 demo 上 banner 也带的那句 `Connect a model → /login`。
- **`session`** —— 模型（在 agent 会话视图里是该 agent 自己的）、工作目录、一个带数字的
  上下文表、权限模式。页脚那条链，竖着读。
- **`running`** —— 任务清单（进行中 `▸`，等待 `·`，已完成不列）、运行中的子 agent，
  以及后台 shell 的计数。内联模式挤在状态条和名册里的那些东西。

## 代价

画面在排版任何东西之前先预留这一列，所以对话的换行宽度 —— 因而每一行缓存 —— 都取决于
它。这就是为什么可见性只是那个设置和屏幕宽度的纯函数、别无其他：没有藏起来的会话状
态，同一块屏幕配上同样的设置永远画出同一列。切换那一行会改变换行宽度，而它本来就是帧
缓存的键的一部分，所以下一帧自己会重新折行。

侧栏显示时**不再**画 banner：侧栏**就是**那个品牌块，画两遍等于用两倍的行走同一件事。
对话于是从第 0 行开始，没有 banner 也没有空行（`ui::banner_tail` 对空 banner 原样返回
它的尾部）。

## 覆盖范围

- `ui::tests::sidebar` —— 三个档位、注册表（小节的缺席不会留下标题）、品牌的措辞、
  事实行、列内截断、什么算「正在跑」、高度裁剪，以及任务的标记。
- `ui::tests::sidebar::the_column_is_opt_in_and_the_setting_gates_the_bands` ——
  默认关着，每个档位都由那个设置把关。
- smoke 阶段 156 —— 一行关着的 140 列窗格断言没有这一列、标题在 banner 上；随后一个
  以 `ZEROTH_SIDEBAR=1` 启动的窗格驱动 140 和 100 列，断言接缝和名字在；再驱动一块
  窄窗格，断言它们不在。
