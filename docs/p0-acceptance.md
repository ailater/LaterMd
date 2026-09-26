# P0 验收清单

日期: 2026-09-25
关联: [roadmap.md](roadmap.md) 阶段 2、[m0-report.md](m0-report.md)

> P0 完成的定义:**编辑-预览-导出-打包四件事在三平台跑通**。
> 本文件逐条记录验收标准、当前状态与**证据出处**(测试名 / commit / 实测命令),未验证项如实标注。
> roadmap 的阶段出口条件要求:阶段完成时产出验收证据 —— 本文件即 P0 的载体。

---

## 1. 四条验收标准

| # | 验收标准(roadmap 阶段 2) | 状态 | 证据 / 缺口 |
|---|---|---|---|
| 1 | **三平台可安装** | 🟨 Linux 已本地验证;Win / mac 待 CI 首跑 | **Linux**:`dist build` 出的 `latermd-x86_64-unknown-linux-gnu.tar.xz` 解压即跑(31 MB 二进制 + LICENSE + README,实测运行 6 秒无 panic),动态链 libgcc_s / libm / libc,**要求 glibc ≥ 2.35**(Ubuntu 22.04+ / Debian 12+ 量级)。`dist plan` 五目标齐备。**macOS** dmg job 与 cask 模板待首个 tag 在 CI 验证;**Windows 产物尚未在任何机器上跑过** |
| 2 | **能连续写 1 小时技术文档不崩、不卡** | 🟨 部分 | 性能有实测(下 §3);「连续 1 小时」的稳定性**无自动化验证**,需人工长跑 |
| 3 | **导出的 HTML 可直接交付他人阅读** | ✅ 逻辑已测 | `latermd-export` 9 项测试:完整文档骨架、内嵌最小 CSS、代码块 language class、GFM 表格、任务列表复选框、标题转义、删除线/脚注。**未做**:在真实浏览器里打开导出件的观感确认 |
| 4 | **`.md` 文件保持原样(无格式化篡改)** | ✅ 已测 | `file.rs::write_then_read_is_byte_exact`(CRLF/LF 混排、尾随空行逐字节一致);另测非 UTF-8 报错、原子落盘、权限位保留。**未做**:Windows CRLF 实机往返 |

---

## 2. 模块完成度(范围表 11 项)

| 模块 | 状态 | commit / 备注 |
|---|---|---|
| 三栏布局 | ✅ | `33fa18f` |
| 编辑器(双栏源码 + 实时预览) | ✅ | `a3f760f` |
| 文件新建 / 打开 / 保存 / 另存为 | ✅ | `6710669` |
| Markdown(CommonMark + GFM) | ✅ | vendor 提供 |
| 代码高亮 + 复制 | ✅ | vendor 提供(syntect) |
| 导出 HTML | ✅ | `35f0bf0` + `latermd-export` |
| 主题系统(批次 A:亮/暗 + 持久化) | ✅ | `bf32495` |
| 快捷键(Ctrl / Cmd 自动适配) | ✅ | `b0bbc2c` |
| 文件树(基础版) | ✅ | `37c9e0c` |
| 大纲(廉价版,跳编辑器光标) | ✅ | `8c00cdd` |
| **打包分发** | 🟨 配置就位,待首跑 | dist 五目标 + `macos-dmg.yml` + `packaging/latermd.rb` |

**10 / 11 功能已落地,剩打包。**

---

## 3. 性能证据(支撑验收 2)

| 场景 | 数据 | 出处 |
|---|---|---|
| 10 万字滚动到中部(bench) | 430 µs/帧,与顶部 415 µs 持平 | `benches/longdoc.rs`,[m0-report](m0-report.md) §2.1 |
| 10 万字真窗口滚动(交互 fps) | p50 16.58 ms(**60.3 fps**)、p95 20.29 ms(49.3 fps) | `examples/scrollbench.rs`,[m0-report](m0-report.md) §2.2 |
| 打开 10 万字(冷首帧) | 136 ms(一次性) | 同上 §2.1 |

⚠️ 本机是 **llvmpipe 软件渲染**,上述 fps 是**下限**;真机(DX12 / Metal / 硬件 Vulkan)需复跑 `cargo run --release --example scrollbench`。

---

## 4. 出 P0 前必须补的三件事

> **操作步骤与判据见 [acceptance-checklist.md](acceptance-checklist.md)**（人工真机验收清单，2026-09-26 新增）。本节只列"哪三件事"，执行细节一律以那份清单为准。

1. **首个 tag 跑通发布链路**:合入打包 PR → `git tag v0.1.0 && git push origin v0.1.0` → 确认 release.yml 五目标产物齐备、macos-dmg.yml 合成 dmg 成功。
2. **真机 IME 实测**(M0 遗留,头号风险):Win11 微软拼音 + macOS 14 简体拼音,记录候选框跟随 / 连续输入不吞字 / 窗口切换不抢焦点。macOS 必须走 `.app` 启动(裸二进制丢输入法上下文)。
3. **三平台装一次、跑一次**:Win11 DX12 与 macOS Metal 的 adapter 上报确认(M0 验证 3),以及连续写作的稳定性长跑(验收 2 的人工部分)。

---

## 5. 已知遗留(不阻塞 P0 出口,但要登记)

- **流式追加是 O(n)**(~77 µs/行):超 ~1300 行就跟不上 100 ms/chunk 的 LLM 节奏。**P1 开工前必须定分段重排方案**,详见 [m0-report](m0-report.md) §4.2。
- **Windows / macOS 字体候选**:`fonts.rs` 的候选表需在各平台实测后补(含 `.ttc` face index 重查)。
- **cask 版本自动更新**:tap 的 auto-bump 目前只管 Formula,Casks/ 需人工或扩展流水线。
