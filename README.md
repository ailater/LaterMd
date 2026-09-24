# LaterMD

**跨平台、版本化、可对话、可演化的 Markdown 知识工作台** —— Markdown 是内容层,AI 是智能层,Git 是时间层。

> **当前状态:规划阶段(P0 之前)。** 全部技术决策见 [docs/](docs/README.md),演进路线与验收标准见 [docs/roadmap.md](docs/roadmap.md)。尚无可运行代码。

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

- **Windows**:从 [Releases](https://github.com/ailater/LaterMd/releases) 直下 `.exe`。首次运行 SmartScreen 会警告,点「更多信息 → 仍要运行」即可。
- **Linux**:`.AppImage` / `.deb` / `.rpm` 从 Releases 直下,或使用 Linuxbrew。

> 未发布前此节为渠道预告,当前无可安装产物。

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

## 开发

要求 rustc **1.98.0**(根目录 `rust-toolchain.toml` 已钉死,egui 0.36.2 要求 rustc ≥ 1.95)。

```bash
cargo build --release
```

规范(所有贡献者与 Agent 必读):[AGENTS.md](AGENTS.md)。

## License

[MIT](LICENSE)
