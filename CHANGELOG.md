# Changelog

本文件维护各版本变更。发布链路(cargo-dist)会把对应版本的小节自动注入 GitHub Release 正文;
发版时把 `Cargo.toml` 的 `workspace.package.version` 提到新版本号,并在顶部追加
`## v<版本> - <日期>` 小节(两者同 PR 提交,合入后 auto-tag 自动打 tag 发布)。

> **写小节是发布的一部分,不是可选项。** 本小节正文会被 cargo-dist 原样注入 GitHub Release
> —— 漏写,Release 页面就只剩一张下载表(v0.0.2 首发即为此形态,事后人工补)。
> 下载表本身由 dist 生成,只含五个矩阵目标;`universal2 dmg` 是 macos-dmg job 事后回传的,
> 永远不在这张表里,需在 Release 页面手工补一行(见 [docs/distribution.md](docs/distribution.md) §3.3)。

## v0.0.2 - 2026-09-29

外壳重构收官 + UI 现代化 + 图片链路 + 预览排版修复。相对 v0.0.1 共 54 个 commit。

### 外壳重构(M1–M5)

- 自绘标题栏替代系统标题栏,窗口与三栏重排,布局持久化到 `layout.json`
- 左栏三段式骨架:文件树 / 搜索 / 大纲 / Git 四个可折叠视图共用一栏
- Markdown 格式工具条(标题、加粗、斜体、列表、代码块、链接等常用格式一键插入)
- **禅定模式**(F11):三栏让位、预览占满内容区,退出时逐项还原进前的面板快照
- 底部状态栏:路径 · 行列 · 字数 · 主题 · 渲染后端 · AI · MCP 一行收口
- 设置入口挪到标题栏齿轮;应用图标素材更新到 v1.1

### 视觉与动效

- **Inter 三字重**接入,Proportional 首位 + CJK 回退链保序,SemiBold 上标题栏与格式条
- 设计 token 扩充(输入高度、圆角投影等)**九套预设色板**:Dracula、Nord、Gruvbox Dark、
  Solarized Dark / Light、Tokyo Night、One Dark / One Light、Rosé Pine
- 编辑/预览模式切换 crossfade 淡入;浮层淡入直接吃 egui 内建路径(不自研)
- 两套主题下 11 项像素级断言通过,证据见 [docs/m5-acceptance.md](docs/m5-acceptance.md)

### 图片与图床(床,此前完全缺失)

- 图片框插入:自绘图标 + 对话框 + 本地图片复制进 `<文档名>.assets/`,相对路径预览出图
- 文件树过滤 `*.assets` 附件目录,不干扰文档浏览
- **Ctrl+V 剪贴板图片**与拖入图片文件直接落 `.assets/` 并插入
- 新增 `latermd-bed` 图床 crate:SM.MS / GitHub 两套预设,设置第五页配置,后台上传链路

### 预览排版

- 表格**边框**出厂默认与九套预设全开(1px 线宽)
- 表格**表头 / 斑马底色**,faint token 对齐导出 CSS 口径(深 `#323438` / 浅 `#F6F8FA`)
- 行高改按「字号 × 比例」,替换写死的 17px(vendored 层,属①类可上游合入改动)

### 编辑器

- **源码模式行号槽**:左侧纯绘制列随滚动平移(零同步成本),宽度随总行数位数自适应,
  光标行 accent 高亮,折行行号标逻辑行首 visual row,只绘视口命中行
- 源码栏滚动修复:TextEdit 外层套 ScrollArea,光标变化时滚入视口

### 缺陷修复

- 预览标题 / 加粗中文变方块 —— CJK 回退循环漏挂 bold 别名族
- Windows 发布版隐藏控制台黑窗
- 选中文字不可见 —— `selection.stroke` 兼任选中文字颜色,不能设 NONE
- 任务列表含中文时崩溃 —— 状态栏持有过期字节偏移
- 四栏黑条 / 状态栏横跨 / 标题栏齿轮哑弹

### 已知限制

- Linux 下 fcitx5 输入法候选框暂不跟随光标(输入本身可用,已入队排查)
- macOS 从 Releases 直下 dmg 首次打开会被 Gatekeeper 拦截,执行一次
  `sudo xattr -dr com.apple.quarantine /Applications/LaterMD.app` 即可;推荐 Homebrew 安装,无此问题
- Windows 无代码签名,SmartScreen 会提示,选「更多信息 → 仍要运行」
- Live Preview 内联半隐藏样式与反向链接面板将在后续版本提供

## v0.0.1 - 2026-09-26

首个发布:P0–P3 功能主体全部就位。

### 编辑与预览

- 左右双栏实时预览,CommonMark + GFM(pulldown-cmark 单一解析器)
- 源码模式 / Live Preview(块级即时渲染)切换,共用同一缓冲与撤销栈
- 多标签编辑,关闭未保存标签时弹确认
- 代码块语法高亮与一键复制

### 文件与知识管理

- 新建 / 打开 / 保存 / 另存为,原子落盘
- 文件树:懒加载、`.gitignore` 过滤、当前文件高亮
- 大纲面板:点击跳编辑器光标,亦可跳预览对应位置
- 侧边栏全文搜索,支持正则,结果点击跳转
- `[[wikilink]]` 双向链接

### AI 智能层

- AI 流式写作;未配置 API key 时内置 Mock 演示,开箱可试
- `ai://` 链接协议与 AI 指令块
- AI commit message 生成、AI 摘要大纲
- API key 本地加密存取,不进 Git

### 版本层

- Git 只读集成:状态 / 提交历史 / diff / blame / 文件回滚

### 外观与配置

- 浅色 / 深色 / 跟随系统三态主题,`themes/*.ron` 皮肤文件
- 标准 / 紧凑界面密度,快捷键可改绑
- 设置持久化(`settings.json`)

### 导出与集成

- HTML 导出,单文件内嵌样式
- 内置 MCP 服务器:五个只读工具,stdio / HTTP 双通道;默认关闭,开启后仅绑定 `127.0.0.1`

### 已知限制

- Linux 下 fcitx5 输入法候选框暂不跟随光标(输入本身可用,排查进行中)
- macOS 从 Releases 直下 dmg 首次打开会被 Gatekeeper 拦截,执行一次
  `sudo xattr -dr com.apple.quarantine /Applications/LaterMD.app` 即可;推荐 Homebrew 安装,无此问题
- Windows 无代码签名,SmartScreen 会提示,选「更多信息 → 仍要运行」
- Live Preview 内联半隐藏样式与反向链接面板将在后续版本提供
