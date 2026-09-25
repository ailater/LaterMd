# ADR-004: 技术栈与依赖版本清单

日期: 2026-09-24
状态: 已接受（2026-09-24 重构，见文末修订记录）
关联: [[adr-001-gui-and-architecture]]、[[adr-002-platform-renderer-wysiwyg]]、[[adr-003-renderer-and-ecosystem-audit]]

> 本 ADR 只记录**技术栈选型、依赖版本与打包方案**。
> 功能范围与排期不在本文 —— 唯一事实来源是 [roadmap.md](roadmap.md)。

---

## 1. 项目定位

**跨平台、版本化、可对话、可演化的 Markdown 知识工作台。**

- **Markdown** = 内容层（pulldown-cmark，单一解析器）
- **AI** = 智能层（token/AST 层操作，见 ADR-003 §3/§6）
- **Git** = 时间层（只读集成，P2）
- **纯 Rust GUI** = 跨平台与性能保障（egui + eframe）

三条贯穿全局的约束（单一解析器 / 业务逻辑不依赖 UI 框架 / AST 是 AI 基础）见 [README.md](README.md)，此处不重复。

---

## 2. 技术栈总览（已按实测修正）

| 层面 | 方案 | 版本 | 说明 |
| :-- | :-- | :-- | :-- |
| **GUI** | **egui + eframe** | 0.36.2 (2026-09-08) | 五个子 crate 同步发版，无版本错配 |
| **渲染层** | **vendored egui_markdown (membrane-io)** | HEAD + 升级到 0.36.2 | 4001 行，`forbid(unsafe_code)`，MIT OR Apache-2.0，带集成测试与 criterion bench |
| **Markdown 解析** | **pulldown-cmark** | 0.13.4 | 从 comrak 改过来，理由见 ADR-003 §4 |
| **序列化回 MD** | `pulldown-cmark-to-cmark` | 22.0.1 | Live Preview 与 AI 回写的刚需 |
| **语法高亮** | `syntect` | 5.3.0 | vendored egui_markdown 已集成 |
| **文本缓冲** | `ropey` | 1.6.1 | 编辑器缓冲 |
| **Git** | `git2` | 0.21.0 | vendored-libgit2 + vendored-openssl 简化构建 |
| **文件对话框** | `rfd` | 0.17.2 | Win32 / Cocoa / GTK+XDG Portal |
| **文件监听** | `notify` | 8.2.0 | inotify / FSEvents / ReadDirectoryChangesW |
| **异步运行时** | `tokio` | 1.53.1 | — |
| **序列化** | `serde` | 1（lock 1.0.229） | Theme 持久化（settings.json）等状态落盘的序列化 derive；P0 主题模块引入 |
| **JSON 序列化** | `serde_json` | 1（lock 1.0.151） | Theme 持久化落盘 settings.json（文件树 file_tree.json 同用）；P0 主题模块引入 |
| **CLI** | `clap` | 4.6.7 | — |
| **目录遍历** | `ignore` | 0.4.33 | 文件树与搜索共用（ADR-005 §4） |
| **搜索·行迭代** | `grep-searcher` | 0.1.17 | P1 侧边栏搜索：只用其 `LineIter`（ripgrep 同源行语义）；`Searcher` 系 API 需 `Matcher` 实参而 `regex` 未实现，亦不为此引入 `grep-regex`（decisions-pending #7） |
| **搜索·正则** | `regex` | 1.13.1（bytes 变体） | P1 侧边栏搜索的匹配引擎；大小写开关经 `RegexBuilder::case_insensitive` |
| **URL 编解码** | `percent-encoding` | 2.3.2 | P1 `ai://` 链接协议：prompt 查询参数的严格 %XX 解码（坏序列自行校验，`+` 不当空格）；decisions-pending #11 |
| **LLM·HTTP 客户端** | `ureq` | 3.4.2（default-features 关闭，仅 rustls） | P1 `latermd-ai`：阻塞 HTTP 读 SSE 流；零 async 依赖。选型取舍见修订记录 2026-09-25 |
| **LLM 接入** | OpenAI / Anthropic / Ollama | — | HTTP + SSE 流式；经 `latermd-ai` 以 std 线程 + mpsc 阻塞流实现，不经 tokio |
| **打包编排** | **`axodotdev/cargo-dist`** | **v0.33.0** | ❌ 修正：不是已归档的 `astral-sh/cargo-dist` |
| **打包（备选）** | `crabnebula-dev/cargo-packager` | 0.11.8 | ❌ 修正：不是 `tauri-apps/`（404） |
| **macOS 打包** | `cargo-bundle` | 0.12.0 | 2026-09-20 仍活跃 |
| **Linux AppImage** | `cargo-appimage` | 2.4.0 | — |
| **Linux deb** | `cargo-deb` | 3.8.0 | — |
| **Linux rpm** | `cargo-generate-rpm` | 0.21.0 | — |

### 已否决

| 方案 | 否决理由 |
|---|---|
| **Tauri** | 用户已选纯 Rust。补充：Linux 需 WebKitGTK 4.1，Ubuntu 24.04 / Debian 13 已移除 4.0 包 |
| **GPUI** | crates.io 版本 0.2.2 停更近一年；实际需 git 依赖 Zed 仓库跟随季度级破坏性重构 |
| **tektite** | 146 下载 / 0 star / 单人业余维护。**仅保留作为设计参考** |
| **True WYSIWYG** | 列入 non-goals，理由见 ADR-002 §4.7 |

---

## 3. 跨平台专项

（扩充自 ADR-002 §5，增加打包维度）

| 类别 | Windows 11 | macOS 14 | Linux |
|---|---|---|---|
| 窗口后端 | Win32 | Cocoa | X11 / Wayland |
| 渲染后端 | DX12 / Vulkan | Metal | Vulkan |
| 快捷键 | `Ctrl` | `Cmd` | `Ctrl` |
| 换行符 | CRLF | LF | LF |
| 字体 | 需加载中文字体 | 系统字体较好 | 需确保中文字体可用 |
| 输入法 | IME | IME | IME |
| 文件监听 | ReadDirectoryChangesW | FSEvents | inotify |
| Git 凭据 | Credential Manager | Keychain | libsecret / gnome-keyring |
| 打包 | `.msi` / `.exe` | `.app`（universal2 dmg） | `.AppImage` / `.deb` / `.rpm` |

### 分发策略（2026-09-24 修订：无签名证书路线）

**不购买 Apple Developer 账号与代码签名证书。** 主渠道 GitHub Release，macOS 经自有 Homebrew tap [`crazykun/homebrew-ailater`](https://github.com/crazykun/homebrew-ailater) 分发：

- **cask 复刻 tap 内 `lscreen` 的成熟模式**：universal2 单 dmg（双架构 lipo 合一）、`postflight` 执行 `xattr -dr com.apple.quarantine` 消除 Gatekeeper「已损坏」拦截、`livecheck :github_latest` 自动发现新版本。
- tap 已有每小时 auto-bump 流水线（`.github/workflows/auto-bump.yml`），接入 LaterMD 只需新增一项。
- cargo-dist 只负责构建三平台 Release 产物与 CI；tap 更新走自有流水线，不走 cargo-dist 的 homebrew installer（它生成 formula 装 `bin/`，不适合 GUI app 的 cask 形态）。
- Linux 的 AppImage / deb / rpm 照常产出，供非 brew 用户直下。
- Windows 直下 `.exe` 触发 SmartScreen，README 写明「更多信息 → 仍要运行」，不视为缺陷。

> 先前的「macOS 必须 codesign + notarytool、建议购买 Windows 代码签名证书」结论**作废**——那是签名路线的前提，成本（$99/年 + 证书费 + 公证调试）对本项目无必要，brew 路线已由 lscreen 验证。

CI 矩阵结构见 [roadmap.md](roadmap.md) 持续项一节（落地以 `cargo dist init` 生成物为准）。

---

## 4. 工具链：必须钉 1.98.0

**实测失败**：

```
error: rustc 1.94.0 is not supported by the following packages:
  egui@0.36.2 requires rustc 1.95
  ecolor@0.36.2 requires rustc 1.95
  emath@0.36.2 requires rustc 1.95
  epaint@0.36.2 requires rustc 1.95
```

本机默认 toolchain 为 1.94.0。**必须加 `rust-toolchain.toml`**：

```toml
[toolchain]
channel = "1.98.0"
```

本机已装该版本，实测可编译通过。

> 注意：上游 egui_markdown 的 `rust-toolchain.toml` 写的是 `channel = "stable"`，vendor 后需改为钉死版本。

---

## 5. 项目结构（终态）

```
LaterMD/
├── Cargo.toml
├── rust-toolchain.toml              # channel = "1.98.0"
├── vendor/
│   └── egui_markdown/               # vendored + 升到 egui 0.36.2
│       ├── src/
│       ├── egui_markdown_style/     # 子 crate：MarkdownStyle + serde
│       ├── tests/                   # cache / indent / truncate / width
│       ├── check.sh                 # 照抄到 CI
│       └── LICENSE-MIT, LICENSE-APACHE
├── crates/
│   ├── latermd-core/                # 应用状态机、DTO（出现第二消费者时创建）
│   ├── latermd-md/                  # pulldown-cmark 封装 + token span
│   ├── latermd-render/              # token → 绘制指令（不 import egui）
│   ├── latermd-editor/              # ropey + caret/选区 + IME
│   ├── latermd-git/                 # git2 封装（P2）
│   ├── latermd-ai/                  # provider trait、流式（P1）
│   ├── latermd-export/              # HTML（P0）/ PDF / DOCX
│   └── latermd-app/                 # eframe binary，唯一 GUI crate
├── docs/
└── .github/workflows/
```

**此结构是终态，不是开工指令。** 各 crate 的增量创建时机见 [roadmap.md](roadmap.md)「crate 增量创建表」—— 空骨架是过早抽象。

**说明**：此结构相对于初版方案的 `apps/desktop` + `core` + `ui` + `packages/` 做了简化。理由：

- egui 是立即模式，UI 代码天然与状态机耦合，强行拆 `ui` 层会产生大量跨 crate 的 `&mut Ui` 传递
- `packages/markdown-editor` 是过早抽象 —— 第一个可用版本之前不存在第二个消费者

保留的核心原则不变：**业务逻辑不依赖 UI 框架**。

---

## 6. 历史决策摘录（保留结论，细节见出处）

- **PoC 阶段已被 Vendor 适配取代。** 原 PoC 的三个验证项中，「升级改动量」已由实测回答（24 个错误、3-5 工作日，无阻塞风险），故可直接进 M0。
- **打包工具勘误**：cargo-dist 官方仓库是 `axodotdev/cargo-dist`；`astral-sh/cargo-dist` 已归档；`tauri-apps/cargo-packager` 404，实为 `crabnebula-dev/cargo-packager`。

---

## 修订记录

| 日期 | 变更 |
|---|---|
| 2026-09-24 | 初版：技术栈汇总（当时还承担「最终汇总」职责，含功能范围与排期） |
| 2026-09-24 | **重构**：原 §六功能范围、§七演进路线是过期快照（P0 6-8 周、无文件树/大纲/搜索），与 ADR-005 修订后的 roadmap 冲突。按「单一事实来源」原则，范围与排期全部移交 roadmap.md，本文收敛为纯技术栈 ADR。同步删除与 ADR-001/002/003 重复的铁律、heal/LinkHandler、架构约束三节 |
| 2026-09-24 | **分发策略修订**：确认无 Apple Developer 账号与签名证书，分发改为 GitHub Release + Homebrew tap `crazykun/homebrew-ailater`（cask 复刻 lscreen 模式）。原「存在性阻塞」小节的公证/证书结论作废，排期中证书申请动作移除 |
| 2026-09-25 | **依赖表补登**（P0 评审整改）：latermd-app 为主题持久化新增的 serde / serde_json 此前漏登，补两行（P0 批次）。同步订正 latermd-app/Cargo.toml 中「本就在依赖图中（criterion 的传递依赖）」的错误注释——criterion 是 dev-dependency，不进运行时依赖图，真实理由是主题持久化需要 |
| 2026-09-25 | **依赖表补登**：P1 搜索核心（`crates/latermd-app/src/search.rs`）新增 grep-searcher 0.1.17 与 regex 1.13.1，遵循 decisions-pending #4 口径；不引入 `grep-regex` 的取舍见 decisions-pending #7 |
| 2026-09-25 | **依赖表补登**：P1 新建 `crates/latermd-ai`（crate 增量创建表「P1 开工」行授权），新增 `ureq` 3.4.2。选型取舍：reqwest 的 blocking 客户端内部仍自建 tokio runtime，与本轮「流式 = std 线程 + mpsc、不引入 tokio」的既定口径冲突，故选零 async 依赖的 ureq；default-features 关闭去掉 gzip（SSE 不需要），只留 rustls。同轮订正「LLM 接入」行的「异步」表述——实际为阻塞流。serde_json 复用清单既有条目，不另列 |
| 2026-09-25 | **依赖表补登**：P1 `ai://` 链接协议（`crates/latermd-app/src/ai_link.rs`）新增 `percent-encoding` 2.3.2，遵循 decisions-pending #4 口径；零传递依赖、Cargo.lock 原有条目提升为直接依赖。协议语义定稿见 decisions-pending #11 |
