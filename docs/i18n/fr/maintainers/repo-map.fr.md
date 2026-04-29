# Carte du dépôt ZeroClaw

ZeroClaw est un runtime d’agent autonome axé sur Rust. Il reçoit des messages depuis les plateformes de messagerie, les achemine vers un LLM, exécute des appels d’outils, persiste la mémoire et renvoie des réponses. Il peut aussi piloter du matériel périphérique et tourner comme démon longue durée.

## Flux d’exécution

```
Message utilisateur (Telegram/Discord/Slack/...)
        │
        ▼
   ┌─────────┐     ┌────────────┐
   │ Channel  │────▶│   Agent    │  (src/agent/)
   └─────────┘     │  Loop      │
                   │            │◀──── Memory Loader (contexte pertinent)
                   │            │◀──── System Prompt Builder
                   │            │◀──── Query Classifier (routage modèle)
                   └─────┬──────┘
                         │
                         ▼
                   ┌───────────┐
                   │  Provider  │  (LLM: Anthropic, OpenAI, Gemini, etc.)
                   └─────┬─────┘
                         │
                    appels d’outils ?
                    ┌────┴────┐
                    ▼         ▼
               ┌────────┐  réponse texte
               │  Tools  │     │
               └────┬───┘     │
                    │         │
                    ▼         ▼
              renvoyer       renvoyer
              résultats      via Channel
              au LLM
```

---

## Arborescence racine

```
zeroclaw/
├── src/                  # Source Rust (le runtime)
├── crates/robot-kit/     # Crate séparé pour le kit robot matériel
├── tests/                # Tests d’intégration / E2E
├── benches/              # Benchmarks (boucle agent)
├── docs/contributing/extension-examples.md  # Exemples d’extension (provider/canal/outil/mémoire)
├── firmware/             # Firmware embarqué Arduino, ESP32, Nucleo
├── web/                  # Web UI (Vite + TypeScript)
├── python/               # SDK Python / pont d’outils
├── dev/                  # Outils de dev local (Docker, scripts CI, sandbox)
├── scripts/              # CI, automatisation des releases, bootstrap
├── docs/                 # Documentation (multilingue, références runtime)
├── .github/              # Workflows CI, modèles PR, automatisation
├── playground/           # (vide, espace d’essais)
├── Cargo.toml            # Manifeste du workspace
├── Dockerfile            # Build conteneur
├── flake.nix             # Environnement de dev Nix
└── install.sh            # Script d’installation en une commande
```

---

## src/ — module par module

### Points d’entrée

| Fichier | Lignes | Rôle |
|---|---|---|
| `main.rs` | 1,977 | Entrée CLI. Parser Clap, dispatch des commandes. Tout le routage `zeroclaw <sous-commande>`. |
| `lib.rs` | 436 | Déclarations de modules, visibilité (`pub` vs `pub(crate)`), énumérations CLI partagées (`ServiceCommands`, `ChannelCommands`, `SkillCommands`, etc.) entre lib et binaire. |

### Runtime principal

| Module | Fichiers clés | Rôle |
|---|---|---|
| `agent/` | `agent.rs`, `loop_.rs` (5.6k), `system_prompt.rs`, `dispatcher.rs`, `prompt.rs`, `classifier.rs`, `memory_loader.rs` | **Le cerveau.** `AgentBuilder` compose provider+outils+mémoire+observer. `system_prompt.rs` assemble le prompt système de l’espace de travail (statique vs dynamique ; marqueur de frontière pour le cache côté fournisseur). `channels/mod.rs` délègue `build_system_prompt_*` ici ; `loop_.rs` peut réécrire le premier message system après compaction lorsque `system_prompt_refresh` est passé. Le dispatcher gère l’analyse des appels d’outils natifs vs XML. Le classifier route les requêtes vers les modèles. |
| `config/` | `schema.rs` (7.6k), `mod.rs`, `traits.rs` | **Toutes les structures de configuration.** Chaque sous-système vit dans `schema.rs` — providers, canaux, mémoire, sécurité, passerelle, outils, matériel, planification, etc. Chargement depuis TOML. |
| `runtime/` | `native.rs`, `docker.rs`, `wasm.rs`, `traits.rs` | **Adaptateurs de plateforme.** Le trait `RuntimeAdapter` abstrait l’accès shell, le système de fichiers, les chemins de stockage, les budgets mémoire. Native = OS direct. Docker = isolation conteneur. WASM = expérimental. |

### Fournisseurs LLM

| Module | Fichiers clés | Rôle |
|---|---|---|
| `providers/` | `traits.rs`, `mod.rs` (2.9k), `reliable.rs`, `router.rs`, + 11 fichiers | **Intégrations LLM.** Trait `Provider` : `chat()`, `chat_with_system()`, `capabilities()`, `convert_tools()`. Fabrique dans `mod.rs`. `ReliableProvider` enveloppe avec retry/chaînes de repli. `RoutedProvider` route selon les indices du classifier. |

Providers: `anthropic`, `openai`, `openai_codex`, `openrouter`, `gemini`, `ollama`, `compatible` (compat OpenAI), `copilot`, `bedrock`, `telnyx`, `glm`

### Canaux de messagerie

| Module | Fichiers clés | Rôle |
|---|---|---|
| `channels/` | `traits.rs`, `mod.rs` (6.6k), + 22 fichiers | **Transports E/S.** Trait `Channel` : `send()`, `listen()`, `health_check()`, `start_typing()`, brouillons. La fabrique dans `mod.rs` relie la config aux instances, gère l’historique par expéditeur (max 50 messages). |

Channels: `telegram` (4.6k), `discord`, `slack`, `whatsapp`, `whatsapp_web`, `matrix`, `signal`, `email_channel`, `qq`, `dingtalk`, `lark`, `imessage`, `irc`, `nostr`, `mattermost`, `nextcloud_talk`, `wati`, `mqtt`, `linq`, `clawdtalk`, `cli`

### Outils (capacités de l’agent)

| Module | Fichiers clés | Rôle |
|---|---|---|
| `tools/` | `traits.rs`, `mod.rs` (635), + 38 fichiers | **Ce que l’agent peut faire.** Trait `Tool` : `name()`, `description()`, `parameters_schema()`, `execute()`. Deux registres : `default_tools()` (6 essentiels) et `all_tools_with_runtime()` (jeu complet, selon config). |

Catégories d’outils :
- **File/Shell** : `shell`, `file_read`, `file_write`, `file_edit`, `glob_search`, `content_search`
- **Memory** : `memory_store`, `memory_recall`, `memory_forget`
- **Web** : `browser`, `browser_open`, `web_fetch`, `web_search_tool`, `http_request`
- **Scheduling** : `cron_add`, `cron_list`, `cron_remove`, `cron_update`, `cron_run`, `cron_runs`, `schedule`
- **Delegation** : `delegate` (sous-agents), `composio` (intégrations OAuth)
- **Hardware** : `hardware_board_info`, `hardware_memory_map`, `hardware_memory_read`
- **SOP** : `sop_execute`, `sop_advance`, `sop_approve`, `sop_list`, `sop_status`
- **Utility** : `git_operations`, `image_info`, `pdf_read`, `screenshot`, `pushover`, `model_routing_config`, `proxy_config`, `cli_discovery`, `schema`

### Mémoire

| Module | Fichiers clés | Rôle |
|---|---|---|
| `memory/` | `traits.rs`, `backend.rs`, `mod.rs`, + 8 fichiers | **Connaissance persistante.** Trait `Memory` : `store()`, `recall()`, `get()`, `list()`, `forget()`, `count()`. Catégories : Core, Daily, Conversation, Custom. |

Backends: `sqlite`, `markdown`, `lucid` (SQLite + embeddings), `qdrant` (vecteur), `postgres`, `none`

Supporting: `embeddings.rs`, `vector.rs`, `chunker.rs`, `hygiene.rs`, `snapshot.rs`, `response_cache.rs`, `cli.rs`

### Sécurité

| Module | Fichiers clés | Rôle |
|---|---|---|
| `security/` | `policy.rs` (2.3k), `secrets.rs`, `pairing.rs`, `prompt_guard.rs`, `leak_detector.rs`, `audit.rs`, `otp.rs`, `estop.rs`, `domain_matcher.rs`, + 4 fichiers sandbox | **Moteur de politique et application.** `SecurityPolicy` : niveaux d’autonomie (ReadOnly/Supervised/Full), confinement de l’espace de travail, listes d’autorisation de commandes, chemins interdits, limites de débit, plafonds de coût. |

Sandboxing: `bubblewrap.rs`, `firejail.rs`, `landlock.rs`, `docker.rs`, `detect.rs` (détection automatique du meilleur disponible)

### Passerelle (API HTTP)

| Module | Fichiers clés | Rôle |
|---|---|---|
| `gateway/` | `mod.rs` (2.8k), `api.rs` (1.4k), `sse.rs`, `ws.rs`, `static_files.rs` | **Serveur HTTP Axum.** Webhooks (WhatsApp, WATI, Linq, Nextcloud Talk), API REST, flux SSE, WebSocket. Limitation de débit, clés d’idempotence, corps 64 Ko, timeout 30 s. |

### Matériel et périphériques

| Module | Fichiers clés | Rôle |
|---|---|---|
| `peripherals/` | `traits.rs`, `mod.rs`, `serial.rs`, `rpi.rs`, `arduino_flash.rs`, `uno_q_bridge.rs`, `uno_q_setup.rs`, `nucleo_flash.rs`, `capabilities_tool.rs` | **Abstraction carte matérielle.** Trait `Peripheral` : `connect()`, `disconnect()`, `health_check()`, `tools()`. |
| `hardware/` | `discover.rs`, `introspect.rs`, `registry.rs`, `mod.rs` | **Découverte USB et identification des cartes.** VID/PID, correspondance, introspection des périphériques connectés. |

### Observabilité

| Module | Fichiers clés | Rôle |
|---|---|---|
| `observability/` | `traits.rs`, `mod.rs`, `log.rs`, `prometheus.rs`, `otel.rs`, `verbose.rs`, `noop.rs`, `multi.rs`, `runtime_trace.rs` | **Métriques et traçage.** Trait `Observer` : `log_event()`. Observateur composite (`multi.rs`) vers plusieurs backends. |

### Skills et SkillForge

| Module | Fichiers clés | Rôle |
|---|---|---|
| `skills/` | `mod.rs` (1.5k), `audit.rs` | **Capacités utilisateur / communauté.** Chargement depuis `~/.zeroclaw/workspace/skills/<name>/SKILL.md`. CLI : list, install, audit, remove. Sync communautaire optionnelle depuis open-skills. |
| `skillforge/` | `scout.rs`, `evaluate.rs`, `integrate.rs`, `mod.rs` | **Découverte et évaluation de skills.** |

### SOP

| Module | Fichiers clés | Rôle |
|---|---|---|
| `sop/` | `engine.rs` (1.6k), `metrics.rs` (1.5k), `types.rs`, `dispatch.rs`, `condition.rs`, `gates.rs`, `audit.rs`, `mod.rs` | **Moteur de procédures.** Procédures multi-étapes avec conditions, portes (points d’approbation) et métriques. |

### Planification et cycle de vie

| Module | Fichiers clés | Rôle |
|---|---|---|
| `cron/` | `scheduler.rs`, `schedule.rs`, `store.rs`, `types.rs`, `mod.rs` | **Planificateur.** Expressions cron, timers one-shot, intervalles fixes. Stockage persistant. |
| `heartbeat/` | `engine.rs`, `mod.rs` | **Surveillance de vivacité.** Contrôles périodiques canaux/passerelle. |
| `daemon/` | `mod.rs` | **Démon longue durée.** Démarre passerelle + canaux + heartbeat + planificateur. |
| `service/` | `mod.rs` (1.3k) | **Gestion de service OS.** systemd ou launchd. |
| `hooks/` | `mod.rs`, `runner.rs`, `traits.rs`, `builtin/` | **Hooks de cycle de vie.** |

### Modules de support

| Module | Fichiers clés | Rôle |
|---|---|---|
| `onboard/` | `wizard.rs` (7.2k), `mod.rs` | **Assistant de premier lancement.** |
| `auth/` | `profiles.rs`, `anthropic_token.rs`, `gemini_oauth.rs`, `openai_oauth.rs`, `oauth_common.rs` | **Profils d’auth et OAuth.** |
| `approval/` | `mod.rs` | **Flux d’approbation.** |
| `doctor/` | `mod.rs` (1.3k) | **Diagnostics.** |
| `health/` | `mod.rs` | **Endpoints de santé.** |
| `cost/` | `tracker.rs`, `types.rs`, `mod.rs` | **Suivi des coûts.** |
| `tunnel/` | `cloudflare.rs`, `ngrok.rs`, `tailscale.rs`, `custom.rs`, `none.rs`, `mod.rs` | **Adaptateurs de tunnel.** |
| `rag/` | `mod.rs` | **RAG.** PDF, découpage en chunks. |
| `integrations/` | `registry.rs`, `mod.rs` | **Registre d’intégrations.** |
| `identity.rs` | (1.5k) | **Identité de l’agent.** |
| `multimodal.rs` | — | **Support multimodal.** |
| `migration.rs` | — | **Migration de données.** OpenClaw. |
| `util.rs` | — | **Utilitaires partagés.** |

---

## Hors src/

| Répertoire | Rôle |
|---|---|
| `crates/robot-kit/` | Crate séparé pour le kit robot |
| `tests/` | Tests d’intégration et E2E |
| `benches/` | Benchmarks (`agent_benchmarks.rs`) |
| `docs/contributing/extension-examples.md` | Exemples d’extension |
| `firmware/` | Firmware : `arduino/`, `esp32/`, `esp32-ui/`, `nucleo/`, `uno-q-bridge/` |
| `web/` | Frontend Web UI |
| `python/` | SDK Python / pont |
| `dev/` | Docker Compose, script CI (`ci.sh`), modèles, sandbox |
| `scripts/` | CI, releases, bootstrap, calcul des niveaux contributeurs |
| `docs/` | Documentation multilingue (en/zh-CN/ja/ru/fr/vi), références, runbooks ops, propositions sécurité |
| `.github/` | CI, modèles PR/issue, automatisation |

---

## Sens des dépendances

```
main.rs ──▶ agent/ ──▶ providers/  (appels LLM)
               │──▶ tools/      (exécution des capacités)
               │──▶ memory/     (persistance du contexte)
               │──▶ observability/ (journalisation)
               │──▶ security/   (application des politiques)
               │──▶ config/     (structures de config)
               │──▶ runtime/    (abstraction plateforme)
               │
main.rs ──▶ channels/ ──▶ agent/ (routage messages)
main.rs ──▶ gateway/  ──▶ agent/ (routage HTTP/WS)
main.rs ──▶ daemon/   ──▶ gateway/ + channels/ + cron/ + heartbeat/

Les modules concrets dépendent vers l’intérieur des traits/config.
Les traits n’importent jamais d’implémentations concrètes.
```

---

## Arbre des commandes CLI

```
zeroclaw
├── onboard [--force] [--reinit] [--channels-only]     # Premier lancement
├── agent [-m "msg"] [-p provider]        # Boucle agent
├── daemon [-p port]                      # Runtime complet
├── gateway [-p port]                     # Serveur HTTP API seul
├── channel {list|start|doctor|add|remove|bind-telegram}
├── skill {list|install|audit|remove}
├── memory {list|get|stats|clear}
├── cron {list|add|add-at|add-every|once|remove|update|pause|resume}
├── peripheral {list|add|flash|flash-nucleo|setup-uno-q}
├── hardware {discover|introspect|info}
├── service {install|start|stop|restart|status|uninstall}
├── doctor                                # Diagnostics
├── status                                # Vue d’ensemble
├── estop [--level] [status|resume]       # Arrêt d’urgence
├── migrate openclaw                      # Migration de données
├── pair                                  # Appairage
├── auth-profiles                         # Gestion des identifiants
├── version / completions                 # Meta
└── config {show|edit|validate|reset}
```
