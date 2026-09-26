> [!NOTE]
> **이 저장소는 [ArthurBrussee/brush](https://github.com/ArthurBrussee/brush)의 개인 수정판입니다.**
>
> 원본 Brush는 수백만 개 splat을 PLY로 저장할 때 메모리를 한꺼번에 많이 써서, 웹에서는 `RuntimeError: unreachable`로 죽고 native에서도 갑자기 꺼지는 문제가 있었습니다. 이 수정판은 PLY를 한 줄씩 바로 파일에 쓰도록 바꿔 Export 중 메모리 사용을 절반 이하로 줄였습니다 (2.76M splat 기준 +4.1 GB → +1.9 GB). 저장되는 PLY 파일은 원본과 바이트 단위로 같습니다.
>
> | | 기반 버전 | 사용법 |
> |---|---|---|
> | **웹 데모** | **구버전**: 원본 2026-04-25 (`e700993a`, 원본 공식 웹 데모와 같은 시점) | https://qwerzaq56-sketch.github.io/brush/ (Chrome 최신 버전, 설치 없이 주소만 열면 됩니다) |
> | **Windows 포터블** | **신버전**: 원본 2026-09-20 (`5ee20534`, 작업 시점 최신 main) | [Releases](https://github.com/qwerzaq56-sketch/brush/releases)에서 zip을 받아 압축을 풀고 `brush.exe` 실행. 사용법은 zip 안의 `README_KO.txt` |
> | **빌드 방법 · 수정 내용 · 테스트 결과** | | [BUILD_NOTES.md](BUILD_NOTES.md) |
>
> 두 버전 모두 같은 Export 수정이 들어 있습니다. 다만 웹 데모는 약 5개월 전 코드라, 그 이후 원본에 추가된 기능과 개선이 없습니다. 예) RealityCapture/Postshot 데이터셋 지원, 어안(fisheye) 카메라 모델, 데이터셋 단위(units_per_meter) 설정, 퇴화 splat을 줄이는 학습 개선. 최신 기능이 필요하면 포터블을 쓰세요.
>
> 브랜치 구성
> - `export-streaming` (기본): 최신 원본 main(2026-09-20) + Export 수정. Windows 포터블은 이 브랜치로 빌드합니다.
> - `web-demo-base`: 원본의 공식 웹 데모와 같은 시점(2026-04-25) + Export 수정. 웹 데모는 이 브랜치로 빌드합니다. 최신 main의 웹 빌드는 개발 PC(RTX 2060 SUPER)에서 학습이 되지 않았기 때문입니다.
> - `gh-pages`: 웹 데모 배포용 빌드 결과물
> - `main`: 원본과 동일 (업데이트 동기화용)
>
> 아래부터는 원본 README입니다.

# Brush

<video src=https://github.com/user-attachments/assets/5756967a-846c-44cf-bde9-3ca4c86f1a4d>A video showing various Brush features and scenes</video>

<p align="center">
  <i>
    Massive thanks to <a href="https://www.youtube.com/@gradeeterna">@GradeEterna</a> for the beautiful scenes
  </i>
</p>

Brush is a 3D reconstruction engine using [Gaussian splatting](https://repo-sam.inria.fr/fungraph/3d-gaussian-splatting/). It works on a wide range of systems: **macOS/windows/linux**, **AMD/Nvidia/Intel** cards, **Android**, and in a **browser**. To achieve this, it uses WebGPU compatible tech and the [Burn](https://github.com/tracel-ai/burn) machine learning framework.

Machine learning for real time rendering has tons of potential, but most ML tools don't work well with it: Rendering requires realtime interactivity, usually involve dynamic shapes & computations, don't run on most platforms, and it can be cumbersome to ship apps with large CUDA deps. Brush on the other hand produces simple dependency free binaries, runs on nearly all devices, without any setup.

[**Try the web demo** <img src="https://cdn-icons-png.flaticon.com/256/888/888846.png" alt="chrome logo" width="24"/>
](https://arthurbrussee.github.io/brush-demo)
_NOTE: Only works on Chrome and Edge. Firefox and Safari are hopefully supported soon)_

[![](https://dcbadge.limes.pink/api/server/https://discord.gg/TbxJST2BbC)](https://discord.gg/TbxJST2BbC)

# Features

## Training

Brush takes in COLMAP data or datasets in the Nerfstudio format. Training is fully supported natively, on mobile, and in a browser. While training you can interact with the scene and see the training dynamics live, and compare the current rendering to input views as the training progresses.

It also supports masking images:
- Images with transparency. This will force the final splat to match the transparency of the input.
- A folder of images called 'masks'. This ignores parts of the image that are masked out.
  Black pixels in the mask are ignored, white pixels are kept. Pass `--invert-masks` if your masks are the other way around.

## Viewer
Brush also works well as a splat viewer, including on the web. It can load .ply & .compressed.ply files. You can stream in data from a URL (for a web app, simply append `?url=`).

Brush also can load .zip of splat files to display them as an animation, or a special ply that includes delta frames (see [cat-4D](https://cat-4d.github.io/) and [Cap4D](https://felixtaubner.github.io/cap4d/)!).

## CLI
Brush can be used as a CLI. Run `brush --help` to get an overview. Every CLI command can work with `--with-viewer` which also opens the UI, for easy debugging.

## Rerun

https://github.com/user-attachments/assets/f679fec0-935d-4dd2-87e1-c301db9cdc2c

While training, additional data can be visualized with the excellent [rerun](https://rerun.io/). To install rerun on your machine, please follow their [instructions](https://rerun.io/docs/getting-started/installing-viewer). Open the ./brush_blueprint.rbl in the viewer for best results.

## Building Brush
First install rust 1.88+. You can run tests with `cargo test --all`. Brush uses the wonderful [rerun](https://rerun.io/) for additional visualizations while training, run `cargo install rerun-cli` if you want to use it.

### Windows/macOS/Linux
Use `cargo run --release` from the workspace root to make an optimized build. Use `cargo run` to run a debug build. 

### Web
Brush can be compiled to WASM. Run `npm run dev` to start the demo website using Next.js, see the web directory in app/brush-app/web.

Brush uses [`wasm-pack`](https://drager.github.io/wasm-pack/) to build the WASM bundle. You can also use it without a bundler, see [wasm-pack's documentation](https://drager.github.io/wasm-pack/book/).

WebGPU is still an upcoming standard, and as such, only Chrome 134+ on Windows and macOS is currently supported.

### Android

As a one time setup, make sure you have the Android SDK & NDK installed.
- Check if ANDROID_NDK_HOME and ANDROID_HOME are set
- Add the Android target to rust `rustup target add aarch64-linux-android`
- Install cargo-ndk to manage building a lib `cargo install cargo-ndk`

Each time you change the rust code, run
- `cargo ndk -t arm64-v8a -o crates/brush-app/app/src/main/jniLibs/ build`
- Nb:  Nb, for best performance, build in release mode. This is separate
  from the Android Studio app build configuration.
- `cargo ndk -t arm64-v8a -o crates/brush-app/app/src/main/jniLibs/  build --release`

You can now either run the project from Android Studio (Android Studio does NOT build the rust code), or run it from the command line:
```
./gradlew build
./gradlew installDebug
adb shell am start -n com.splats.app/.MainActivity
```

You can also open this folder as a project in Android Studio and run things from there. Nb: Running in Android Studio does _not_ rebuild the rust code automatically.

## Benchmarks

Rendering and training are generally faster than gsplat. You can run benchmarks of some of the kernels using `cargo bench`.

# Acknowledgements

[**gSplat**](https://github.com/nerfstudio-project/gsplat), for their reference version of the kernels

**Peter Hedman, George Kopanas & Bernhard Kerbl**, for the many discussions & pointers.

**The Burn team**, for help & improvements to Burn along the way

**Raph Levien**, for the [original version](https://github.com/googlefonts/compute-shader-101/pull/31) of the GPU radix sort.

**GradeEterna**, for feedback and their scenes.

# Disclaimer

This is *not* an official Google product. This repository is a forked public version of [the google-research repository](https://github.com/google-research/google-research/tree/master/brush_splat)
