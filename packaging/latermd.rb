# Homebrew cask —— 复刻 lscreen 模式(AGENTS.md §5 分发策略),放到
# crazykun/homebrew-ailater 的 Casks/ 目录即可分发。
#
# 资产命名:latermd-v{version}-universal2-apple-darwin.dmg
# 由本仓 .github/workflows/macos-dmg.yml 产出(双架构 lipo 合一)。
# 命名**不含**平台/版本之外的变量,且版本号只出现在固定两处,
# 供 livecheck :github_latest 自动发现新版本(roadmap 风险登记册 #7)。
#
# sha256:首次发布后从 macos-dmg job 的 step summary 取真值填入;
# 占位用 :no_check 是为了让 cask 在填值前也可安装(不校验校验和)。

cask "latermd" do
  version "0.1.0"
  sha256 :no_check # TODO: 首个 Release 发布后填 macos-dmg job 输出的 sha256

  url "https://github.com/ailater/LaterMd/releases/download/v#{version}/latermd-v#{version}-universal2-apple-darwin.dmg"
  name "LaterMD"
  desc "Cross-platform Markdown knowledge workbench"
  homepage "https://github.com/ailater/LaterMd"

  livecheck do
    url :url
    regex(/^v?(\d+(?:\.\d+)+)$/i)
    strategy :github_latest
  end

  # 平台基线 macOS 14 Sonoma(AGENTS.md §5:显式不支持 macOS 13 及以下)
  depends_on macos: :sonoma

  app "LaterMD.app"

  # 无签名证书路线:移除隔离属性,避免 Gatekeeper 报「已损坏,无法打开」
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/LaterMD.app"]
  end

  # 主题设置 settings.json 与文件树持久化都在该目录(theme.rs::config_dir)
  zap trash: "~/Library/Application Support/latermd"
end
