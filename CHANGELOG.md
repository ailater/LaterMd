# Changelog

本文件维护各版本变更。发布链路(cargo-dist)会把对应版本的小节自动注入 GitHub Release 正文;
发版时把 `Cargo.toml` 的 `workspace.package.version` 提到新版本号,并在顶部追加
`## v<版本> - <日期>` 小节(两者同 PR 提交,合入后 auto-tag 自动打 tag 发布)。

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
