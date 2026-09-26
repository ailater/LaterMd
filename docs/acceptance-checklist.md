# 人工验收清单（真机执行）

日期：2026-09-26
关联：[p0-acceptance.md](p0-acceptance.md)（P0 验收标准与证据）、[m0-report.md](m0-report.md)（M0 实测与遗留）、[distribution.md](distribution.md)（发布 runbook）

> 自动化门禁替代不了的部分都在这里：**IME、真机渲染后端、装包启动、发布链路**。
> 判据写死在每一项里 —— 没写判据的项不算验收项。勾完即 P0 出口。
>
> 执行顺序即文档顺序：先静态前置，再打 tag，再逐平台验，最后收尾。

---

## 0. 打 tag 前的静态前置（我这边已查，你复核）

| # | 项 | 判据 | 状态 |
|---|---|---|---|
| 0.1 | `dist-workspace.toml` 五目标矩阵 | Linux x64 / macOS 双架构 / Windows x64 + ARM64，`installers = []`、`merge-tasks = true` | ✅ 已核对 |
| 0.2 | `release.yml` 触发 | tag 形如 `v0.1.0`（正则 `**[0-9]+.[0-9]+.[0-9]+*`）；由 `dist generate` 维护，**禁止手改** | ✅ 已核对 |
| 0.3 | `macos-dmg.yml` | 监听 `release: published`，产出 `latermd-v{version}-universal2-apple-darwin.dmg` | ✅ 已核对 |
| 0.4 | cask 模板 `packaging/latermd.rb` | version 与 URL 模板两处版本号一致，`depends_on macos: :sonoma` | ✅ 已核对（sha256 待 §6 填） |
| 0.5 | 六项门禁本地全绿 | fmt / 三轮 clippy / test / doc；vendor 改动加跑 `vendor/egui_markdown/check.sh` | ✅ 每提交必跑 |
| 0.6 | main 分支 CI 绿 | 合入 PR 后 `rust.yml` 六项 + 三平台 build | ☐ 待你在 GitHub 上看一眼 |

---

## 1. 打 tag 触发发布

```bash
git checkout main && git pull
git tag v0.1.0 && git push origin v0.1.0
```

| # | 观察点 | 判据 |
|---|---|---|
| 1.1 | `release.yml` plan 阶段 | 五目标全部出现在 manifest，无 `notCovered` |
| 1.2 | 五个 build job | 全部成功；Linux 产物是 `latermd-x86_64-unknown-linux-gnu.tar.xz` |
| 1.3 | Release 创建 | 非 draft、非 prerelease（dmg job 依赖 `published`） |
| 1.4 | `macos-dmg.yml` | lipo 合一 → `.app` → dmg → 回传同一 Release；**step summary 里有 sha256**（§6 要用） |

**失败处置**：`release.yml` 失败 → 删 tag 重来（`git tag -d v0.1.0 && git push --delete origin v0.1.0`）后修完再打；dmg job 失败 → 修完在 Release 页手动重跑该 workflow 即可，不必重打 tag。

---

## 2. Windows 11

| # | 项 | 操作 | 判据 |
|---|---|---|---|
| 2.1 | 装 | 下 `latermd-x86_64-pc-windows-msvc.zip` 解压 | 双击启动，SmartScreen 警告 →「更多信息 → 仍要运行」（无签名是既定路线，不算缺陷） |
| 2.2 | 渲染后端 | 设置 → 外观 看「渲染后端」 | 显示 `wgpu`；M0 验证 3 要求确认 **DX12 adapter 上报合理**（不是回落软件渲染） |
| 2.3 | 中文字体 | 打开含中文的 md | 无方块；`fonts.rs` 的 Windows 候选（`msyh.ttc` / `simhei.ttf`）命中其一 |
| 2.4 | **IME** | 微软拼音连续输入中文 | ①候选框**跟随光标**；②不吞字；③切走窗口再回来不抢焦点。**任一条不过 = M0 头号风险命中，停下来记档** |
| 2.5 | MCP | 设置 → MCP 勾启用 → 保存 | 状态行显示 `监听 127.0.0.1:8731`；浏览器打开该地址不是必须，用客户端连一次即可 |

---

## 3. macOS 14（Sonoma）

| # | 项 | 操作 | 判据 |
|---|---|---|---|
| 3.1 | 装 | **必须走 `.app`**（dmg 拖入 `/Applications` 或 `brew install --cask crazykun/ailater/latermd`） | 裸二进制跑命令行会**丢输入法上下文**，IME 结论不成立 —— 这条是 2.4 的前置，不是可选项 |
| 3.2 | Gatekeeper | 首次打开 | 无签名 → 报「已损坏/无法验证」；brew 装的由 postflight 去 quarantine；直下 dmg 的手动 `xattr -dr com.apple.quarantine /Applications/LaterMD.app` |
| 3.3 | 渲染后端 | 设置 → 外观 | `wgpu` + **Metal adapter** 上报合理 |
| 3.4 | **IME** | 简体拼音连续输入 | 同 2.4 三条判据；macOS 上 IME 是最容易挂的一项 |
| 3.5 | universal2 | Apple Silicon 与 Intel 各跑一次 | 两架构都能启动（lipo 合一的验证） |

---

## 4. Linux（本机 Deepin 已跑，换发行版复验）

| # | 项 | 判据 |
|---|---|---|
| 4.1 | tar.xz 解压即跑 | 31 MB 二进制 + LICENSE + README，无 panic |
| 4.2 | glibc 要求 | ≥ 2.35（Ubuntu 22.04+ / Debian 12+）；**更老的发行版跑不起来是已知约束**，不是 bug |
| 4.3 | IME | fcitx5 下输入可用但**候选框不跟随**（m0-report 验证 1 已记档，非放行线失败） |

---

## 5. 稳定性长跑（P0 验收 2 的人工部分）

| # | 项 | 判据 |
|---|---|---|
| 5.1 | 连续写 1 小时技术文档 | 不崩、不卡；内存无明显增长 |
| 5.2 | 期间切换明暗 / 紧凑密度 | 外壳与正文**同帧**换肤无闪变 |
| 5.3 | 期间开关 MCP、切 Live Preview | 切模式不丢光标、不丢 undo |

---

## 6. 本轮新功能的真机抽查（2026-09-26 落地项）

| # | 项 | 判据 |
|---|---|---|
| 6.1 | 皮肤文件 | 设置 → 外观 导出一个皮肤 → `themes/*.ron` 出现并可下拉切换；改一个颜色重启生效 |
| 6.2 | 跟随系统 | 选「跟随系统」→ 切系统主题，最迟 1 秒跟上；读不到时页面明确提示（不静默） |
| 6.3 | Live Preview | `Cmd/Ctrl+/` 切到 Live：光标所在块显示源码、其余富渲染；↑↓ 跨块正常 |
| 6.4 | `[[wikilink]]` | 文档里写 `[[另一篇]]` → 预览变链接（青绿）→ 点击打开同名文档 |
| 6.5 | 大纲预览跳转 | 点大纲条目 → **预览滚到该节顶部**（不只是编辑器跳光标） |
| 6.6 | MCP 被外部 AI 调用 | `claude mcp add latermd -- latermd --mcp-stdio` 或 HTTP 端点，调一次 `search_docs` 拿到命中 |

---

## 7. 发布后收尾

| # | 项 | 判据 |
|---|---|---|
| 7.1 | cask sha256 | 把 §1.4 的 sha256 填进 `packaging/latermd.rb`（替换 `:no_check`） |
| 7.2 | 推 tap | 该文件进 `crazykun/homebrew-ailater` 的 `Casks/`；`brew install --cask` 实测通过 |
| 7.3 | README 更新 | 「当前还没发过版」那段改写为真实版本与下载方式 |
| 7.4 | 回填证据 | 本文件勾选结果回填 [p0-acceptance.md](p0-acceptance.md) §1 与 [m0-report.md](m0-report.md) 验证 1/3；**IME 结论无论好坏都要写进去** |

---

## 判据速查：什么算「挂」

- **IME 任一条不过**（吞字 / 候选框不跟随 / 抢焦点）→ 头号风险命中，按 ADR-001 §2.5 的备选方案讨论，**不要硬修 egui**。
- **渲染后端回落软件渲染** → 记 adapter 信息与 `LATERMD_RENDERER=glow` 逃生口实测结果。
- **装包后启动崩溃** → 取终端输出与 `RUST_BACKTRACE=1`，回填 m0-report。
