# LaterMD

**跨平台、版本化、可对话、可演化的 Markdown 知识工作台** —— Markdown 是内容层,AI 是智能层,Git 是时间层。

> **当前状态:Vendor 适配已完成,M0 技术验证进行中,P0 骨架在建。** 全部技术决策见 [docs/](docs/README.md),演进路线与验收标准见 [docs/roadmap.md](docs/roadmap.md),M0 实测数据见 [docs/m0-report.md](docs/m0-report.md)。
> 仓库已可编译运行(骨架窗口),但**尚无对外发布的安装包** —— 想跑就本地编译,见「构建与运行」。

## 平台支持

| 平台 | 支持 | 图形后端 |
|---|---|---|
| Windows 11+ (x64 / ARM64) | ✅ | DX12 (wgpu) |
| macOS 14+ (Sonoma) | ✅ | Metal (wgpu) |
| Linux (X11 / Wayland) | ✅ | Vulkan (wgpu) |

**显式不支持 Windows 10(含 LTSC)与 macOS 13 及以下** —— 支持它们意味着为 Metal 特性差异和 DX12 降级路径做双份测试,不在本产品范围内。

驱动黑名单逃生口:`LATERMD_RENDERER=glow` 环境变量可切换渲染后端(仅诊断用)。

## 安装(发布后)

本项目**不做代码签名与公证**(个人开源软件,省 $99/年与证书成本),分发渠道:

```bash
# macOS(推荐):Homebrew tap,postflight 自动移除 quarantine,无 Gatekeeper 拦截
brew install --cask crazykun/ailater/latermd
```

- **Windows**:从 [Releases](https://github.com/ailater/LaterMd/releases) 下 `latermd-x86_64-pc-windows-msvc.zip`(ARM64 机器取 `aarch64-pc-windows-msvc`),解压后运行 `latermd.exe`。无签名,SmartScreen 会警告,点「更多信息 → 仍要运行」。
- **Linux**:下 `latermd-x86_64-unknown-linux-gnu.tar.xz`,解包后 `./latermd`,或自行放进 `~/.local/bin`。
- **macOS**:universal2 单 dmg(`latermd-v{版本}-universal2-apple-darwin.dmg`),Intel / Apple Silicon 通用。推荐走 brew(postflight 自动去 quarantine);**从 Releases 直下 dmg** 的用户,拖入 `/Applications/` 后首次打开会被 Gatekeeper 拦(本项目无签名公证,提示「无法验证开发者」或「已损坏,无法打开」),执行一次下面的命令即可正常启动(brew 安装的用户不需要):

  ```bash
  sudo xattr -dr com.apple.quarantine /Applications/LaterMD.app
  ```

> 未发布前此节为渠道预告,当前无可安装产物;发布链路(cargo-dist 五目标 + macOS dmg + cask)已就位,操作手册见 [docs/distribution.md](docs/distribution.md)。

## 技术栈

纯 Rust。核心选型(完整清单与版本见 [docs/adr-004](docs/adr-004-technical-stack.md)):

- **GUI**:egui + eframe 0.36.2(wgpu 后端)
- **Markdown**:pulldown-cmark —— 全项目唯一解析器
- **渲染**:vendored [egui_markdown](https://github.com/membrane-io/egui_markdown)(升级至 egui 0.36.2)
- **版本层**:git2(只读集成)
- **打包**:axodotdev/cargo-dist,三平台 CI 矩阵

## 范围

按优先级:P0 编辑器骨架 → P1 AI 流式写作 → P2 Git 只读集成 → P3 Live Preview 与双向链接。明细见 [roadmap](docs/roadmap.md)。

**明确不做(不是「以后做」,是不属于本产品)**:

- True WYSIWYG(Word / Notion 形态)—— `.md` 文件永远是真相源
- 知识图谱、语义搜索、MOC、多模态嵌入 —— 这是另一个产品
- Git 的 rebase / cherry-pick / LFS / submodule / 多仓库
- 小说助手、闪卡、日记洞察

> 判断标准:如果一项功能不能让「写下一篇技术文档」变得更快,它就不在 P0–P2。

## 构建与运行

> 没有发布产物,跑起来必须本地编译。产物**不能跨平台拷贝** —— Linux 编出来的是 ELF,Windows 上跑不了。

### 前提

| 项 | 要求 |
|---|---|
| Rust | **1.98.0**(根目录 `rust-toolchain.toml` 已钉死,rustup 自动切换;egui 0.36.2 要求 ≥ 1.95) |
| Linux 额外依赖 | `sudo apt-get install -y libxkbcommon-dev`(winit 编译需要) |
| 图形环境 | Linux 需 X11 / Wayland 会话;无显示器时窗口起不来 |

### 命令

```bash
cargo run --release -p latermd-app     # 编译并直接运行
cargo build --release                  # 只编译
./target/release/latermd               # 跑已编译好的产物
```

产物位置:`target/release/latermd`(Linux 约 31 MB;crate 名 `latermd-app` 不变,bin 名 `latermd`,与发布资产同名)。另有 `target/debug/latermd`(约 393 MB,带调试信息、启动慢)。

### Windows / macOS

同样在各自平台上执行 `cargo run --release -p latermd-app`,仓库无平台特定逻辑。Windows 需 MSVC 生成工具(rustup 默认会装),首次编译 10 分钟级。

**已知限制**:字体候选表目前只列了 Linux 的 Noto Sans CJK / 文泉驿,**Windows 上中文会显示为方块**,界面会给出「⚠ 未找到候选 CJK 字体」的警告。P0 打包前补 `msyh.ttc` 候选(PingFang 同理)。

### 渲染后端

默认 wgpu。`LATERMD_RENDERER=glow` 是驱动黑名单的逃生口,**只在启用 glow feature 的构建里生效**:

```bash
cargo run --release -p latermd-app --features glow
```

### 合入 PR 前:本地跑完六项门禁

CI 只在 `main` 上跑(`push: branches: [main]`),所以 PR 合入前必须本地验证:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --no-default-features -- -D warnings
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --no-deps --all-features
```

改动落在 `vendor/egui_markdown/` 时,再跑一遍上游门禁 `vendor/egui_markdown/check.sh`。

## 开发

分支与推送规则、vendor 改动三类拆分、三条铁律等,见 [AGENTS.md](AGENTS.md)(所有贡献者与 Agent 必读)。

## License

[MIT](LICENSE)
