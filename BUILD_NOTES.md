# Brush 개인 수정판 빌드 노트

## 기준 버전

| 항목 | 값 |
|---|---|
| 원본 저장소 | https://github.com/ArthurBrussee/brush |
| 포크 | https://github.com/qwerzaq56-sketch/brush |
| 기준 커밋 | `5ee2053437bcb5d42d64b361d82cd8c915f87ad1` (main, 2026-09-20, "Let burn generate the Fusion plumbing (#553)"), v0.3.0 이후 198커밋 |
| 작업 브랜치 | `export-streaming` (native portable용, 최신 main 기준) |
| 웹용 브랜치 | `web-demo-base`: 공식 웹 데모와 같은 `e700993a` (2026-04-25, "Update deps & fix WASM") + 같은 Export 수정 |

## 빌드 환경

| 항목 | 버전 |
|---|---|
| OS | Windows 10 Home 10.0.19045 |
| GPU / 드라이버 | NVIDIA GeForce RTX 2060 SUPER 8GB / 596.36 |
| Rust | rustc 1.98.1, cargo 1.98.1 (stable-x86_64-pc-windows-msvc, README 요구사항 1.88+) |
| C/C++ 컴파일러 | Visual Studio Build Tools 2026 18.10 (MSVC 14.51.36231, C++ 워크로드, `F:\VS\BuildTools`). 링커와 `libsqlite3-sys` 등 C 의존성 빌드에 필요. 처음에는 H:의 2022 Build Tools를 썼으나 H 고장으로 다시 설치 |
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

- 결과물: `target/release/brush.exe`. 이번 빌드에서는 `CARGO_TARGET_DIR=target\portable`을 썼습니다 (일반 빌드 캐시와 섞이지 않게).
- `dist/native/` 구성: `brush.exe` (138,285,056 B, SHA-256 `8D32042A…B970D6`), `LICENSE`, 사용 가이드 `README_KO.txt`
- 의존 DLL(`dumpbin /dependents`)은 모두 Windows 기본 DLL입니다 (kernel32, user32, dxgi, opengl32 등). vcruntime은 없습니다. GPU 드라이버 DLL(DX12/Vulkan)은 실행 중에 불러오며, NVIDIA 드라이버에 포함되어 있습니다.
- 공식 배포(cargo-dist, `dist-workspace.toml`)는 installer 없이 zip만 만들고, 기본 `release` 프로필을 씁니다 (crt-static 없음).

## Web 빌드 (데모 기반, 권장)

최신 main의 web 빌드는 이 PC에서 학습이 되지 않습니다 (아래 "알려진 제한사항"). 그래서 공식 GitHub Pages 데모(`ArthurBrussee/brush-demo`, 2026-04-25 배포)와 같은 코드인 `e700993a`에 Export 수정을 옮긴 `web-demo-base` 브랜치로 web을 빌드합니다. 이 시점의 web은 Next.js 기반입니다.

```bash
git worktree add _work/demo-base web-demo-base
cd _work/demo-base/brush_nextjs
npm install
npm run build                 # wasm-pack --release 후 next build (정적 내보내기 → out/)
```

- 결과물 `out/`을 `dist/web-demo/`로 복사했습니다 (파일 32개, 17.2 MB, wasm 16.3 MB). 빌드 약 7분.
- GitHub Pages용 경로(`/brush-demo`)가 필요하면 `NEXT_PUBLIC_BASE_PATH=/brush-demo`를 주고 빌드합니다 (`build-gh-demo` 스크립트).
- 로컬 확인: `node _work/serve-web-demo.mjs dist/web-demo 4175` → `http://localhost:4175/`. 정적 파일 서버라면 무엇이든 되지만, `.wasm`을 `application/wasm`으로 내보내야 합니다.
- Export 수정은 `e700993a`의 API(제네릭 `Splats<B>`, up_axis/min_scale 없음)에 맞춰 옮겼습니다. 자동 체크포인트는 이 시점 런타임에 맞춰 비동기 쓰기를 유지하고, 중간 객체 목록만 없앴습니다 (web에는 자동 저장이 없음).
- 이 브랜치의 `cargo test -p brush-serde --release --features export`: 10개 통과. 기존 방식과 새 방식의 출력이 SH 0–3에서 바이트 단위로 같습니다.

## Web 빌드 (최신 main, 이 PC에서는 학습 불가)

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

### Web (데모 기반, `dist/web-demo`)

- 소규모 학습 + Export 버튼 (사용자 직접 확인, Chrome, 2026-09-27): 테스트 데이터셋으로 학습이 진행되고, Export 버튼으로 `.ply` 다운로드에 성공했습니다.
- 대규모 학습 + Export 버튼 (사용자 직접 확인, Chrome, 2026-09-27): 실제 데이터셋으로 학습한 뒤 **385 MB** PLY 다운로드에 성공했습니다 (약 1.6~1.7M splat, SH3 기준).
- 미확인: 원래 `RuntimeError: unreachable`이 나던 2.76M splat(약 650 MB) 규모의 web Export.

### Web (최신 main)

- production 빌드가 생성되었고, 페이지가 로드되었습니다. WebGPU 어댑터는 nvidia turing으로 인식되었습니다.
- **학습 불가: 수정 전 main에서도 동일하게 재현됩니다** (아래 참고). 그래서 web Export 버튼은 테스트할 수 없었습니다.
- H 드라이브 고장 후 다시 빌드하지 않았습니다 (`dist/web` 없음).

## 알려진 제한사항

1. **최신 main의 web 학습이 이 PC(RTX 2060 SUPER, 드라이버 596.36)에서 동작하지 않습니다.**
   - 데이터셋을 불러오면 GPU 정렬 셰이더(`sort_reduce/sort_scan/sort_scan_add/sort_scatter/scan_blocks`)가 `Invalid ShaderModule`로 생성에 실패합니다.
   - 이어서 `Failed to read tensor data synchronously` panic과 `RuntimeError: unreachable`이 발생합니다.
   - 제 수정이 전혀 들어가지 않은 main 빌드로 Chrome에서도 같은 오류가 재현되었습니다. 이번 Export 수정과는 무관한 기존 문제입니다.
   - 정렬 코드는 #546, #553에서 최근 크게 바뀌었습니다. 원본 저장소에 같은 증상의 이슈는 없습니다 (2026-09-27 검색 기준).
2. **위 web 테스트 중 시스템 전체가 멈췄습니다.**
   - 2026-09-26 23:42, 23:54에 비정상 종료 2회 (Kernel-Power 41, BugcheckCode 0), 23:47에 GPU 드라이버 리셋 (nvlddmkm 153)이 기록되었습니다.
   - 첫 번째 멈춤(23:42) 때는 portable `brush.exe` 학습이 동시에 실행 중이었습니다 (23:31 메모리 급증 경고 기록).
   - 2026-09-27 Chrome 테스트에서는 portable 없이 web(수정 전 main) 단독으로도 화면이 멈췄습니다.
   - 따라서 최신 main web 빌드는 단독으로도 이 PC에서 시스템 멈춤을 일으킵니다. 학습을 시도하는 것은 권장하지 않습니다.
3. 처음 보고된 "native가 로그 없이 종료된" 건은 2026-09-26 13:53 기록에 남아 있습니다.
   - 이전 공식 포터블 `brush_app.exe`가 `0xc0000409`(Rust abort)로 종료되었습니다.
   - 13:25에 같은 프로세스의 메모리 급증 경고가 먼저 있었습니다.
   - 메모리 부족 계열 종료로 보이지만, Export 중이었는지는 로그로 확정할 수 없습니다.
4. web Export(`BlobPartsWriter`)는 컴파일만 확인했고, 브라우저 실측은 하지 못했습니다.
5. 수정 후에도 GPU에서 읽어온 전체 배열(splat 수 × 236 B)은 여전히 한 번에 메모리에 올라갑니다. 이것까지 청크 단위로 나누려면 본문 writer를 직접 구현해야 합니다 (계획서의 2-C). 3.5M splat까지는 필요하지 않았습니다.
6. 자동 체크포인트 저장은 이제 학습 스레드에서 동기적으로 파일에 씁니다 (수 초). 기존에도 Export가 끝날 때까지 학습을 기다렸으므로 동작상 차이는 없습니다.

## 로컬 폴더 구성

모든 작업물은 `F:\Claude\brush` 안에 있습니다. 원래 `H:\Claude\brush`에서 작업했으나, 2026-09-27 H 드라이브(Crucial MX500 SATA SSD)가 입출력 오류 끝에 응답하지 않아 GitHub 포크와 백업본으로 F에 복구했습니다. 아래 `dist\` 결과물은 다시 빌드해야 합니다.

| 경로 | 내용 | git |
|---|---|---|
| `dist\native\` | portable `brush.exe` (최신 main + Export 수정), 사용 가이드 `README_KO.txt` | `.gitignore` |
| `dist\web-demo\` | web 빌드 (공식 데모 시점 `e700993a` + Export 수정). 테스트용 `test_dataset.zip` 포함 | `.gitignore` |
| `_work\demo-base\` | `web-demo-base` 브랜치 작업 폴더 (git worktree) | `.git/info/exclude` |
| `_work\backup\` | Export 수정 패치 백업 | `.git/info/exclude` |
| `_work\serve-web-demo.mjs` | `dist\web-demo` 로컬 확인용 정적 서버 (의존성 없음) | `.git/info/exclude` |
| `target\` | Cargo 빌드 캐시 (삭제해도 됨) | `.gitignore` |
