# 会话临时存储与已移除的 Agent scratchpad

日期：2026-11-17

## 决策

Zeroth 不再为 Agent 配置专属 scratchpad。会话不创建该目录，不向系统提示词追加
scratchpad 指令或路径，不导出专属变量，也不提供特殊写入豁免。
子 Agent 和批量任务遵循相同规则。冗长的生成路径以及要求所有笔记写到固定位置的指令
会增加上下文负担，无法直接推进用户任务。

启动时忽略旧的 `ZEROTH_SCRATCHPAD` 和 `ZEROTH_SCRATCHPAD_DIR` 开关，工具 shell
环境中也移除它们。不为模型指定替代目录。用户明确要求创建的临时文件使用普通文件工具
和权限策略。

## 内部存储

后台进程输出和缩放后的图片载荷仍需要存储：

```text
{tmp}/zeroth-{uid}/{session}/
├── tasks/                 background process output
└── images/                resized image payload cache
```

这些路径按会话计算，不存在进程级全局 scratchpad 值。后台工具按需提供输出位置；
图片缓存是实现细节，不注入提示词。
参见[后台命令](background.md)与[图片](../ui/images.md)。

## 兼容

现有文件保持原位。旧 rollout 保留工具输出、批准说明和路径。仅用于显示的历史
scratchpad 编辑识别继续保留，确保已保存会话可以渲染；它不提供当前权限，也不创建
新目录。新会话没有 Agent scratchpad，因此 hook 载荷省略 `scratchpad_dir`。

## 验证

Headless 集成测试在旧开关存在的情况下启动，验证不创建目录、不添加提示词区块，
shell 子进程也拿不到这两个变量。会话存储 smoke 阶段验证没有 Agent scratchpad 时
后台输出仍然可用。现有 UI 测试继续覆盖历史显示。
