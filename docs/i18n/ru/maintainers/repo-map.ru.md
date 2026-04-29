# Карта репозитория ZeroClaw

ZeroClaw — автономный агентный рантайм с приоритетом на Rust. Принимает сообщения из мессенджеров, маршрутизирует их в LLM, выполняет вызовы инструментов, сохраняет память и возвращает ответы. Может управлять периферией и работать как долгоживущий демон.

## Поток выполнения

```
Сообщение пользователя (Telegram/Discord/Slack/...)
        │
        ▼
   ┌─────────┐     ┌────────────┐
   │ Channel  │────▶│   Agent    │  (src/agent/)
   └─────────┘     │  Loop      │
                   │            │◀──── Memory Loader (релевантный контекст)
                   │            │◀──── System Prompt Builder
                   │            │◀──── Query Classifier (маршрутизация моделей)
                   └─────┬──────┘
                         │
                         ▼
                   ┌───────────┐
                   │  Provider  │  (LLM: Anthropic, OpenAI, Gemini и т.д.)
                   └─────┬─────┘
                         │
                    вызовы инструментов?
                    ┌────┴────┐
                    ▼         ▼
               ┌────────┐  текстовый ответ
               │  Tools  │     │
               └────┬───┘     │
                    │         │
                    ▼         ▼
              результаты    ответ
              обратно в LLM через Channel
```

---

## Верхний уровень

```
zeroclaw/
├── src/                  # Исходники Rust (рантайм)
├── crates/robot-kit/     # Отдельный crate для роботокомплекта
├── tests/                # Интеграционные/E2E-тесты
├── benches/              # Бенчмарки (цикл агента)
├── docs/contributing/extension-examples.md  # Примеры расширений (provider/channel/tool/memory)
├── firmware/             # Встроенное ПО для Arduino, ESP32, Nucleo
├── web/                  # Web UI (Vite + TypeScript)
├── python/               # Python SDK / мост инструментов
├── dev/                  # Локальная разработка (Docker, CI, sandbox)
├── scripts/              # CI, релизы, bootstrap
├── docs/                 # Документация (мультиязычность, справки)
├── .github/              # CI, шаблоны PR, автоматизация
├── playground/           # (пусто, эксперименты)
├── Cargo.toml            # Манифест workspace
├── Dockerfile            # Сборка контейнера
├── flake.nix             # Среда Nix
└── install.sh            # Установка одной командой
```

---

## src/ — по модулям

### Точки входа

| Файл | Строки | Роль |
|---|---|---|
| `main.rs` | 1,977 | CLI. Парсер Clap, диспетчер команд. Весь роутинг `zeroclaw <subcommand>`. |
| `lib.rs` | 436 | Объявления модулей, видимость (`pub` / `pub(crate)`), общие CLI-перечисления (`ServiceCommands`, `ChannelCommands`, `SkillCommands` и т.д.) для lib и binary. |

### Ядро рантайма

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `agent/` | `agent.rs`, `loop_.rs` (5.6k), `system_prompt.rs`, `dispatcher.rs`, `prompt.rs`, `classifier.rs`, `memory_loader.rs` | **Мозг.** `AgentBuilder` собирает provider+tools+memory+observer. `system_prompt.rs` собирает системный промпт рабочей области (статика vs динамика; маркер границы для кэша на стороне провайдера). `channels/mod.rs` делегирует `build_system_prompt_*` сюда; `loop_.rs` может обновить первое system-сообщение после компакции, если передан `system_prompt_refresh`. Dispatcher разбирает нативные и XML-вызовы инструментов. Classifier маршрутизирует запросы к моделям. |
| `config/` | `schema.rs` (7.6k), `mod.rs`, `traits.rs` | **Все структуры конфигурации.** Подсистемы в `schema.rs` — providers, channels, memory, security, gateway, tools, hardware, scheduling и т.д. Загрузка из TOML. |
| `runtime/` | `native.rs`, `docker.rs`, `wasm.rs`, `traits.rs` | **Адаптеры платформы.** Трейт `RuntimeAdapter`: shell, ФС, пути хранения, бюджеты памяти. Native = ОС напрямую. Docker = изоляция. WASM = эксперимент. |

### LLM-провайдеры

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `providers/` | `traits.rs`, `mod.rs` (2.9k), `reliable.rs`, `router.rs`, + 11 файлов | **Интеграции LLM.** Трейт `Provider`: `chat()`, `chat_with_system()`, `capabilities()`, `convert_tools()`. Фабрика в `mod.rs`. `ReliableProvider` — повторы/цепочки fallback. `RoutedProvider` — по подсказкам классификатора. |

Providers: `anthropic`, `openai`, `openai_codex`, `openrouter`, `gemini`, `ollama`, `compatible` (совместимость с OpenAI), `copilot`, `bedrock`, `telnyx`, `glm`

### Каналы сообщений

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `channels/` | `traits.rs`, `mod.rs` (6.6k), + 22 файла | **Транспорт ввода-вывода.** Трейт `Channel`: `send()`, `listen()`, `health_check()`, `start_typing()`, черновики. Фабрика в `mod.rs`, история до 50 сообщений на отправителя. |

Channels: `telegram` (4.6k), `discord`, `slack`, `whatsapp`, `whatsapp_web`, `matrix`, `signal`, `email_channel`, `qq`, `dingtalk`, `lark`, `imessage`, `irc`, `nostr`, `mattermost`, `nextcloud_talk`, `wati`, `mqtt`, `linq`, `clawdtalk`, `cli`

### Инструменты (возможности агента)

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `tools/` | `traits.rs`, `mod.rs` (635), + 38 файлов | **Что умеет агент.** Трейт `Tool`: `name()`, `description()`, `parameters_schema()`, `execute()`. Два реестра: `default_tools()` (6 базовых) и `all_tools_with_runtime()` (полный набор, по конфигу). |

Категории инструментов:
- **File/Shell**: `shell`, `file_read`, `file_write`, `file_edit`, `glob_search`, `content_search`
- **Memory**: `memory_store`, `memory_recall`, `memory_forget`
- **Web**: `browser`, `browser_open`, `web_fetch`, `web_search_tool`, `http_request`
- **Scheduling**: `cron_add`, `cron_list`, `cron_remove`, `cron_update`, `cron_run`, `cron_runs`, `schedule`
- **Delegation**: `delegate` (субагенты), `composio` (OAuth)
- **Hardware**: `hardware_board_info`, `hardware_memory_map`, `hardware_memory_read`
- **SOP**: `sop_execute`, `sop_advance`, `sop_approve`, `sop_list`, `sop_status`
- **Utility**: `git_operations`, `image_info`, `pdf_read`, `screenshot`, `pushover`, `model_routing_config`, `proxy_config`, `cli_discovery`, `schema`

### Память

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `memory/` | `traits.rs`, `backend.rs`, `mod.rs`, + 8 файлов | **Постоянные знания.** Трейт `Memory`: `store()`, `recall()`, `get()`, `list()`, `forget()`, `count()`. Категории: Core, Daily, Conversation, Custom. |

Backends: `sqlite`, `markdown`, `lucid` (SQLite + эмбеддинги), `qdrant` (векторная БД), `postgres`, `none`

Supporting: `embeddings.rs`, `vector.rs`, `chunker.rs`, `hygiene.rs`, `snapshot.rs`, `response_cache.rs`, `cli.rs`

### Безопасность

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `security/` | `policy.rs` (2.3k), `secrets.rs`, `pairing.rs`, `prompt_guard.rs`, `leak_detector.rs`, `audit.rs`, `otp.rs`, `estop.rs`, `domain_matcher.rs`, + 4 sandbox-файла | **Политика и принуждение.** `SecurityPolicy`: уровни автономии (ReadOnly/Supervised/Full), ограничение workspace, allowlist команд, запрещённые пути, лимиты, капы стоимости. |

Sandboxing: `bubblewrap.rs`, `firejail.rs`, `landlock.rs`, `docker.rs`, `detect.rs` (автовыбор лучшего)

### Шлюз (HTTP API)

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `gateway/` | `mod.rs` (2.8k), `api.rs` (1.4k), `sse.rs`, `ws.rs`, `static_files.rs` | **HTTP-сервер Axum.** Webhooks (WhatsApp, WATI, Linq, Nextcloud Talk), REST, SSE, WebSocket. Rate limit, idempotency, лимит тела 64KB, таймаут 30s. |

### Железо и периферия

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `peripherals/` | `traits.rs`, `mod.rs`, `serial.rs`, `rpi.rs`, `arduino_flash.rs`, `uno_q_bridge.rs`, `uno_q_setup.rs`, `nucleo_flash.rs`, `capabilities_tool.rs` | **Абстракция плат.** Трейт `Peripheral`: `connect()`, `disconnect()`, `health_check()`, `tools()`. |
| `hardware/` | `discover.rs`, `introspect.rs`, `registry.rs`, `mod.rs` | **USB и идентификация плат.** VID/PID, известные платы, интроспекция устройств. |

### Наблюдаемость

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `observability/` | `traits.rs`, `mod.rs`, `log.rs`, `prometheus.rs`, `otel.rs`, `verbose.rs`, `noop.rs`, `multi.rs`, `runtime_trace.rs` | **Метрики и трассировка.** Трейт `Observer`: `log_event()`. Композитный observer в `multi.rs`. |

### Skills и SkillForge

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `skills/` | `mod.rs` (1.5k), `audit.rs` | **Пользовательские/сообщество навыки.** Загрузка из `~/.zeroclaw/workspace/skills/<name>/SKILL.md`. CLI: list, install, audit, remove. Опциональная синхронизация с open-skills. |
| `skillforge/` | `scout.rs`, `evaluate.rs`, `integrate.rs`, `mod.rs` | **Поиск и оценка навыков.** |

### SOP

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `sop/` | `engine.rs` (1.6k), `metrics.rs` (1.5k), `types.rs`, `dispatch.rs`, `condition.rs`, `gates.rs`, `audit.rs`, `mod.rs` | **Движок процедур.** Многошаговые сценарии, условия, ворота (согласование), метрики. |

### Планирование и жизненный цикл

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `cron/` | `scheduler.rs`, `schedule.rs`, `store.rs`, `types.rs`, `mod.rs` | **Планировщик.** Cron, one-shot, фиксированные интервалы, персистентное хранилище. |
| `heartbeat/` | `engine.rs`, `mod.rs` | **Живость.** Периодические проверки каналов/шлюза. |
| `daemon/` | `mod.rs` | **Демон.** Запуск gateway + channels + heartbeat + scheduler. |
| `service/` | `mod.rs` (1.3k) | **Сервис ОС.** systemd или launchd. |
| `hooks/` | `mod.rs`, `runner.rs`, `traits.rs`, `builtin/` | **Хуки жизненного цикла.** |

### Вспомогательные модули

| Модуль | Ключевые файлы | Роль |
|---|---|---|
| `onboard/` | `wizard.rs` (7.2k), `mod.rs` | **Мастер первого запуска.** |
| `auth/` | `profiles.rs`, `anthropic_token.rs`, `gemini_oauth.rs`, `openai_oauth.rs`, `oauth_common.rs` | **Профили и OAuth.** |
| `approval/` | `mod.rs` | **Согласование рискованных действий.** |
| `doctor/` | `mod.rs` (1.3k) | **Диагностика.** |
| `health/` | `mod.rs` | **Эндпоинты health.** |
| `cost/` | `tracker.rs`, `types.rs`, `mod.rs` | **Учёт стоимости.** |
| `tunnel/` | `cloudflare.rs`, `ngrok.rs`, `tailscale.rs`, `custom.rs`, `none.rs`, `mod.rs` | **Туннели.** |
| `rag/` | `mod.rs` | **RAG.** PDF, чанкинг. |
| `integrations/` | `registry.rs`, `mod.rs` | **Реестр интеграций.** |
| `identity.rs` | (1.5k) | **Идентичность агента.** |
| `multimodal.rs` | — | **Мультимодальность.** |
| `migration.rs` | — | **Миграция данных.** OpenClaw. |
| `util.rs` | — | **Общие утилиты.** |

---

## Вне src/

| Каталог | Роль |
|---|---|
| `crates/robot-kit/` | Отдельный crate для роботокомплекта |
| `tests/` | Интеграция/E2E |
| `benches/` | Бенчмарки (`agent_benchmarks.rs`) |
| `docs/contributing/extension-examples.md` | Примеры расширений |
| `firmware/` | Прошивки: `arduino/`, `esp32/`, `esp32-ui/`, `nucleo/`, `uno-q-bridge/` |
| `web/` | Фронтенд Web UI |
| `python/` | Python SDK / мост |
| `dev/` | Docker Compose, `ci.sh`, шаблоны, sandbox |
| `scripts/` | CI, релизы, bootstrap, уровни контрибьюторов |
| `docs/` | Документация: мультиязычность (en/zh-CN/ja/ru/fr/vi), справки, runbooks, security |
| `.github/` | CI, шаблоны PR/issue, автоматизация |

---

## Направление зависимостей

```
main.rs ──▶ agent/ ──▶ providers/  (вызовы LLM)
               │──▶ tools/      (исполнение)
               │──▶ memory/     (персистентность контекста)
               │──▶ observability/ (логи)
               │──▶ security/   (политика)
               │──▶ config/     (структуры конфигурации)
               │──▶ runtime/    (платформа)
               │
main.rs ──▶ channels/ ──▶ agent/ (маршрутизация сообщений)
main.rs ──▶ gateway/  ──▶ agent/ (HTTP/WS)
main.rs ──▶ daemon/   ──▶ gateway/ + channels/ + cron/ + heartbeat/

Конкретные модули зависят внутрь от трейтов и конфигурации.
Трейты не импортируют конкретные реализации.
```

---

## Дерево команд CLI

```
zeroclaw
├── onboard [--force] [--reinit] [--channels-only]     # Первый запуск
├── agent [-m "msg"] [-p provider]        # Цикл агента
├── daemon [-p port]                      # Полный рантайм
├── gateway [-p port]                     # Только HTTP API
├── channel {list|start|doctor|add|remove|bind-telegram}
├── skill {list|install|audit|remove}
├── memory {list|get|stats|clear}
├── cron {list|add|add-at|add-every|once|remove|update|pause|resume}
├── peripheral {list|add|flash|flash-nucleo|setup-uno-q}
├── hardware {discover|introspect|info}
├── service {install|start|stop|restart|status|uninstall}
├── doctor                                # Диагностика
├── status                                # Обзор системы
├── estop [--level] [status|resume]       # Аварийная остановка
├── migrate openclaw                      # Миграция данных
├── pair                                  # Сопряжение устройств
├── auth-profiles                         # Учётные данные
├── version / completions                 # Мета
└── config {show|edit|validate|reset}
```
