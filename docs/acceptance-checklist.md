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
| 0.2 | `release.yml` 触发 | tag 形如 `v0.0.x`（正则 `**[0-9]+.[0-9]+.[0-9]+*`；实际 tag v0.0.1–v0.0.3，`v0.1.0` 从未存在，V1 L1/E9）；由 `dist generate` 维护，**禁止手改** | ✅ 已核对 |
| 0.3 | `macos-dmg.yml` | 监听 `release: published`，产出 `latermd-v{version}-universal2-apple-darwin.dmg` | ✅ 已核对 |
| 0.4 | cask 模板 `packaging/latermd.rb` | version 与 URL 模板两处版本号一致，`depends_on macos: :sonoma` | ✅ 已核对（sha256 待 §6 填） |
| 0.5 | 六项门禁本地全绿 | fmt / 三轮 clippy / test / doc；vendor 改动加跑 `vendor/egui_markdown/check.sh` | ✅ 每提交必跑 |
| 0.6 | main 分支 CI 绿 | 合入 PR 后 `rust.yml` 六项 + 三平台 build | ✅ 结构化核验通过(2026-09-30 V3 复验:`name=Rust, conclusion=success @ fd40701`,命令与输出见 [p0-acceptance-status-2026-10.md](p0-acceptance-status-2026-10.md) §8.4 V3-E5;V1 首验同结论,E6) |

---

## 1. 发布触发(已成历史记录:v0.0.1–v0.0.3 三版均自动发版)

> 常规路径全自动(合入版本 bump PR → auto-tag 自动打 tag → release.yml → dmg/deb
> dispatch,见 distribution.md §1),无需手打 tag——三版 tag 均为 annotated、tagger
> `github-actions[bot]`(V1 核对表 R9/R10,证据 E9/E10)。下方手打命令仅是自动链路
> 故障时的**应急通道**(distribution.md §3.2),版本号按实际待发版本写(`v0.1.0`
> 从未存在,证据 E9):

```bash
git checkout main && git pull
git tag vX.Y.Z && git push origin vX.Y.Z   # 应急通道;X.Y.Z 按实际版本
```

三版观察点回填(2026-09-30 按资产名单实证,V1 核对表 R10,证据 E1–E4/E7):

| # | 观察点 | 判据 | 三版结果 |
|---|---|---|---|
| 1.1 | `release.yml` plan 阶段 | 五目标全部出现在 manifest,无 `notCovered` | ✅ 五目标资产三版齐备(A2) |
| 1.2 | 五个 build job | 全部成功;Linux 产物是 `latermd-x86_64-unknown-linux-gnu.tar.xz` | ✅ 三版均在(A2) |
| 1.3 | Release 创建 | 非 draft、非 prerelease(dmg job 依赖 `published`) | ✅ 三版均非(A1/E1) |
| 1.4 | `macos-dmg.yml` | lipo 合一 → `.app` → dmg → 回传同一 Release;**step summary 里有 sha256** | ✅ 三版 dmg 均回传(A4/E2–E4);sha256 已不经手填——tap auto-bump 直读 Release asset digest(§7.1,E12/E13) |

**失败处置**(应急通道语境,V1 核对表 R11:原示例 `v0.1.0` 从未存在,改为按实际 tag 口径):`release.yml` 失败 → 删 tag 重来(`git tag -d <tag> && git push --delete origin <tag>`)后修完再打;dmg job 失败 → 修完在 Release 页手动重跑该 workflow 即可,不必重打 tag。

---

## 2. Windows 11

> 整节需 Win11 实体机,自动侧无替代——逐行 ☐ blocked_external(缺什么/谁能补见 [p0-acceptance-status-2026-10.md](p0-acceptance-status-2026-10.md) §8.3)。

| # | 项 | 操作 | 判据 |
|---|---|---|---|
| 2.1 | 装 | 下 `latermd-x86_64-pc-windows-msvc.zip` 解压 | 双击启动，SmartScreen 警告 →「更多信息 → 仍要运行」（无签名是既定路线，不算缺陷） ☐ blocked_external |
| 2.2 | 渲染后端 | 设置 → 外观 看「渲染后端」 | 显示 `wgpu`；M0 验证 3 要求确认 **DX12 adapter 上报合理**（不是回落软件渲染） ☐ blocked_external |
| 2.3 | 中文字体 | 打开含中文的 md | 无方块；`fonts.rs` 的 Windows 候选（`msyh.ttc` / `simhei.ttf`）命中其一 ☐ blocked_external |
| 2.4 | **IME** | 微软拼音连续输入中文 | ①候选框**跟随光标**；②不吞字；③切走窗口再回来不抢焦点。**任一条不过 = M0 头号风险命中，停下来记档** ☐ blocked_external |
| 2.5 | MCP | 设置 → MCP 勾启用 → 保存 | 状态行显示 `监听 127.0.0.1:8731`；浏览器打开该地址不是必须，用客户端连一次即可 ☐ blocked_external |

---

## 3. macOS 14（Sonoma）

> 整节需 macOS 14 实体机,自动侧无替代——逐行 ☐ blocked_external(挂账明细同 [p0-acceptance-status-2026-10.md](p0-acceptance-status-2026-10.md) §8.3)。

| # | 项 | 操作 | 判据 |
|---|---|---|---|
| 3.1 | 装 | **必须走 `.app`**（dmg 拖入 `/Applications` 或 `brew install --cask crazykun/ailater/latermd`） | 裸二进制跑命令行会**丢输入法上下文**，IME 结论不成立 —— 这条是 2.4 的前置，不是可选项 ☐ blocked_external |
| 3.2 | Gatekeeper | 首次打开 | 无签名 → 报「已损坏/无法验证」；brew 装的由 postflight 去 quarantine；直下 dmg 的手动 `xattr -dr com.apple.quarantine /Applications/LaterMD.app` ☐ blocked_external |
| 3.3 | 渲染后端 | 设置 → 外观 | `wgpu` + **Metal adapter** 上报合理 ☐ blocked_external |
| 3.4 | **IME** | 简体拼音连续输入 | 同 2.4 三条判据；macOS 上 IME 是最容易挂的一项 ☐ blocked_external |
| 3.5 | universal2 | Apple Silicon 与 Intel 各跑一次 | 两架构都能启动（lipo 合一的验证） ☐ blocked_external |

---

## 4. Linux（本机 Deepin 已跑，换发行版复验）

| # | 项 | 判据 |
|---|---|---|
| 4.1 | tar.xz 解压即跑 | 31 MB 二进制 + LICENSE + README，无 panic |
| 4.2 | glibc 要求 | ≥ 2.35（Ubuntu 22.04+ / Debian 12+）；**更老的发行版跑不起来是已知约束**，不是 bug |
| 4.3 | IME | fcitx5 下输入可用但**候选框不跟随**（m0-report 验证 1 已记档，非放行线失败） |

---

## 5. 稳定性长跑（P0 验收 2 的人工部分）

> 人工连续写作时段不可自动替代——自动侧只做过 **15 秒有界冒烟**(Linux/X11 debug build,无 panic,证据 [p0-acceptance-status-2026-10.md](p0-acceptance-status-2026-10.md) §8.4 V3-E4),不冒充本节长跑;逐行 ☐ blocked_external。

| # | 项 | 判据 |
|---|---|---|
| 5.1 | 连续写 1 小时技术文档 | 不崩、不卡；内存无明显增长 ☐ blocked_external |
| 5.2 | 期间切换明暗 / 紧凑密度 | 外壳与正文**同帧**换肤无闪变 ☐ blocked_external |
| 5.3 | 期间开关 MCP、切 Live Preview | 切模式不丢光标、不丢 undo ☐ blocked_external |

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

## 7. 发布后收尾(2026-09-30 重定性:常规已自动化,真机项留人工;V1 核对表 L3/R15,证据 E12/E13)

| # | 项 | 原判据 → 现状 |
|---|---|---|
| 7.1 | cask sha256 | ~~把 §1.4 的 sha256 填进 `packaging/latermd.rb`(替换 `:no_check`)~~ 已由 tap 仓 auto-bump 自动维护(直读 Release asset digest,每小时 :23),不经 step summary → 主仓模板手填 |
| 7.2 | 推 tap | ~~该文件进 `crazykun/homebrew-ailater` 的 `Casks/`~~ 已由 tap 侧完成(cask 已建,version "0.0.3" 自动跟版,E12);**`brew install --cask` 真机实测仍 ☐ blocked_external**(缺 macOS 真机) |
| 7.3 | README 更新 | 「当前还没发过版」那段已于 2026-09-26 按发布状态重写(见 git 历史);**未完**:README:8 版本行仍写 v0.0.1(V1 R1)、安装表无 deb 渠道(V1 L2)——因本棒路径约束登记 decisions-pending #54 留后续单独 PR |
| 7.4 | 回填证据 | 本文件勾选结果回填 [p0-acceptance.md](p0-acceptance.md) §1 与 [m0-report.md](m0-report.md) 验证 1/3;**IME 结论无论好坏都要写进去**——真机项留人工(blocked_external)。**自动侧回填已做**(2026-09-30 V3:p0-acceptance §1 验收 1/3/4 已引用 [p0-acceptance-status-2026-10.md](p0-acceptance-status-2026-10.md) §8 证据;§0.6 已勾 V3-E5;m0-report 验证 1/3 的真机结论段仍空缺,IME 真机未做) |

---

## 8. 无边框外壳三项（2026-09-27 外壳重构 M1–M5，D1 自绘无边框）

> 规格：[ui-shell-redesign.md](ui-shell-redesign.md) §3（标题栏）与 §3.3 R1（resize 风险）。
> 标题栏实际是**七钮**（┃左 / ┃右 / ⦿禅定 / ⚙设置 / ─ / ⤢ / ✕）——M1 规格为六钮，
> 齿轮是 2026-09-27 decisions-pending #31 增设（设置入口挪标题栏），以代码
> `TITLE_BUTTONS`（`ui/titlebar.rs`）为准。

| # | 项 | 操作（验证方法） | 判据 |
|---|---|---|---|
| 8.1 | 自绘标题栏按钮 | 依次点击标题栏右端七钮；hover 每一钮；按住标题区空白拖动；双击标题区空白 | ①各钮触发对应动作：┃左/┃右翻转对应面板、⦿ 进出禅定（F11 同款）、⚙ 打开设置默认页（右键直达四页）、─ 最小化、⤢ 最大化↔还原（图标随 `viewport().maximized` 切换）、✕ 关窗；②hover 有底色（✕ 取 `DANGER` 红）；③拖动可移动窗口、双击最大化/还原 |
| 8.2 | 边缘 resize 八方向 | 光标依次置于窗口**四边中部**（让开四角约 12px）与**四角**，各拖动一次 | 光标变成对应方向 resize 箭头（N/S/E/W/NE/NW/SE/SW 八种）；每个方向上窗口尺寸随拖动变化，四角为双向缩放 |
| 8.3 | `LATERMD_NATIVE_DECORATIONS=1` 逃生口 | 该环境变量启动一次 | 窗口回到**系统原生装饰**：出现系统标题栏，自绘 36px 条与七钮不再渲染，拖动/resize/最小化/关闭全部交还系统；不设该变量时行为不变（自绘无边框） |

**当前状态（2026-09-27）**：

- 8.1 / 8.2 自动化已兜的部分：`cargo test -p latermd-app titlebar` 9 项全绿——七钮排布与齿轮位次、交互命令（StartDrag / 双击 Maximized / Minimized / Maximized / Close / 左右栏与禅定消息）、八向命中区几何与方向、八向 `BeginResize` 命令、命中区与标题栏共存（跨层命中回归）。像素层：明暗两套像素采样已确认七钮群坐标逐组一致（[m5-acceptance.md](m5-acceptance.md) §1④/§3④）。
- 8.2 真机拖拽手感（三平台）：本机 xdotool 合成输入不可信（m5-acceptance §0 已记档），**留人工**；Win11 / macOS 圆角与阴影观感一并在此项里看（R1 的 mac `.with_has_shadow` / Win DWM 圆角）。
- 8.3 **本机 Linux/X11 已实测通过**（2026-09-27）：`xprop _NET_FRAME_EXTENTS` 在逃生口模式下为 `0, 0, 24, 0`（WM 顶部装 24px 装饰条），默认无边框模式该属性不存在（WM 不装装饰）；截图对照：逃生口下自绘 36px 标题栏与右端七钮消失、菜单栏文字上移 36px、client 区上方多出 WM 装饰条。Win/mac 同法可验（看窗口是否有系统标题栏即可）。

---

## 9. 源码查找浮层（2026-09-29）

- 源码模式打开 Ctrl/Cmd+F，在查找框输入有多处命中的词。连续按 Enter 应逐项前进并在末项后环绕，Shift+Enter 应反向跳转；每次跳转后仍可继续输入查询，不需要重新点查找框。
- 无结果时按 Enter / Shift+Enter 不修改源码；点击回源码后，Enter 仍是正常换行，不被查找浮层截走。
- 查找框应位于格式工具条下方。缩窄窗口使工具条换行后仍不重叠；开关查找框不推动源码正文的起始位置。

自动化覆盖：`ui::layout::tests::find_` 四项完整 `reduce → draw` 回归，含 1200 / 1600 宽度的定位断言。Win/mac/Linux 真机键盘、IME 和视觉抽查仍按上述判据执行，不以无头测试代替。

---

## 判据速查：什么算「挂」

- **IME 任一条不过**（吞字 / 候选框不跟随 / 抢焦点）→ 头号风险命中，按 ADR-001 §2.5 的备选方案讨论，**不要硬修 egui**。
- **渲染后端回落软件渲染** → 记 adapter 信息与 `LATERMD_RENDERER=glow` 逃生口实测结果。
- **装包后启动崩溃** → 取终端输出与 `RUST_BACKTRACE=1`，回填 m0-report。
