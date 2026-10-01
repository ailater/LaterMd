# 自动保存与崩溃恢复:真机 Linux X11 冒烟验收(2026-10-01)

> 验收对象:#18 自动保存核心(44538d3)+ 孤儿 draft 恢复条(50feb06),`feature/autosave-recovery` 分支。
> 方法:`timeout` 包裹 `cargo run` 于 `DISPLAY=:0` 真机启动,xdotool 合成键盘、xclip+Ctrl+V 剪贴板通道合成输入,
> `import -window` 截图 + python3/PIL 像素断言取证;文件级断言用 `ls`/`stat`/`cat`/`od`。
> 环境隔离:`XDG_CONFIG_HOME=/tmp/latermd-smoke/config`(应用配置不碰本机真实 `~/.config/latermd`),
> 测试文档在 `/tmp/latermd-smoke/vault/doc.md`;冒烟后 vault 只剩 `doc.md`,本机用户配置未动。

## 0. 环境备注(影响验收通道的四件事,非应用缺陷)

1. **dde-lock 反复盖屏**:锁屏进程数次被桌面环境自动拉起(`_NET_ACTIVE_WINDOW` 归零、合成输入零到达)。
   按 m5-acceptance §0 先例同用户 `kill` 该进程恢复输入;盖屏复发时再次处理。
2. **fcitx5 拦截裸字符键**:xdotool `type` 合成的裸按键全部被输入法转为 IME 预编辑——**屏幕显示「已输入」但
   `EditorBuffer` 从未变更**(Ctrl+S 保存写盘内容仍为原文,实证)。带 Ctrl 修饰的组合键不受影响。
   因此冒烟的文本输入一律走 **xclip 写 CLIPBOARD + Ctrl+V 粘贴**;粘贴前若焦点不在编辑器,先按一次
   Ctrl+B(格式动作经 `write_selection` 把焦点还给编辑器,`ui/editor.rs:234`)。
3. **X server 客户端连接耗尽(MaxClients)**:一次高频截图扫描触发,当时 app 实例被连坐退出(日志无 panic、
   无错误,纯 X 断连)。清理残留 `import`/`xdotool` 进程后重启实例继续。
4. **任务书与现状的两处出入,以现状为准**:①包名是 `latermd-app` 而非 `latermd`;②「再编辑触发新 draft 后
   点『丢弃』」与 decisions-pending #65「直接编辑即撤条」口径冲突——再编辑后恢复条已撤、按钮不在场,
   故丢弃验证按「二次 kill -9 重启 → 恢复条再现 → 点『丢弃』」兑现(见 §2.3)。

## 1. 冒烟 a:停顿 30s 落 draft,内容与缓冲一致 —— PASS

启动命令(实际执行,`timeout` 包裹;cwd=vault 使 Ctrl+O 对话框起始目录即测试目录,`file.rs:44` `start_dir` 回落进程 cwd):

```bash
cd /tmp/latermd-smoke/vault && timeout 2400 env -u XMODIFIERS RUSTUP_TOOLCHAIN=1.98.0 \
  DISPLAY=:0 XDG_CONFIG_HOME=/tmp/latermd-smoke/config RUST_BACKTRACE=1 \
  cargo run --manifest-path /home/data/www/LaterMD/Cargo.toml -p latermd-app
```

流程:Ctrl+O → Down → Return 打开 `doc.md`(窗口标题变 `LaterMD — doc.md`)→ Ctrl+B 聚焦 → Ctrl+End →
xclip 写入 `smoke-v1-20261001 粘贴通道追加` + Ctrl+V 粘贴(编辑器与**预览列同步变化**,像素 diff
editor 3092 / preview 2333,证明缓冲真实变更而非 IME 假象)→ 静置等待。

| 断言 | 证据 | 结论 |
|---|---|---|
| 停顿阈值后 draft 出现 | 轮询 2s 一次,iter=1 即命中(`AUTOSAVE_IDLE`=30s,`state.rs:77`);`ls` 见 `doc.md.latermd-draft`,78 字节 | PASS |
| 内容与缓冲一致 | `cat` 全文 `**smoke-v1-20261001 粘贴通道追加**# 自动保存冒烟\n\n原始内容。\n`,44 字符;状态栏同帧显示「44 字」(`layout.rs:838` 字数取自 `editor.text()`) | PASS |
| draft 与原文件同目录、命名追加式 | `/tmp/latermd-smoke/vault/doc.md.latermd-draft`(`DRAFT_SUFFIX` 追加完整文件名之后,`state.rs:83`) | PASS |
| 原文件未被篡改 | `doc.md` mtime/内容全程不变(38 字节原文) | PASS |

## 2. 冒烟 b:kill -9 → 恢复条 → 恢复 → 再编辑 → 丢弃 —— PASS

### 2.1 kill -9 后重启,恢复条出现

`kill -9 <pid>` 后 draft 保留在盘上;重启(同 §1 命令)后 Ctrl+O 打开同一 `doc.md`:

- 像素断言:编辑器顶部 WARN 黄(`#EBB43C`,`tokens.rs:111`)620/625/616 px,label 带 y[101,114] ——
  「发现未保存草稿(保存于 N 分钟前)」(`ui/layout.rs:921` `recovery_bar`,行内条把编辑器下推一行)。
- 按钮定位:hover 网格扫描(13px×15px 步进),两枚按钮命中区稳定在 **恢复 x[248,289]、丢弃 x[296,337],
  y[123,150]**(egui button hover 变色矩形,同一按钮多扫描点命中矩形一致)。

### 2.2 点「恢复」→ 缓冲含草稿内容,可撤销

点击恢复按钮中心(268,136)后:

| 断言 | 证据 | 结论 |
|---|---|---|
| draft 被消费 | `ls`:`doc.md.latermd-draft` 消失(`recover_draft` 读入即删,`state.rs:1944`) | PASS |
| 缓冲含草稿内容 | 状态栏「44 字」== 草稿字符数;编辑器可见 `**smoke-v1…粘贴通道追加**…`;提示行「已恢复未保存草稿(Ctrl+Z 可撤销)」 | PASS |
| 恢复可撤销 | Ctrl+Z 后状态栏变「16 字」== 盘上原文字符数(整篇替换打碎为一步 undo,`EditorBuffer::replace_all` 路径) | PASS |

### 2.3 再编辑触发新 draft,点「丢弃」→ draft 消失

Ctrl+B 聚焦 + Ctrl+V 再粘贴 → 编辑器持焦点(caret 闪烁驱动重绘)→ **约 2s 内**(即 30s 到点即写)新
`doc.md.latermd-draft` 落盘。此后按 §0.4 口径二次 `kill -9` → 重启 → 打开 → 恢复条再现(黄 616px)→
点击丢弃按钮中心(316,136):

| 断言 | 证据 | 结论 |
|---|---|---|
| draft 消失 | `ls`:vault 只剩 `doc.md`(`discard_draft` 删盘上文件,`state.rs:1980`) | PASS |
| 缓冲不动 | 丢弃后状态栏仍「16 字」(盘上版本,分毫未动) | PASS |
| 留痕可追溯 | 提示行「已丢弃未保存草稿(/tmp/latermd-smoke/vault/doc.md.latermd-draft)」 | PASS |
| 直接编辑即撤条(#65)顺带实证 | 误触格式条 B(缓冲置脏)后,恢复条当帧消失(黄像素 620→0)且 draft 文件保留 | PASS |

## 3. 冒烟 c:全程无 panic —— PASS

六份启动日志(`run-a3/a5/b1/b2/b3/b4.log`,均带 `RUST_BACKTRACE=1`)全文检索:

```text
$ grep -c "panick" /tmp/latermd-smoke/run-*.log   →  全部 0
```

涵盖:两轮 kill -9 硬杀、一次 X 断连连坐、CJK 内容全程、恢复/丢弃归约、F11 禅定往返——无一处 panic 栈。

## 4. 三处导航过滤汇总核对(引用回归测试)—— PASS

`*.latermd-draft` 不入文件树 / 全文搜索 / git 状态面板,口径由 #18 核心模块的回归测试钉住(本轮复跑复核各 1 passed):

| 过滤点 | 回归测试 | 断言 | 本轮实跑 |
|---|---|---|---|
| 文件树 | `filetree.rs:414` `list_children_excludes_draft_files` | `x.md.latermd-draft` 扩展名不在 Markdown 清单,`list_children` 只出 `x.md` | `cargo test -p latermd-app list_children_excludes_draft_files` → 1 passed |
| 全文搜索 | `latermd-search/src/lib.rs:484` `search_skips_draft_mirror_files` | draft 即使命中关键词也不入搜索结果 | `cargo test -p latermd-search search_skips_draft_mirror_files` → 1 passed |
| git 状态面板 | `git_panel.rs:298` `refresh_hides_draft_files_from_changes_and_badges` | `?? a.md.latermd-draft` 不入改动列表、文件树无角标 | `cargo test -p latermd-app refresh_hides_draft_files` → 1 passed |

另:`cargo test -p latermd-app autosave`(7 passed)与 `cargo test -p latermd-app recovery`(8 passed)本轮实跑
全绿,覆盖停顿/切出落盘、保存/关闭清理、同修订跳过、未命名落状态目录、写失败留旧稿、孤儿检测、恢复/丢弃/坏内容不 panic、直接编辑撤条。

## 5. 发现与遗留(缺陷线索,非本模块修复职责)

- **帧饥饿延迟落盘(实证)**:编辑器**失焦**且无输入事件时,egui 无重绘驱动,30s 停顿到期时刻没有帧可跑
  `autosave_pass`(它在 `end_of_logic` 每帧归约,`state.rs:1848`),draft 推迟到下一帧。真机两例:
  焦点在编辑器(粘贴后)2 秒内落盘;焦点被工具条按钮抢走(格式动作置脏)后 6 分钟 + 多次 motion/F11/
  Ctrl+N 均未落盘。**真实风险**:用户打字中途切去别的窗口,LaterMD 失焦无 caret 闪烁,若恰在下一帧之前
  崩溃/断电,「停顿 30s 落 draft」承诺空转。建议后续在 `last_edit` 刷新处补
  `ctx.request_repaint_after(AUTOSAVE_IDLE)` 类兜底(属 #18 核心模块,登记待修,不在本验收模块改)。
- **合成输入的通道依赖**:本机 fcitx5 环境下裸键合成必被 IME 吞(§0.2),后续所有 GUI 冒烟沿用
  「Ctrl+B 聚焦 + 剪贴板粘贴」通道;真实用户路径(实体键盘 + IME 上屏)不受影响,留人工。

## 6. 人工验收清单(blocked_external / 目视项,不得以自动断言冒充)

| # | 验收项 | 缺什么 / 为什么留人工 |
|---|---|---|
| 1 | 恢复条交互目视:真实鼠标点击「恢复/丢弃」的手感、按钮 hover 反馈、条下推编辑器一行的观感、相对时间文案随时间推移的刷新 | 本机已用合成输入验证到「像素断言 + 归约结果」,真实手感与观感需人手目视 |
| 2 | 未命名文档 draft 行为(落 `config_dir()/drafts/untitled-<标签id>`,重启后**不**弹恢复条——孤儿候选仅随「打开文档」检测) | 单测 `autosave_untitled_draft_goes_to_config_drafts_dir` 已钉落点;真机目视与「未命名孤儿如何找回」的产品口径留人工 |
| 3 | Windows CRLF 下 draft 内容口径(缓冲 CRLF→draft 落盘是否保真、恢复往返不换行错乱) | 缺 Windows 真机;Linux LF 无法复现 CRLF 路径 |
| 4 | macOS 行为(状态目录 `~/Library/Application Support/latermd/drafts/`、APFS 原子 rename、恢复条渲染) | 缺 macOS 真机 |
| 5 | Win/mac IME 并发下的自动保存(输入法组合中停顿计时是否被预编辑刷新) | 缺两平台真机 + 实体 IME |
