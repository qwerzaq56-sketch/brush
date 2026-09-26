# Brush 개인 수정판 빌드 노트

## 기준 버전

| 항목 | 값 |
|---|---|
| 원본 저장소 | https://github.com/ArthurBrussee/brush |
| 포크 | https://github.com/qwerzaq56-sketch/brush |
| 기준 커밋 | `5ee2053437bcb5d42d64b361d82cd8c915f87ad1` (main, 2026-09-20, "Let burn generate the Fusion plumbing (#553)"), v0.3.0 이후 198커밋 |
| 작업 브랜치 | `export-streaming` |

## 빌드 환경

| 항목 | 버전 |
|---|---|
| OS | Windows 10 Home 10.0.19045 |
| GPU / 드라이버 | NVIDIA GeForce RTX 2060 SUPER 8GB / 596.36 |
| Rust | rustc 1.98.1, cargo 1.98.1 (stable-x86_64-pc-windows-msvc, README 요구사항 1.88+) |
| C/C++ 컴파일러 | Visual Studio 2022 Build Tools 17.14 (MSVC 14.44.35207, C++ 워크로드). `libsqlite3-sys` 등 C 의존성 빌드에 필요 |
| Node.js / npm | 24.19.0 / 11.17.0 |
| wasm-pack | 0.15.0 (`npm install -g wasm-pack`) |
| WASM 타깃 | `rustup target add wasm32-unknown-unknown` |

CUDA는 필요하지 않습니다. Brush는 wgpu(DX12/Vulkan, 웹에서는 WebGPU)를 사용합니다.

## Native 빌드 (Windows portable)

```bash
cargo build --release -p brush-app --bin brush
```

portable 배포용으로는 C 런타임을 정적으로 링크했습니다. 이렇게 하면 VC++ 재배포 패키지 없이 실행됩니다. PowerShell 기준 명령은 다음과 같습니다.

```bash
$env:RUSTFLAGS = "-C target-feature=+crt-static"; cargo build --release -p brush-app --bin brush
```

- 결과물: `target/release/brush.exe`. 이번 빌드에서는 `CARGO_TARGET_DIR=F:\brush-target-dist`를 썼습니다.
- `dist/native/` 구성: `brush.exe` (138,268,160 B, SHA-256 `D80738EB…8BC4A`), `LICENSE`
- 의존 DLL(`dumpbin /dependents`)은 모두 Windows 기본 DLL입니다 (kernel32, user32, dxgi, opengl32 등). vcruntime은 없습니다. GPU 드라이버 DLL(DX12/Vulkan)은 실행 중에 불러오며, NVIDIA 드라이버에 포함되어 있습니다.
- 공식 배포(cargo-dist, `dist-workspace.toml`)는 installer 없이 zip만 만들고, 기본 `release` 프로필을 씁니다 (crt-static 없음).

## Web 빌드

Web UI는 `apps/brush-app/web`에 있고 Vite + React + wasm-pack 구성입니다. README에는 Next.js라고 적혀 있지만 현재 코드와 다릅니다.

```bash
npm ci                        # 저장소 루트 (npm workspaces)
npm run dev                   # 개발 서버: wasm-pack --dev 빌드 후 vite, http://localhost:5173
npm run build                 # GitHub Pages용: wasm-pack --release 후 vite build, base path /brush-demo
```

로컬에서 base path `/`로 쓰는 production 빌드는 아래처럼 만들었습니다 (`apps/brush-app/web`에서 실행).

```bash
npm run build:wasm-release
npx vite build --outDir ../../../dist/web --emptyOutDir
```

- 결과물 `dist/web/`: `index.html`, `assets/brush_app_bg-*.wasm` (20.8 MB, gzip 6.9 MB), JS 2개
- 로컬 확인: `node node_modules/vite/bin/vite.js preview apps/brush-app/web --outDir ../../../dist/web --port 4173`. 같은 설정이 `.claude/launch.json`의 `brush-web-preview`에 있습니다.
- `wasm-opt -Oz --converge` 단계가 오래 걸립니다 (이 PC 기준 약 15분). 확인용으로만 빌드할 때는 `wasm-pack build .. --release --no-opt`로 약 1분이면 됩니다.

## Export 수정 내용

### 문제 원인 (수정 전 main)

Export는 native와 web이 같은 경로를 탑니다.
- Export 버튼: `training_panel.rs`
- 자동 체크포인트: `train_stream.rs`

두 곳 모두 `splat_to_ply()`로 전체 파일을 `Vec<u8>`로 만든 뒤 저장합니다. 2.76M splats, SH3 기준으로 splat 하나당 float 59개(236 B)입니다. 이때 메모리를 차지하는 단계는 다음과 같습니다.

1. GPU에서 CPU로 전체 데이터를 읽어옵니다 (~650 MB).
2. `Vec<DynamicPlyGaussian>`을 만듭니다. splat마다 SH용 `Vec`을 따로 할당합니다 (~750 MB, 할당 276만 회).
3. `serde_ply::to_bytes`가 빈 Vec을 두 배씩 늘립니다. 재할당 순간 512 MB와 1 GB가 동시에 존재합니다.
4. Web에서는 `Uint8Array::from`으로 JS 쪽에 한 번 더 복사합니다 (+650 MB).

그 결과 wasm32의 4 GB 한계에 닿아 메모리 할당이 실패하고, WASM에서는 이것이 `RuntimeError: unreachable`로 나타납니다.

### 수정 방식

PLY 포맷, 필드 순서와 이름, 쿼터니언 정규화, `bake_min_scale`, up_axis와 코멘트 처리는 모두 그대로 두었습니다.

- `crates/brush-serde/src/export.rs`
  - `PlyExport`를 추가했습니다. GPU에서 읽은 flat 배열만 보관하고, 행은 쓸 때 즉석에서 만듭니다. 그래서 `Vec<DynamicPlyGaussian>`이 없어졌습니다.
  - `serde_ply::to_writer`에 lazy 시퀀스를 넘깁니다. serde_ply는 헤더를 만들 때 첫 행만 보고, 본문은 writer에 바로 씁니다.
  - 새 API: `prepare_ply_export`, `PlyExport::write_to(impl Write)`, `splat_to_ply_writer`
  - 기존 `splat_to_ply()` → `Vec<u8>`는 호환용으로 남겼고, 정확한 크기로 미리 할당하도록 바꿨습니다.
  - 읽어온 배열 길이 검증을 추가했습니다.
- `crates/rrfd/src/lib.rs`: `save_file_with(name, |w| ...)`를 추가했습니다. native는 저장 대화상자에서 고른 파일에 `BufWriter`(1 MB)로 바로 스트리밍합니다.
- `crates/rrfd/src/wasm.rs`
  - `BlobPartsWriter`를 추가했습니다. 16 MB 단위로 JS `Uint8Array` 조각을 만들어 `Blob`으로 묶습니다. WASM 메모리에는 16 MB 버퍼 하나만 남습니다.
  - 기존 `save_file`도 조각 단위로 복사하도록 바꿨습니다.
  - 다운로드 URL 해제를 클릭 직후에서 60초 뒤로 늦췄습니다. 큰 파일 다운로드가 중간에 끊기는 것을 막기 위해서입니다.
- `apps/brush-app/src/ui/training_panel.rs`: Export 버튼이 `prepare_ply_export` + `save_file_with`를 쓰도록 바꿨습니다.
- `crates/brush-process/src/train_stream.rs`: 자동 체크포인트가 파일에 바로 스트리밍하도록 바꿨습니다.
- `.gitignore`: `/dist`를 추가했습니다.

변경 규모: 7개 파일, +489 / −168 (대부분 export.rs의 테스트와 기존 구현 보존분)

## 테스트 결과

### 단위 테스트 (`cargo test -p brush-serde --release`): 12개 통과

- `test_streaming_matches_legacy_bytes`: 수정 전 exporter를 테스트 모듈에 그대로 보존해 두고 출력을 비교했습니다. SH 0/1/2/3, up_axis 유무, 정규화되지 않은 쿼터니언에서 **바이트 단위로 동일**합니다.
- 기존 export/import 왕복 테스트와 SH 차수별 필드 수 테스트도 모두 통과했습니다.

### 대용량 Export 메모리

측정 방법: `export::tests::large_export` (native release)를 실행하면서 외부에서 50 ms마다 메모리를 샘플링했습니다. SH3 합성 데이터입니다.

| splats | 방식 | Export 중 private 메모리 증가 | Working set 최대 | 시간 | 파일 |
|---|---|---|---|---|---|
| 2,764,885 | 수정 전 | +4,107 MB | 2,878 MB | 3.3 s | 652,514,445 B |
| 2,764,885 | 수정 후 | **+1,897 MB** | **1,435 MB** | 2.6 s | 652,514,445 B |
| 3,500,000 | 수정 후 | +2,374 MB | 1,742 MB | 3.1 s | 826,001,585 B, 재로드 정상 |

수정 후 증가분은 GPU readback 스테이징 버퍼와 읽어온 배열이 대부분입니다. Web에서는 WASM 메모리에 readback 배열(~650 MB)과 16 MB 버퍼만 남는 구조입니다. 다만 브라우저에서 실측하지는 못했습니다 (아래 "알려진 제한사항" 참고).

직접 실행하는 명령:

```bash
BRUSH_EXPORT_SPLATS=2764885 cargo test -p brush-serde --release -- --ignored large_export --nocapture
```

`BRUSH_EXPORT_LEGACY=1`을 주면 수정 전 방식으로 측정합니다.

### Native

- `cargo check`: native와 wasm32 모두 통과했습니다 (app, process, rrfd).
- `dist/native`를 다른 폴더(F:)에 복사해서 실행했습니다: `brush.exe <test_dataset> --total-train-iters 300 --export-every 100`
  - 결과: 학습 10초, 자동 Export 3회 성공 (`export_100/200/300.ply`)
  - PLY 헤더: 속성 59개, `binary_little_endian`, 코멘트 정상. 파일 크기가 계산값과 정확히 일치했습니다.
- GUI 학습 + Export 버튼 (사용자 직접 확인, 2026-09-27): portable `brush.exe`로 학습한 뒤 저장 대화상자를 거쳐 `.ply` 저장에 성공했습니다. 파일 크기 약 200~400 MB 범위에서 확인했습니다.
- 미확인: 2~3M splat(600 MB 이상) 규모의 GUI Export. 같은 코드 경로를 `large_export` 테스트로 2.76M/3.5M까지 확인했습니다.

### Web

- production 빌드가 생성되었고, 페이지가 로드되었습니다. WebGPU 어댑터는 nvidia turing으로 인식되었습니다.
- **학습 불가: 수정 전 main에서도 동일하게 재현됩니다** (아래 참고). 그래서 web Export 버튼은 테스트할 수 없었습니다.

## 알려진 제한사항

1. **최신 main의 web 학습이 이 PC(RTX 2060 SUPER, 드라이버 596.36)에서 동작하지 않습니다.**
   - 데이터셋을 불러오면 GPU 정렬 셰이더(`sort_reduce/sort_scan/sort_scan_add/sort_scatter/scan_blocks`)가 `Invalid ShaderModule`로 생성에 실패합니다.
   - 이어서 `Failed to read tensor data synchronously` panic과 `RuntimeError: unreachable`이 발생합니다.
   - 제 수정이 전혀 들어가지 않은 main 빌드로 Chrome에서도 같은 오류가 재현되었습니다. 이번 Export 수정과는 무관한 기존 문제입니다.
   - 정렬 코드는 #546, #553에서 최근 크게 바뀌었습니다. 원본 저장소에 같은 증상의 이슈는 없습니다 (2026-09-27 검색 기준).
2. **위 web 테스트 중 시스템 전체가 멈췄습니다.**
   - 2026-09-26 23:42, 23:54에 비정상 종료 2회 (Kernel-Power 41, BugcheckCode 0), 23:47에 GPU 드라이버 리셋 (nvlddmkm 153)이 기록되었습니다.
   - 2026-09-27 Chrome 테스트 때도 화면 멈춤이 있었습니다.
   - 이 PC에서 최신 main web 빌드로 학습을 시도하는 것은 권장하지 않습니다.
3. web Export(`BlobPartsWriter`)는 컴파일만 확인했고, 브라우저 실측은 하지 못했습니다.
4. 수정 후에도 GPU에서 읽어온 전체 배열(splat 수 × 236 B)은 여전히 한 번에 메모리에 올라갑니다. 이것까지 청크 단위로 나누려면 본문 writer를 직접 구현해야 합니다 (계획서의 2-C). 3.5M splat까지는 필요하지 않았습니다.
5. 자동 체크포인트 저장은 이제 학습 스레드에서 동기적으로 파일에 씁니다 (수 초). 기존에도 Export가 끝날 때까지 학습을 기다렸으므로 동작상 차이는 없습니다.

## 참고: 작업 중 만든 로컬 폴더 (저장소 밖)

- `F:\brush-target`, `F:\brush-target-dist`, `F:\brush-target-wasm`: 빌드 캐시 (삭제해도 됨)
- `F:\brush-main`: 수정 전 main 비교용 git worktree. 정리 명령: `git worktree remove F:\brush-main`
- `F:\brush-main-web`: 수정 전 main의 web 빌드 (비교용)
- `F:\brush-backup\export-streaming.patch`: 수정 코드 백업
