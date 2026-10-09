# macOS 开发与验证

## 本轮计划（2026-10-09）

基线：`b47120e`，分支 `feature/macos-support`。项目已支持 macOS 14+、
Metal 与 universal2 发布。本轮补齐本地开发包生成和启动验证，属于非 vendor 改动。

1. 准备 Rust 1.98.0 与 Xcode Command Line Tools，先编译现有代码，记录平台问题。
2. 提供本地 `.app` 构建入口，复用发布用 Info.plist 与已有图标资源；本地与 CI
   共用 bundle 组装逻辑，校验 plist 和 ad-hoc 签名，避免只测裸二进制。
3. 支持本机架构快速开发，以及可选的 Intel + Apple Silicon universal2 构建。
4. 在本机运行六项门禁，生成并启动开发包；记录能确认的结果。macOS 14、Intel
   实机、中文输入法候选框跟随和连续写作长跑，必须有对应实测才能判定通过。
5. 将改动整理为独立提交，准备 PR 说明和验证证据。

不在本轮扩大 UI 重设计、修改 vendor、升级依赖或自动发布版本。

## 真机测试发现与修复

- Git 路径别名：libgit2 把 `/var` 解析为 `/private/var`，文件树与标签仍保留
  用户选择的路径，导致状态角标缺失、回滚之后编辑器未重载。仓库发现现在保留
  指向同一工作区的用户路径拼写，新增符号链接工作区回归测试。
- 空 Git 仓库：未提交的 `main` 分支不能只依赖 libgit2 的 `is_empty` 判定。
  历史与 diff 显式处理 unborn HEAD；测试固定初始分支为 `main`，不依赖个人配置。
- MCP HTTP：macOS/BSD 的 accept 连接可继承非阻塞模式，同步解析收到 `WouldBlock`
  就会中断请求。连接处理前显式切回阻塞模式，保留原有读写超时，新增延迟请求回归。
- 平台测试：macOS 使用 Cmd、Application Support，且既定设计关闭 Alt 菜单助记；
  测试改为验证对应平台行为，菜单点击/关闭仍在 macOS 执行。
- 字体测试：Hiragino 与 Noto 的基线偏差方向不同；UV 矩形还包含透明留白。
  基线比较取偏差绝对值，裁切检查读取 atlas 的非透明覆盖，标题间距采用同字体
  增加 spacer 前后的配对测量。没有放宽裁切阈值或跳过 macOS 字体验证。
- CI：既有 macOS 构建 job 增加原生全工作区测试；仍只在 main 上触发。

## 本地构建

要求：macOS 14+、Xcode Command Line Tools、rustup（仓库固定 Rust 1.98.0）、
Python 3（用于读取 Cargo metadata）。构建脚本可从任意工作目录调用。

```bash
bash packaging/macos/build.sh --open
bash packaging/macos/build.sh --release --universal --dmg
```

- 默认构建当前 Rust host 的 debug 二进制，并生成 `.app`。
- `--release` 生成优化构建；`--universal` 安装两个 Apple target 的标准库，
  编译 arm64 和 x86_64 后用 lipo 合并并校验。
- `--dmg` 额外生成并校验磁盘映像；`--open` 通过 Launch Services 打开 `.app`。
- 产物目录为 `<Cargo target>/macos/<debug|release>/<host|universal2>/`，
  支持 Cargo 配置和 `CARGO_TARGET_DIR`。重复构建会替换该目录中本项目的开发包。
- 部署基线固定为 `MACOSX_DEPLOYMENT_TARGET=14.0`。`.app` 使用发布模板和仓库
  已有的 `AppIcon.iconset`，验证后才替换旧包。签名失败直接报错；ad-hoc 签名
  不等于开发者证书签名或公证，下载分发的 Gatekeeper 行为仍遵循 README。

本地与 `.github/workflows/macos-dmg.yml` 都调用 `bundle.sh`，CI 下载已有的
双架构发布二进制后继续组包，不在打包阶段重新编译。

## 验证记录

- 主机：Apple Silicon，macOS 27.0.1；Xcode Command Line Tools 已存在。
- Rust：`rustc 1.98.0 (88d9e12ae 2026-08-18)`。
- 六项门禁全部通过：`cargo fmt --all --check`，三轮
  `cargo clippy --workspace --all-targets`（default / `--no-default-features` /
  `--all-features`，均 `-- -D warnings`），`cargo test --workspace --all-features`，
  `cargo doc --no-deps --all-features`。
- 本机测试：1,417 passed，0 failed，3 ignored（沿用已有 ignore）。
  验证时目标为 `aarch64-apple-darwin`，部署基线 14.0。
- `build.sh --dmg` 与 `build.sh --release --universal --dmg` 均成功。
  universal2 二进制包含 `x86_64 arm64`；plist、ICNS、严格签名和 DMG 校验通过。
- 应用包经 Launch Services 启动，已观察到 900×600 的 LaterMD 窗口与存活进程；
  启动 stderr 无报错。最终 release 窗口截图已保存，中文正文、标题、代码块与表格
  可见，未出现方块字；这不代替 IME 和全部交互项验收。
- 打包逻辑独立冒烟：用 `/usr/bin/true` 作为临时输入，生成包含空格路径下的
  `.app`；图标转换、版本 `0.0.4` 写入、plist 校验和签名验证通过。
  该检查只验证组包逻辑，不代表 LaterMD 已编译或 GUI 已启动。
- 输出位于 `target/macos/release/universal2/`；本机详细门禁日志与 PR 草稿留在
  `target/macos/validation/`（构建产物，不提交）。

未验证：macOS 14 和 Intel 实体机运行、简体拼音候选框跟随/吞字/焦点、
Gatekeeper 下载路径、Homebrew 安装和一小时写作。双架构编译成功不等于
Intel 真机已运行；本轮未触发 GitHub Release、修改版本号或提交 vendor 改动。
