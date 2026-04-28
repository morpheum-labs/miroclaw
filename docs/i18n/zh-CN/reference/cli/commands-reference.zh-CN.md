# Miroclaw 命令参考文档

本参考文档派生自当前 CLI 界面（`miroclaw --help`）。

最后验证时间：**2026年4月15日**。

## 顶级命令

| 命令 | 用途 |
|---|---|
| `onboard` | 快速或交互式初始化工作区/配置 |
| `agent` | 运行交互式聊天或单消息模式 |
| `gateway` | 启动 webhook 和 WhatsApp HTTP 网关 |
| `daemon` | 启动受监管的运行时（网关 + 渠道 + 可选心跳/调度器） |
| `service` | 管理用户级操作系统服务生命周期 |
| `doctor` | 运行诊断和新鲜度检查 |
| `status` | 打印当前配置和系统摘要 |
| `estop` | 启动/恢复紧急停止级别并检查 estop 状态 |
| `cron` | 管理计划任务 |
| `models` | 刷新提供商模型目录 |
| `providers` | 列出提供商 ID、别名和活动提供商 |
| `channel` | 管理渠道和渠道健康检查 |
| `integrations` | 检查集成详情 |
| `skills` | 列出/安装/移除技能 |
| `migrate` | 从外部运行时导入（当前支持 OpenClaw） |
| `config` | 导出机器可读的配置模式 |
| `completions` | 生成 shell 补全脚本到 stdout |
| `hardware` | 发现和检查 USB 硬件 |
| `peripheral` | 配置和烧录外围设备 |

## 命令组

### `onboard`

- `miroclaw onboard`
- `miroclaw onboard --channels-only`
- `miroclaw onboard --force`
- `miroclaw onboard --reinit`
- `miroclaw onboard --api-key <KEY> --provider <ID> --memory <sqlite|lucid|markdown|none>`
- `miroclaw onboard --api-key <KEY> --provider <ID> --model <MODEL_ID> --memory <sqlite|lucid|markdown|none>`
- `miroclaw onboard --api-key <KEY> --provider <ID> --model <MODEL_ID> --memory <sqlite|lucid|markdown|none> --force`

`onboard` 安全行为：

- 如果 `config.toml` 已存在，引导程序提供两种模式：
  - 完整引导（覆盖 `config.toml`）
  - 仅更新提供商（更新提供商/模型/API 密钥，同时保留现有渠道、隧道、内存、钩子和其他设置）
- 在非交互式环境中，现有 `config.toml` 会导致安全拒绝，除非传递 `--force`。
- 当你只需要轮换渠道令牌/白名单时，使用 `miroclaw onboard --channels-only`。
- 使用 `miroclaw onboard --reinit` 重新开始。这会备份现有配置目录并添加时间戳后缀，然后从头创建新配置。

### `agent`

- `miroclaw agent`
- `miroclaw agent -m \"Hello\"`
- `miroclaw agent --provider <ID> --model <MODEL> --temperature <0.0-2.0>`
- `miroclaw agent --peripheral <board:path>`

提示：

- 在交互式聊天中，你可以用自然语言要求更改路由（例如“对话使用 kimi，编码使用 gpt-5.3-codex”）；助手可以通过工具 `model_routing_config` 持久化这些设置。

### `gateway` / `daemon`

- `miroclaw gateway [--host <HOST>] [--port <PORT>]`
- `miroclaw daemon [--host <HOST>] [--port <PORT>]`

### `estop`

- `miroclaw estop`（启动 `kill-all`）
- `miroclaw estop --level network-kill`
- `miroclaw estop --level domain-block --domain \"*.chase.com\" [--domain \"*.paypal.com\"]`
- `miroclaw estop --level tool-freeze --tool shell [--tool browser]`
- `miroclaw estop status`
- `miroclaw estop resume`
- `miroclaw estop resume --network`
- `miroclaw estop resume --domain \"*.chase.com\"`
- `miroclaw estop resume --tool shell`
- `miroclaw estop resume --otp <123456>`

注意事项：

- `estop` 命令需要 `[security.estop].enabled = true`。
- 当 `[security.estop].require_otp_to_resume = true` 时，`resume` 需要 OTP 验证。
- 如果省略 `--otp`，OTP 提示会自动出现。

### `service`

- `miroclaw service install`
- `miroclaw service start`
- `miroclaw service stop`
- `miroclaw service restart`
- `miroclaw service status`
- `miroclaw service uninstall`

### `cron`

- `miroclaw cron list`
- `miroclaw cron add <expr> [--tz <IANA_TZ>] <command>`
- `miroclaw cron add-at <rfc3339_timestamp> <command>`
- `miroclaw cron add-every <every_ms> <command>`
- `miroclaw cron once <delay> <command>`
- `miroclaw cron remove <id>`
- `miroclaw cron pause <id>`
- `miroclaw cron resume <id>`

注意事项：

- 修改计划/cron 操作需要 `cron.enabled = true`。
- 用于创建计划的 Shell 命令 payload（`create` / `add` / `once`）在作业持久化前会经过安全命令策略验证。

### `models`

- `miroclaw models refresh`
- `miroclaw models refresh --provider <ID>`
- `miroclaw models refresh --force`

`models refresh` 当前支持以下提供商 ID 的实时目录刷新：`openrouter`、`openai`、`anthropic`、`groq`、`mistral`、`deepseek`、`xai`、`together-ai`、`gemini`、`ollama`、`llamacpp`、`sglang`、`vllm`、`astrai`、`venice`、`fireworks`、`cohere`、`moonshot`、`glm`、`zai`、`qwen` 和 `nvidia`。

### `doctor`

- `miroclaw doctor`
- `miroclaw doctor query-engine` — 进程内 QueryEngine 状态迁移尾部、最近一次系统提示组装、`[memory.layered]` 时的分层选择器统计、上次**压缩后记忆注入**时间戳，以及来自合并结果的**会话记忆摘要**短预览（仅当前进程）。
- `miroclaw doctor models [--provider <ID>] [--use-cache]`
- `miroclaw doctor traces [--limit <N>] [--event <TYPE>] [--contains <TEXT>]`
- `miroclaw doctor traces --id <TRACE_ID>`
- `miroclaw doctor long-run [HAND]` — 可选 `HAND` 为 `~/.zeroclaw/hands` 下 TOML 文件名（不含扩展名）；省略则扫描全部 hand。对每个 hand 检查协调器 scratchpad（`decisions.md` / `final_summary.md`）新鲜度、启用分层时工作区 AutoMemory 索引年龄，以及组装后的 hand 系统提示是否仍含 `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__`（Phase 1 缓存分段）。

`doctor traces` 从 `observability.runtime_trace_path` 读取运行时工具/模型诊断信息。

### `channel`

- `miroclaw channel list`
- `miroclaw channel start`
- `miroclaw channel doctor`
- `miroclaw channel bind-telegram <IDENTITY>`
- `miroclaw channel add <type> <json>`
- `miroclaw channel remove <name>`

运行时聊天内命令（渠道服务器运行时的 Telegram/Discord）：

- `/models`
- `/models <provider>`
- `/model`
- `/model <model-id>`
- `/new`

渠道运行时还会监视 `config.toml` 并热应用以下更新：
- `default_provider`
- `default_model`
- `default_temperature`
- `api_key` / `api_url`（针对默认提供商）
- `reliability.*` 提供商重试设置

`add/remove` 当前会引导你回到托管安装/手动配置路径（尚未支持完整的声明式修改）。

### `integrations`

- `miroclaw integrations info <name>`

### `skills`

- `miroclaw skills list`
- `miroclaw skills audit <source_or_name>`
- `miroclaw skills install <source>`
- `miroclaw skills remove <name>`

`<source>` 接受 git 远程地址（`https://...`、`http://...`、`ssh://...` 和 `git@host:owner/repo.git`）或本地文件系统路径。

`skills install` 在接受技能前始终会运行内置的静态安全审计。审计会阻止：
- 技能包内的符号链接
- 类脚本文件（`.sh`、`.bash`、`.zsh`、`.ps1`、`.bat`、`.cmd`）
- 高风险命令片段（例如管道到 Shell 的 payload）
- 逃出技能根目录、指向远程 markdown 或目标为脚本文件的 markdown 链接

在共享候选技能目录（或按名称已安装的技能）前，使用 `skills audit` 手动验证。

技能清单（`SKILL.toml`）支持 `prompts` 和 `[[tools]]`；两者都会在运行时注入到代理系统提示中，因此模型可以遵循技能指令而无需手动读取技能文件。

### `migrate`

- `miroclaw migrate openclaw [--source <path>] [--dry-run]`

### `config`

- `miroclaw config schema`

`config schema` 将完整 `config.toml` 契约的 JSON Schema（草案 2020-12）打印到 stdout。

### `completions`

- `miroclaw completions bash`
- `miroclaw completions fish`
- `miroclaw completions zsh`
- `miroclaw completions powershell`
- `miroclaw completions elvish`

`completions` 设计为仅输出到 stdout，因此脚本可以直接被 source 而不会被日志/警告污染。

### `hardware`

- `miroclaw hardware discover`
- `miroclaw hardware introspect <path>`
- `miroclaw hardware info [--chip <chip_name>]`

### `peripheral`

- `miroclaw peripheral list`
- `miroclaw peripheral add <board> <path>`
- `miroclaw peripheral flash [--port <serial_port>]`
- `miroclaw peripheral setup-uno-q [--host <ip_or_host>]`
- `miroclaw peripheral flash-nucleo`

## 验证提示

要快速针对当前二进制文件验证文档：

```bash
miroclaw --help
miroclaw <command> --help
```
