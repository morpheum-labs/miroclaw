# Tham khảo lệnh Miroclaw

Dựa trên CLI hiện tại (`miroclaw --help`).

Xác minh lần cuối: **2026-04-15**.

## Lệnh cấp cao nhất

| Lệnh | Mục đích |
|---|---|
| `onboard` | Khởi tạo workspace/config nhanh hoặc tương tác |
| `agent` | Chạy chat tương tác hoặc chế độ gửi tin nhắn đơn |
| `gateway` | Khởi động gateway webhook và HTTP WhatsApp |
| `daemon` | Khởi động runtime có giám sát (gateway + channels + heartbeat/scheduler tùy chọn) |
| `service` | Quản lý vòng đời dịch vụ cấp hệ điều hành |
| `doctor` | Chạy chẩn đoán và kiểm tra trạng thái |
| `status` | Hiển thị cấu hình và tóm tắt hệ thống |
| `cron` | Quản lý tác vụ định kỳ |
| `models` | Làm mới danh mục model của provider |
| `providers` | Liệt kê ID provider, bí danh và provider đang dùng |
| `channel` | Quản lý kênh và kiểm tra sức khỏe kênh |
| `integrations` | Kiểm tra chi tiết tích hợp |
| `skills` | Liệt kê/cài đặt/gỡ bỏ skills |
| `migrate` | Nhập dữ liệu từ runtime khác (hiện hỗ trợ OpenClaw) |
| `config` | Xuất schema cấu hình dạng máy đọc được |
| `completions` | Tạo script tự hoàn thành cho shell ra stdout |
| `hardware` | Phát hiện và kiểm tra phần cứng USB |
| `peripheral` | Cấu hình và nạp firmware thiết bị ngoại vi |

## Nhóm lệnh

### `onboard`

- `miroclaw onboard`
- `miroclaw onboard --channels-only`
- `miroclaw onboard --api-key <KEY> --provider <ID> --memory <sqlite|lucid|markdown|none>`
- `miroclaw onboard --api-key <KEY> --provider <ID> --model <MODEL_ID> --memory <sqlite|lucid|markdown|none>`

### `agent`

- `miroclaw agent`
- `miroclaw agent -m "Hello"`
- `miroclaw agent --provider <ID> --model <MODEL> --temperature <0.0-2.0>`
- `miroclaw agent --peripheral <board:path>`

### `gateway` / `daemon`

- `miroclaw gateway [--host <HOST>] [--port <PORT>]`
- `miroclaw daemon [--host <HOST>] [--port <PORT>]`

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

### `models`

- `miroclaw models refresh`
- `miroclaw models refresh --provider <ID>`
- `miroclaw models refresh --force`

`models refresh` hiện hỗ trợ làm mới danh mục trực tiếp cho các provider: `openrouter`, `openai`, `anthropic`, `groq`, `mistral`, `deepseek`, `xai`, `together-ai`, `gemini`, `ollama`, `astrai`, `venice`, `fireworks`, `cohere`, `moonshot`, `glm`, `zai`, `qwen` và `nvidia`.

### `doctor`

- `miroclaw doctor`
- `miroclaw doctor query-engine` — trace QueryEngine, system prompt, layered memory, memory injection, tóm tắt session-memory (trong tiến trình).
- `miroclaw doctor models [--provider <ID>] [--use-cache]`
- `miroclaw doctor traces [--limit <N>] [--event <TYPE>] [--contains <TEXT>]` / `miroclaw doctor traces --id <TRACE_ID>`
- `miroclaw doctor long-run [HAND]` — kiểm tra hand điều phối (scratchpad, index AutoMemory, ranh giới `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__`); `HAND` là tên file TOML trong `~/.zeroclaw/hands` (bỏ qua để quét tất cả).

### `channel`

- `miroclaw channel list`
- `miroclaw channel start`
- `miroclaw channel doctor`
- `miroclaw channel bind-telegram <IDENTITY>`
- `miroclaw channel add <type> <json>`
- `miroclaw channel remove <name>`

Lệnh trong chat khi runtime đang chạy (Telegram/Discord):

- `/models`
- `/models <provider>`
- `/model`
- `/model <model-id>`

Channel runtime cũng theo dõi `config.toml` và tự động áp dụng thay đổi cho:
- `default_provider`
- `default_model`
- `default_temperature`
- `api_key` / `api_url` (cho provider mặc định)
- `reliability.*` cài đặt retry của provider

`add/remove` hiện chuyển hướng về thiết lập có hướng dẫn / cấu hình thủ công (chưa hỗ trợ đầy đủ mutator khai báo).

### `integrations`

- `miroclaw integrations info <name>`

### `skills`

- `miroclaw skills list`
- `miroclaw skills install <source>`
- `miroclaw skills remove <name>`

`<source>` chấp nhận git remote (`https://...`, `http://...`, `ssh://...` và `git@host:owner/repo.git`) hoặc đường dẫn cục bộ.

Skill manifest (`SKILL.toml`) hỗ trợ `prompts` và `[[tools]]`; cả hai được đưa vào system prompt của agent khi chạy, giúp model có thể tuân theo hướng dẫn skill mà không cần đọc thủ công.

### `migrate`

- `miroclaw migrate openclaw [--source <path>] [--dry-run]`

### `config`

- `miroclaw config schema`

`config schema` xuất JSON Schema (draft 2020-12) cho toàn bộ hợp đồng `config.toml` ra stdout.

### `completions`

- `miroclaw completions bash`
- `miroclaw completions fish`
- `miroclaw completions zsh`
- `miroclaw completions powershell`
- `miroclaw completions elvish`

`completions` chỉ xuất ra stdout để script có thể được source trực tiếp mà không bị lẫn log/cảnh báo.

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

## Kiểm tra nhanh

Để xác minh nhanh tài liệu với binary hiện tại:

```bash
miroclaw --help
miroclaw <command> --help
```
