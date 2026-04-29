# Cài đặt một lệnh

Cách cài đặt và khởi tạo Miroclaw được hỗ trợ nhanh nhất.

Xác minh lần cuối: **2026-02-20**.

## Cách 0: Homebrew (macOS/Linuxbrew)

```bash
brew install miroclaw
```

## Cách A (Khuyến nghị): Clone + script cục bộ

```bash
git clone https://github.com/morpheum-labs/miroclaw.git
cd miroclaw
bash scripts/install.sh
```

Mặc định script sẽ:

1. `cargo build --release --locked`
2. `cargo install --path . --force --locked`

### Kiểm tra tài nguyên và binary dựng sẵn

Build từ mã nguồn thường yêu cầu tối thiểu:

- **2 GB RAM + swap**
- **6 GB dung lượng trống**

Khi tài nguyên hạn chế, bootstrap sẽ thử binary dựng sẵn trước.

```bash
bash scripts/install.sh --prefer-prebuilt
```

Chỉ dùng binary dựng sẵn, báo lỗi nếu không có asset phù hợp:

```bash
bash scripts/install.sh --prebuilt-only
```

Bỏ qua binary dựng sẵn, buộc biên dịch từ mã nguồn:

```bash
bash scripts/install.sh --force-source-build
```

## Bootstrap kép

Mặc định là **chỉ ứng dụng** (build/cài Miroclaw), yêu cầu Rust toolchain sẵn có.

Trên máy mới, bật bootstrap môi trường:

```bash
bash scripts/install.sh --install-system-deps --install-rust
```

Lưu ý:

- `--install-system-deps` cài phụ thuộc biên dịch/build (có thể cần `sudo`).
- `--install-rust` cài Rust qua `rustup` nếu thiếu.
- `--prefer-prebuilt` thử tải binary phát hành trước, sau đó build từ nguồn.
- `--prebuilt-only` tắt fallback build từ nguồn.
- `--force-source-build` tắt hoàn toàn luồng binary dựng sẵn.

## Cách B: Một dòng từ xa

```bash
curl -fsSL https://raw.githubusercontent.com/morpheum-labs/miroclaw/master/scripts/install.sh | bash
```

Môi trường yêu cầu bảo mật cao nên dùng Cách A để đọc script trước.

Nếu chạy Cách B ngoài checkout repo, script sẽ clone workspace tạm, build, cài và dọn dẹp.

## Chế độ onboarding tùy chọn

### Trong container (Docker)

```bash
bash scripts/install.sh --docker
```

Build image Miroclaw cục bộ và chạy onboarding trong container, lưu config/workspace vào `./.zeroclaw-docker`.

CLI container mặc định là `docker`; nếu không có Docker nhưng có `podman`, installer tự chuyển. Có thể đặt `MIROCLAW_CONTAINER_CLI` (ví dụ: `MIROCLAW_CONTAINER_CLI=podman bash scripts/install.sh --docker`).

Với Podman, installer dùng `--userns keep-id` và nhãn `:Z` cho volume.

Nếu thêm `--skip-build`, installer bỏ bước build image cục bộ. Trước tiên thử tag Docker cục bộ (`MIROCLAW_DOCKER_IMAGE`, mặc định: `miroclaw-bootstrap:local`); nếu không có, kéo `ghcr.io/morpheum-labs/miroclaw:latest` và tag cục bộ trước khi chạy.

### Dừng và khởi động lại container Docker/Podman

Sau khi `bash scripts/install.sh --docker` kết thúc, container thoát. Config và workspace được lưu trong thư mục dữ liệu (mặc định: `./.zeroclaw-docker`, hoặc `~/.zeroclaw-docker` khi bootstrap qua `curl | bash`). Ghi đè bằng `MIROCLAW_DOCKER_DATA_DIR`.

**Không chạy lại `install.sh`** chỉ để restart — sẽ rebuild image và chạy lại onboarding. Thay vào đó, khởi động container mới từ image hiện có và mount lại thư mục dữ liệu.

#### Chạy container thủ công (thư mục dữ liệu từ install.sh)

Nếu đã cài qua `bash scripts/install.sh --docker` và muốn tái sử dụng `.zeroclaw-docker` không dùng compose:

```bash
# Docker
docker run -d --name miroclaw \
  --restart unless-stopped \
  -v "$PWD/.zeroclaw-docker/.zeroclaw:/zeroclaw-data/.zeroclaw" \
  -v "$PWD/.zeroclaw-docker/workspace:/zeroclaw-data/workspace" \
  -e HOME=/zeroclaw-data \
  -e MIROCLAW_WORKSPACE=/zeroclaw-data/workspace \
  -p 42617:42617 \
  miroclaw-bootstrap:local \
  gateway

# Podman (thêm --userns keep-id và :Z cho volume)
podman run -d --name miroclaw \
  --restart unless-stopped \
  --userns keep-id \
  --user "$(id -u):$(id -g)" \
  -v "$PWD/.zeroclaw-docker/.zeroclaw:/zeroclaw-data/.zeroclaw:Z" \
  -v "$PWD/.zeroclaw-docker/workspace:/zeroclaw-data/workspace:Z" \
  -e HOME=/zeroclaw-data \
  -e MIROCLAW_WORKSPACE=/zeroclaw-data/workspace \
  -p 42617:42617 \
  miroclaw-bootstrap:local \
  gateway
```

#### Lệnh vòng đời thường dùng

```bash
docker stop miroclaw
docker start miroclaw
docker logs -f miroclaw
docker rm miroclaw
docker exec miroclaw miroclaw status
```

#### Biến môi trường

Khi chạy thủ công, truyền cấu hình provider qua biến môi trường hoặc đảm bảo đã lưu trong `config.toml` được persist:

```bash
docker run -d --name miroclaw \
  -e API_KEY="sk-..." \
  -e PROVIDER="openrouter" \
  -v "$PWD/.zeroclaw-docker/.zeroclaw:/zeroclaw-data/.zeroclaw" \
  -v "$PWD/.zeroclaw-docker/workspace:/zeroclaw-data/workspace" \
  -p 42617:42617 \
  miroclaw-bootstrap:local \
  gateway
```

### Onboarding nhanh (không tương tác)

```bash
bash scripts/install.sh --api-key "sk-..." --provider openrouter
```

Hoặc:

```bash
MIROCLAW_API_KEY="sk-..." MIROCLAW_PROVIDER="openrouter" bash scripts/install.sh
```

## Các cờ hữu ích

- `--install-system-deps`
- `--install-rust`
- `--skip-build` (trong `--docker`: dùng image cục bộ nếu có, nếu không kéo `ghcr.io/morpheum-labs/miroclaw:latest`)
- `--skip-install`
- `--provider <id>`

```bash
bash scripts/install.sh --help
```

## Tài liệu liên quan

- [README.md](../../README.md)
- [commands-reference.md](commands-reference.md)
- [providers-reference.md](providers-reference.md)
- [channels-reference.md](channels-reference.md)
