# ZeroClaw リポジトリマップ

ZeroClaw は Rust 優先の自律エージェントランタイムです。メッセージングプラットフォームからメッセージを受け取り、LLM にルーティングし、ツール呼び出しを実行し、メモリを永続化して応答を返します。ハードウェア周辺機器の制御や長寿命デーモンとしての実行も可能です。

## ランタイムの流れ

```
ユーザメッセージ (Telegram/Discord/Slack/...)
        │
        ▼
   ┌─────────┐     ┌────────────┐
   │ Channel  │────▶│   Agent    │  (src/agent/)
   └─────────┘     │  Loop      │
                   │            │◀──── Memory Loader（関連コンテキストを読込）
                   │            │◀──── System Prompt Builder
                   │            │◀──── Query Classifier（モデルルーティング）
                   └─────┬──────┘
                         │
                         ▼
                   ┌───────────┐
                   │  Provider  │  (LLM: Anthropic, OpenAI, Gemini など)
                   └─────┬─────┘
                         │
                    tool calls?
                    ┌────┴────┐
                    ▼         ▼
               ┌────────┐  text response
               │  Tools  │     │
               └────┬───┘     │
                    │         │
                    ▼         ▼
              feed results   send back
              back to LLM    via Channel
```

---

## トップレベル構成

```
zeroclaw/
├── src/                  # Rust ソース（ランタイム本体）
├── crates/robot-kit/     # ハードウェアロボットキット用の別 crate
├── tests/                # 結合/E2E テスト
├── benches/              # ベンチマーク（エージェントループ）
├── docs/contributing/extension-examples.md  # 拡張例（カスタム provider/channel/tool/memory）
├── firmware/             # Arduino、ESP32、Nucleo 向け組込みファームウェア
├── web/                  # Web UI（Vite + TypeScript）
├── python/               # Python SDK / ツールブリッジ
├── dev/                  # ローカル開発（Docker、CI スクリプト、サンドボックス）
├── scripts/              # CI、リリース自動化、ブートストラップ
├── docs/                 # ドキュメント（多言語、ランタイム参照）
├── .github/              # CI ワークフロー、PR テンプレート、自動化
├── playground/           # （空、実験用スクラッチ）
├── Cargo.toml            # ワークスペースマニフェスト
├── Dockerfile            # コンテナビルド
├── flake.nix             # Nix 開発環境
└── install.sh            # ワンコマンドセットアップ
```

---

## src/ — モジュール別

### エントリポイント

| ファイル | 行数 | 役割 |
|---|---|---|
| `main.rs` | 1,977 | CLI エントリ。Clap パーサ、コマンドディスパッチ。`zeroclaw <subcommand>` のルーティングはすべてここ。 |
| `lib.rs` | 436 | モジュール宣言、可視性（`pub` / `pub(crate)`）、ライブラリとバイナリで共有する CLI 列挙（`ServiceCommands`、`ChannelCommands`、`SkillCommands` など）。 |

### コアランタイム

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `agent/` | `agent.rs`, `loop_.rs` (5.6k), `system_prompt.rs`, `dispatcher.rs`, `prompt.rs`, `classifier.rs`, `memory_loader.rs` | **頭脳。** `AgentBuilder` が provider + tools + memory + observer を合成。`system_prompt.rs` がワークスペースのシステムプロンプトを組み立てる（静的 vs 動的。プロバイダ側キャッシュ用の境界マーカー）。`channels/mod.rs` の `build_system_prompt_*` はここに委譲。呼び出し元が `system_prompt_refresh` を渡すと、`loop_.rs` がコンパクション後に先頭の system メッセージを差し替え可能。Dispatcher がネイティブと XML のツール呼び出し解析を担当。Classifier がクエリをモデルへ振り分ける。 |
| `config/` | `schema.rs` (7.6k), `mod.rs`, `traits.rs` | **すべての設定構造体。** 各サブシステムの設定は `schema.rs` に集約 — provider、channel、memory、security、gateway、tools、hardware、scheduling など。TOML から読み込み。 |
| `runtime/` | `native.rs`, `docker.rs`, `wasm.rs`, `traits.rs` | **プラットフォームアダプタ。** `RuntimeAdapter` がシェル、ファイルシステム、ストレージパス、メモリ予算を抽象化。Native = 直接 OS。Docker = コンテナ隔離。WASM = 実験的。 |

### LLM プロバイダ

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `providers/` | `traits.rs`, `mod.rs` (2.9k), `reliable.rs`, `router.rs`, + 11 ファイル | **LLM 統合。** `Provider`: `chat()`, `chat_with_system()`, `capabilities()`, `convert_tools()`。`mod.rs` のファクトリが名前からインスタンス化。`ReliableProvider` がリトライ/フォールバック。`RoutedProvider` が分類器ヒントでルート。 |

Providers: `anthropic`, `openai`, `openai_codex`, `openrouter`, `gemini`, `ollama`, `compatible` (OpenAI 互換), `copilot`, `bedrock`, `telnyx`, `glm`

### メッセージングチャネル

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `channels/` | `traits.rs`, `mod.rs` (6.6k), + 22 ファイル | **入出力トランスポート。** `Channel`: `send()`, `listen()`, `health_check()`, `start_typing()`, ドラフト更新。`mod.rs` のファクトリが設定とインスタンスを接続し、送信者ごとの履歴（最大 50 メッセージ）を管理。 |

Channels: `telegram` (4.6k), `discord`, `slack`, `whatsapp`, `whatsapp_web`, `matrix`, `signal`, `email_channel`, `qq`, `dingtalk`, `lark`, `imessage`, `irc`, `nostr`, `mattermost`, `nextcloud_talk`, `wati`, `mqtt`, `linq`, `clawdtalk`, `cli`

### ツール（エージェント能力）

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `tools/` | `traits.rs`, `mod.rs` (635), + 38 ファイル | **エージェントができること。** `Tool`: `name()`, `description()`, `parameters_schema()`, `execute()`。2 つのレジストリ: `default_tools()`（6 種の必須）と `all_tools_with_runtime()`（フルセット、設定ゲート）。 |

Tool categories:
- **File/Shell**: `shell`, `file_read`, `file_write`, `file_edit`, `glob_search`, `content_search`
- **Memory**: `memory_store`, `memory_recall`, `memory_forget`
- **Web**: `browser`, `browser_open`, `web_fetch`, `web_search_tool`, `http_request`
- **Scheduling**: `cron_add`, `cron_list`, `cron_remove`, `cron_update`, `cron_run`, `cron_runs`, `schedule`
- **Delegation**: `delegate`（サブエージェント起動）, `composio`（OAuth 連携）
- **Hardware**: `hardware_board_info`, `hardware_memory_map`, `hardware_memory_read`
- **SOP**: `sop_execute`, `sop_advance`, `sop_approve`, `sop_list`, `sop_status`
- **Utility**: `git_operations`, `image_info`, `pdf_read`, `screenshot`, `pushover`, `model_routing_config`, `proxy_config`, `cli_discovery`, `schema`

### メモリ

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `memory/` | `traits.rs`, `backend.rs`, `mod.rs`, + 8 ファイル | **永続知識。** `Memory`: `store()`, `recall()`, `get()`, `list()`, `forget()`, `count()`。カテゴリ: Core, Daily, Conversation, Custom。 |

Backends: `sqlite`, `markdown`, `lucid`（SQLite + 埋め込みハイブリッド）, `qdrant`（ベクトル DB）, `postgres`, `none`

Supporting: `embeddings.rs`, `vector.rs`, `chunker.rs`, `hygiene.rs`, `snapshot.rs`, `response_cache.rs`, `cli.rs`

### セキュリティ

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `security/` | `policy.rs` (2.3k), `secrets.rs`, `pairing.rs`, `prompt_guard.rs`, `leak_detector.rs`, `audit.rs`, `otp.rs`, `estop.rs`, `domain_matcher.rs`, + 4 sandbox ファイル | **ポリシーエンジンと強制。** `SecurityPolicy`: 自律レベル（ReadOnly/Supervised/Full）、ワークスペース拘束、コマンド許可リスト、禁止パス、レート制限、コスト上限。 |

Sandboxing: `bubblewrap.rs`, `firejail.rs`, `landlock.rs`, `docker.rs`, `detect.rs`（利用可能な最良手段を自動検出）

### ゲートウェイ（HTTP API）

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `gateway/` | `mod.rs` (2.8k), `api.rs` (1.4k), `sse.rs`, `ws.rs`, `static_files.rs` | **Axum HTTP サーバ。** Webhook（WhatsApp、WATI、Linq、Nextcloud Talk）、REST、SSE、WebSocket。レート制限、べき等キー、64KB ボディ上限、30s タイムアウト。 |

### ハードウェアとペリフェラル

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `peripherals/` | `traits.rs`, `mod.rs`, `serial.rs`, `rpi.rs`, `arduino_flash.rs`, `uno_q_bridge.rs`, `uno_q_setup.rs`, `nucleo_flash.rs`, `capabilities_tool.rs` | **ボード抽象。** `Peripheral`: `connect()`, `disconnect()`, `health_check()`, `tools()`。各ペリフェラルがツールとして能力を公開。 |
| `hardware/` | `discover.rs`, `introspect.rs`, `registry.rs`, `mod.rs` | **USB 検出とボード識別。** VID/PID スキャン、既知ボード照合、接続デバイスの内省。 |

### 可観測性

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `observability/` | `traits.rs`, `mod.rs`, `log.rs`, `prometheus.rs`, `otel.rs`, `verbose.rs`, `noop.rs`, `multi.rs`, `runtime_trace.rs` | **メトリクスとトレース。** `Observer`: `log_event()`。複合オブザーバ（`multi.rs`）が複数バックエンドへファンアウト。 |

### Skills と SkillForge

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `skills/` | `mod.rs` (1.5k), `audit.rs` | **ユーザー/コミュニティ作成の能力。** `~/.zeroclaw/workspace/skills/<name>/SKILL.md` から読込。CLI: list, install, audit, remove。任意で open-skills から同期。 |
| `skillforge/` | `scout.rs`, `evaluate.rs`, `integrate.rs`, `mod.rs` | **スキル探索と評価。** 探索、品質/適合評価、ランタイムへの統合。 |

### SOP（標準作業手順）

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `sop/` | `engine.rs` (1.6k), `metrics.rs` (1.5k), `types.rs`, `dispatch.rs`, `condition.rs`, `gates.rs`, `audit.rs`, `mod.rs` | **ワークフローエンジン。** 条件、ゲート（承認チェックポイント）、メトリクスを含む多段手順。実行、前進、監査が可能。 |

### スケジューリングとライフサイクル

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `cron/` | `scheduler.rs`, `schedule.rs`, `store.rs`, `types.rs`, `mod.rs` | **タスクスケジューラ。** Cron、ワンショット、固定間隔。永続ストア。 |
| `heartbeat/` | `engine.rs`, `mod.rs` | **生存監視。** チャネル/ゲートウェイの定期ヘルスチェック。 |
| `daemon/` | `mod.rs` | **長寿命デーモン。** gateway + channels + heartbeat + scheduler をまとめて起動。 |
| `service/` | `mod.rs` (1.3k) | **OS サービス管理。** systemd または launchd で install/start/stop/restart。 |
| `hooks/` | `mod.rs`, `runner.rs`, `traits.rs`, `builtin/` | **ライフサイクルフック。** イベント時にユーザスクリプト（ツール前後、メッセージ受信など）。 |

### サポートモジュール

| モジュール | 主なファイル | 役割 |
|---|---|---|
| `onboard/` | `wizard.rs` (7.2k), `mod.rs` | **初回セットアップウィザード。** 対話またはクイック: provider、API キー、channel、memory バックエンド。 |
| `auth/` | `profiles.rs`, `anthropic_token.rs`, `gemini_oauth.rs`, `openai_oauth.rs`, `oauth_common.rs` | **認証プロファイルと OAuth。** プロバイダごとの資格情報。 |
| `approval/` | `mod.rs` | **承認ワークフロー。** リスクの高い操作を人間承認の背後に置く。 |
| `doctor/` | `mod.rs` (1.3k) | **診断。** デーモン健全性、スケジューラ鮮度、チャネル接続。 |
| `health/` | `mod.rs` | **ヘルスチェックエンドポイント。** |
| `cost/` | `tracker.rs`, `types.rs`, `mod.rs` | **コスト追跡。** セッション単位・日単位の集計。 |
| `tunnel/` | `cloudflare.rs`, `ngrok.rs`, `tailscale.rs`, `custom.rs`, `none.rs`, `mod.rs` | **トンネルアダプタ。** Cloudflare、ngrok、Tailscale、カスタムでゲートウェイを公開。 |
| `rag/` | `mod.rs` | **RAG。** PDF 抽出、チャンク化サポート。 |
| `integrations/` | `registry.rs`, `mod.rs` | **統合レジストリ。** サードパーティカタログ。 |
| `identity.rs` | (1.5k) | **エージェント同一性。** 名前、説明、ペルソナ。 |
| `multimodal.rs` | — | **マルチモーダル。** 画像/ビジョン設定。 |
| `migration.rs` | — | **データ移行。** OpenClaw ワークスペースからのインポート。 |
| `util.rs` | — | **共有ユーティリティ。** |

---

## src/ 外

| ディレクトリ | 役割 |
|---|---|
| `crates/robot-kit/` | ロボットキット機能用の別 Rust crate |
| `tests/` | 結合/E2E（エージェントループ、設定永続化、チャネルルーティング、プロバイダ解決、Webhook セキュリティ） |
| `benches/` | パフォーマンスベンチ（`agent_benchmarks.rs`） |
| `docs/contributing/extension-examples.md` | カスタム provider/channel/tool/memory の拡張例 |
| `firmware/` | 組込み: `arduino/`, `esp32/`, `esp32-ui/`, `nucleo/`, `uno-q-bridge/` |
| `web/` | Web UI フロント（Vite + TypeScript） |
| `python/` | Python SDK / ブリッジ（独自テスト付き） |
| `dev/` | ローカル開発: Docker Compose、CI（`ci.sh`）、設定テンプレート、サンドボックス |
| `scripts/` | CI 補助、リリース自動化、ブートストラップ、コントリビュータ層計算 |
| `docs/` | ドキュメント: 多言語（en/zh-CN/ja/ru/fr/vi）、ランタイム参照、運用、セキュリティ提案 |
| `.github/` | CI、PR/issue テンプレート、自動化 |

---

## 依存の向き

```
main.rs ──▶ agent/ ──▶ providers/  (LLM 呼び出し)
               │──▶ tools/      (能力実行)
               │──▶ memory/     (コンテキスト永続化)
               │──▶ observability/ (イベントログ)
               │──▶ security/   (ポリシー強制)
               │──▶ config/     (全設定構造体)
               │──▶ runtime/    (プラットフォーム抽象)
               │
main.rs ──▶ channels/ ──▶ agent/ (メッセージルーティング)
main.rs ──▶ gateway/  ──▶ agent/ (HTTP/WS ルーティング)
main.rs ──▶ daemon/   ──▶ gateway/ + channels/ + cron/ + heartbeat/

具体モジュールは内側のトレイト/設定へ依存する。
トレイトは具体実装を import しない。
```

---

## CLI コマンドツリー

```
zeroclaw
├── onboard [--force] [--reinit] [--channels-only]     # 初回セットアップ
├── agent [-m "msg"] [-p provider]        # エージェントループ起動
├── daemon [-p port]                      # フルランタイム（gateway+channels+cron+heartbeat）
├── gateway [-p port]                     # HTTP API のみ
├── channel {list|start|doctor|add|remove|bind-telegram}
├── skill {list|install|audit|remove}
├── memory {list|get|stats|clear}
├── cron {list|add|add-at|add-every|once|remove|update|pause|resume}
├── peripheral {list|add|flash|flash-nucleo|setup-uno-q}
├── hardware {discover|introspect|info}
├── service {install|start|stop|restart|status|uninstall}
├── doctor                                # 診断
├── status                                # システム概要
├── estop [--level] [status|resume]       # 緊急停止
├── migrate openclaw                      # データ移行
├── pair                                  # デバイスペアリング
├── auth-profiles                         # 資格情報管理
├── version / completions                 # メタ
└── config {show|edit|validate|reset}
```
