# Bản đồ kho ZeroClaw

ZeroClaw là runtime agent tự trị ưu tiên Rust. Nhận tin nhắn từ nền tảng chat, định tuyến qua LLM, thực thi gọi tool, lưu bộ nhớ và trả lời. Cũng có thể điều khiển phần cứng ngoại vi và chạy như daemon lâu dài.

## Luồng runtime

```
Tin nhắn người dùng (Telegram/Discord/Slack/...)
        │
        ▼
   ┌─────────┐     ┌────────────┐
   │ Channel  │────▶│   Agent    │  (src/agent/)
   └─────────┘     │  Loop      │
                   │            │◀──── Memory Loader (tải ngữ cảnh liên quan)
                   │            │◀──── System Prompt Builder
                   │            │◀──── Query Classifier (định tuyến model)
                   └─────┬──────┘
                         │
                         ▼
                   ┌───────────┐
                   │  Provider  │  (LLM: Anthropic, OpenAI, Gemini, v.v.)
                   └─────┬─────┘
                         │
                    gọi tool?
                    ┌────┴────┐
                    ▼         ▼
               ┌────────┐  phản hồi văn bản
               │  Tools  │     │
               └────┬───┘     │
                    │         │
                    ▼         ▼
              đưa kết quả   gửi lại
              về LLM       qua Channel
```

---

## Cấu trúc thư mục gốc

```
zeroclaw/
├── src/                  # Mã Rust (runtime)
├── crates/robot-kit/     # Crate riêng cho bộ robot phần cứng
├── tests/                # Kiểm thử tích hợp / E2E
├── benches/              # Benchmark (vòng agent)
├── docs/contributing/extension-examples.md  # Ví dụ mở rộng (provider/kênh/tool/bộ nhớ)
├── firmware/             # Firmware nhúng cho Arduino, ESP32, Nucleo
├── web/                  # Web UI (Vite + TypeScript)
├── python/               # Python SDK / cầu nối tool
├── dev/                  # Công cụ dev cục bộ (Docker, script CI, sandbox)
├── scripts/              # CI, tự động hóa phát hành, bootstrap
├── docs/                 # Hệ thống tài liệu (đa ngôn ngữ, tham chiếu runtime)
├── .github/              # Workflow CI, mẫu PR, tự động hóa
├── playground/           # (trống, không gian thử)
├── Cargo.toml            # Manifest workspace
├── Dockerfile            # Build container
├── flake.nix             # Môi trường dev Nix
└── install.sh            # Script cài đặt một lệnh
```

---

## src/ — Theo module

### Điểm vào

| Tệp | Dòng | Vai trò |
|---|---|---|
| `main.rs` | 1,977 | Điểm vào CLI. Parser Clap, phân phối lệnh. Toàn bộ định tuyến `zeroclaw <lệnh con>` ở đây. |
| `lib.rs` | 436 | Khai báo module, visibility (`pub` / `pub(crate)`), enum lệnh CLI dùng chung (`ServiceCommands`, `ChannelCommands`, `SkillCommands`, v.v.) giữa lib và binary. |

### Runtime lõi

| Module | Tệp chính | Vai trò |
|---|---|---|
| `agent/` | `agent.rs`, `loop_.rs` (5.6k), `system_prompt.rs`, `dispatcher.rs`, `prompt.rs`, `classifier.rs`, `memory_loader.rs` | **Bộ não.** `AgentBuilder` kết hợp provider+tool+bộ nhớ+observer. `system_prompt.rs` lắp ráp system prompt workspace (tĩnh vs động; marker ranh giới để cache phía provider). `channels/mod.rs` ủy quyền `build_system_prompt_*` tới đây; `loop_.rs` có thể vá lại tin system đầu tiên sau compaction khi caller truyền `system_prompt_refresh`. Dispatcher xử lý phân tích gọi tool native vs XML. Classifier định tuyến truy vấn tới các model. |
| `config/` | `schema.rs` (7.6k), `mod.rs`, `traits.rs` | **Mọi struct cấu hình.** Mỗi hệ con trong `schema.rs` — provider, kênh, bộ nhớ, bảo mật, gateway, tool, phần cứng, lịch, v.v. Đọc từ TOML. |
| `runtime/` | `native.rs`, `docker.rs`, `wasm.rs`, `traits.rs` | **Bộ thích ứng nền tảng.** Trait `RuntimeAdapter` trừu tượng hóa shell, filesystem, đường dẫn lưu trữ, ngân sách bộ nhớ. Native = OS trực tiếp. Docker = cô lập container. WASM = thử nghiệm. |

### Nhà cung cấp LLM

| Module | Tệp chính | Vai trò |
|---|---|---|
| `providers/` | `traits.rs`, `mod.rs` (2.9k), `reliable.rs`, `router.rs`, + 11 tệp | **Tích hợp LLM.** Trait `Provider`: `chat()`, `chat_with_system()`, `capabilities()`, `convert_tools()`. Factory trong `mod.rs` tạo provider theo tên. `ReliableProvider` bọc retry/fallback. `RoutedProvider` định tuyến theo gợi ý classifier. |

Providers: `anthropic`, `openai`, `openai_codex`, `openrouter`, `gemini`, `ollama`, `compatible` (tương thích OpenAI), `copilot`, `bedrock`, `telnyx`, `glm`

### Kênh nhắn tin

| Module | Tệp chính | Vai trò |
|---|---|---|
| `channels/` | `traits.rs`, `mod.rs` (6.6k), + 22 tệp | **Vận chuyển vào/ra.** Trait `Channel`: `send()`, `listen()`, `health_check()`, `start_typing()`, cập nhật nháp. Factory trong `mod.rs` nối cấu hình với instance, quản lý lịch sử hội thoại tối đa 50 tin mỗi người gửi. |

Channels: `telegram` (4.6k), `discord`, `slack`, `whatsapp`, `whatsapp_web`, `matrix`, `signal`, `email_channel`, `qq`, `dingtalk`, `lark`, `imessage`, `irc`, `nostr`, `mattermost`, `nextcloud_talk`, `wati`, `mqtt`, `linq`, `clawdtalk`, `cli`

### Tool (khả năng agent)

| Module | Tệp chính | Vai trò |
|---|---|---|
| `tools/` | `traits.rs`, `mod.rs` (635), + 38 tệp | **Agent có thể làm gì.** Trait `Tool`: `name()`, `description()`, `parameters_schema()`, `execute()`. Hai registry: `default_tools()` (6 thiết yếu) và `all_tools_with_runtime()` (đầy đủ, theo cổng cấu hình). |

Nhóm tool:
- **File/Shell**: `shell`, `file_read`, `file_write`, `file_edit`, `glob_search`, `content_search`
- **Memory**: `memory_store`, `memory_recall`, `memory_forget`
- **Web**: `browser`, `browser_open`, `web_fetch`, `web_search_tool`, `http_request`
- **Scheduling**: `cron_add`, `cron_list`, `cron_remove`, `cron_update`, `cron_run`, `cron_runs`, `schedule`
- **Delegation**: `delegate` (spawn sub-agent), `composio` (tích hợp OAuth)
- **Hardware**: `hardware_board_info`, `hardware_memory_map`, `hardware_memory_read`
- **SOP**: `sop_execute`, `sop_advance`, `sop_approve`, `sop_list`, `sop_status`
- **Utility**: `git_operations`, `image_info`, `pdf_read`, `screenshot`, `pushover`, `model_routing_config`, `proxy_config`, `cli_discovery`, `schema`

### Bộ nhớ

| Module | Tệp chính | Vai trò |
|---|---|---|
| `memory/` | `traits.rs`, `backend.rs`, `mod.rs`, + 8 tệp | **Tri thức bền.** Trait `Memory`: `store()`, `recall()`, `get()`, `list()`, `forget()`, `count()`. Danh mục: Core, Daily, Conversation, Custom. |

Backends: `sqlite`, `markdown`, `lucid` (SQLite + embedding), `qdrant` (vector DB), `postgres`, `none`

Hỗ trợ: `embeddings.rs`, `vector.rs`, `chunker.rs`, `hygiene.rs`, `snapshot.rs`, `response_cache.rs`, `cli.rs`

### Bảo mật

| Module | Tệp chính | Vai trò |
|---|---|---|
| `security/` | `policy.rs` (2.3k), `secrets.rs`, `pairing.rs`, `prompt_guard.rs`, `leak_detector.rs`, `audit.rs`, `otp.rs`, `estop.rs`, `domain_matcher.rs`, + 4 tệp sandbox | **Chính sách và thực thi.** `SecurityPolicy`: mức tự trị (ReadOnly/Supervised/Full), giới hạn workspace, allowlist lệnh, đường dẫn cấm, giới hạn tốc độ, trần chi phí. |

Sandboxing: `bubblewrap.rs`, `firejail.rs`, `landlock.rs`, `docker.rs`, `detect.rs` (tự phát hiện tốt nhất)

### Gateway (HTTP API)

| Module | Tệp chính | Vai trò |
|---|---|---|
| `gateway/` | `mod.rs` (2.8k), `api.rs` (1.4k), `sse.rs`, `ws.rs`, `static_files.rs` | **Máy chủ HTTP Axum.** Webhook (WhatsApp, WATI, Linq, Nextcloud Talk), REST API, SSE, WebSocket. Giới hạn tốc độ, khóa idempotency, giới hạn body 64KB, timeout 30s. |

### Phần cứng và ngoại vi

| Module | Tệp chính | Vai trò |
|---|---|---|
| `peripherals/` | `traits.rs`, `mod.rs`, `serial.rs`, `rpi.rs`, `arduino_flash.rs`, `uno_q_bridge.rs`, `uno_q_setup.rs`, `nucleo_flash.rs`, `capabilities_tool.rs` | **Trừu tượng hóa bo mạch.** Trait `Peripheral`: `connect()`, `disconnect()`, `health_check()`, `tools()`. |
| `hardware/` | `discover.rs`, `introspect.rs`, `registry.rs`, `mod.rs` | **Phát hiện USB và nhận dạng bo.** Quét VID/PID, khớp bo đã biết, introspect thiết bị. |

### Khả năng quan sát

| Module | Tệp chính | Vai trò |
|---|---|---|
| `observability/` | `traits.rs`, `mod.rs`, `log.rs`, `prometheus.rs`, `otel.rs`, `verbose.rs`, `noop.rs`, `multi.rs`, `runtime_trace.rs` | **Số liệu và trace.** Trait `Observer`: `log_event()`. Observer tổng hợp (`multi.rs`) fan-out tới nhiều backend. |

### Skills và SkillForge

| Module | Tệp chính | Vai trò |
|---|---|---|
| `skills/` | `mod.rs` (1.5k), `audit.rs` | **Khả năng do người dùng/cộng đồng viết.** Tải từ `~/.zeroclaw/workspace/skills/<name>/SKILL.md`. CLI: list, install, audit, remove. Đồng bộ cộng đồng tùy chọn từ kho open-skills. |
| `skillforge/` | `scout.rs`, `evaluate.rs`, `integrate.rs`, `mod.rs` | **Tìm và đánh giá skill.** |

### SOP

| Module | Tệp chính | Vai trò |
|---|---|---|
| `sop/` | `engine.rs` (1.6k), `metrics.rs` (1.5k), `types.rs`, `dispatch.rs`, `condition.rs`, `gates.rs`, `audit.rs`, `mod.rs` | **Động cơ quy trình.** Quy trình nhiều bước với điều kiện, cổng (điểm phê duyệt) và số liệu. |

### Lịch và vòng đời

| Module | Tệp chính | Vai trò |
|---|---|---|
| `cron/` | `scheduler.rs`, `schedule.rs`, `store.rs`, `types.rs`, `mod.rs` | **Bộ lập lịch.** Biểu thức cron, timer một lần, khoảng cố định. Lưu trữ bền. |
| `heartbeat/` | `engine.rs`, `mod.rs` | **Giám sát sống.** Kiểm tra định kỳ kênh/gateway. |
| `daemon/` | `mod.rs` | **Daemon lâu dài.** Khởi động gateway + kênh + heartbeat + scheduler. |
| `service/` | `mod.rs` (1.3k) | **Quản lý dịch vụ OS.** systemd hoặc launchd. |
| `hooks/` | `mod.rs`, `runner.rs`, `traits.rs`, `builtin/` | **Hook vòng đời.** |

### Module hỗ trợ

| Module | Tệp chính | Vai trò |
|---|---|---|
| `onboard/` | `wizard.rs` (7.2k), `mod.rs` | **Trình hướng dẫn lần đầu.** |
| `auth/` | `profiles.rs`, `anthropic_token.rs`, `gemini_oauth.rs`, `openai_oauth.rs`, `oauth_common.rs` | **Hồ sơ xác thực và OAuth.** |
| `approval/` | `mod.rs` | **Luồng phê duyệt.** |
| `doctor/` | `mod.rs`, `long_run.rs` | **Chẩn đoán.** Kiểm tra daemon, scheduler, kênh; `long_run` thăm dò hand điều phối (scratchpad, index, ranh giới prompt cache). |
| `health/` | `mod.rs` | **Endpoint health.** |
| `cost/` | `tracker.rs`, `types.rs`, `mod.rs` | **Theo dõi chi phí.** |
| `tunnel/` | `cloudflare.rs`, `ngrok.rs`, `tailscale.rs`, `custom.rs`, `none.rs`, `mod.rs` | **Bộ thích ứng tunnel.** |
| `rag/` | `mod.rs` | **RAG.** Trích PDF, hỗ trợ chunk. |
| `integrations/` | `registry.rs`, `mod.rs` | **Danh mục tích hợp.** |
| `identity.rs` | (1.5k) | **Danh tính agent.** |
| `multimodal.rs` | — | **Đa phương thức.** |
| `migration.rs` | — | **Di chuyển dữ liệu.** OpenClaw. |
| `util.rs` | — | **Tiện ích dùng chung.** |

---

## Ngoài src/

| Thư mục | Vai trò |
|---|---|
| `crates/robot-kit/` | Crate Rust riêng cho bộ robot |
| `tests/` | Kiểm thử tích hợp / E2E |
| `benches/` | Benchmark (`agent_benchmarks.rs`) |
| `docs/contributing/extension-examples.md` | Ví dụ mở rộng |
| `firmware/` | Firmware: `arduino/`, `esp32/`, `esp32-ui/`, `nucleo/`, `uno-q-bridge/` |
| `web/` | Frontend Web UI |
| `python/` | Python SDK / cầu nối |
| `dev/` | Docker Compose, script CI (`ci.sh`), mẫu cấu hình, sandbox |
| `scripts/` | CI, phát hành, bootstrap, tính cấp contributor |
| `docs/` | Tài liệu đa ngôn ngữ (en/zh-CN/ja/ru/fr/vi), tham chiếu, runbook vận hành, đề xuất bảo mật |
| `.github/` | CI, mẫu PR/issue, tự động hóa |

---

## Hướng phụ thuộc

```
main.rs ──▶ agent/ ──▶ providers/  (gọi LLM)
               │──▶ tools/      (thực thi khả năng)
               │──▶ memory/     (bền ngữ cảnh)
               │──▶ observability/ (log sự kiện)
               │──▶ security/   (áp chính sách)
               │──▶ config/     (struct cấu hình)
               │──▶ runtime/    (trừu tượng nền tảng)
               │
main.rs ──▶ channels/ ──▶ agent/ (định tuyến tin nhắn)
main.rs ──▶ gateway/  ──▶ agent/ (định tuyến HTTP/WS)
main.rs ──▶ daemon/   ──▶ gateway/ + channels/ + cron/ + heartbeat/

Module cụ thể phụ thuộc vào trong tới trait/cấu hình.
Trait không import implementation cụ thể.
```

---

## Cây lệnh CLI

```
zeroclaw
├── onboard [--force] [--reinit] [--channels-only]     # Thiết lập lần đầu
├── agent [-m "msg"] [-p provider]        # Chạy vòng agent
├── daemon [-p port]                      # Runtime đầy đủ
├── gateway [-p port]                     # Chỉ máy chủ HTTP API
├── channel {list|start|doctor|add|remove|bind-telegram}
├── skill {list|install|audit|remove}
├── memory {list|get|stats|clear}
├── cron {list|add|add-at|add-every|once|remove|update|pause|resume}
├── peripheral {list|add|flash|flash-nucleo|setup-uno-q}
├── hardware {discover|introspect|info}
├── service {install|start|stop|restart|status|uninstall}
├── doctor                                # Chẩn đoán
├── status                                # Tổng quan hệ thống
├── estop [--level] [status|resume]       # Dừng khẩn cấp
├── migrate openclaw                      # Di chuyển dữ liệu
├── pair                                  # Ghép nối thiết bị
├── auth-profiles                         # Quản lý thông tin đăng nhập
├── version / completions                 # Meta
└── config {show|edit|validate|reset}
```
