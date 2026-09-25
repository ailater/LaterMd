//! 预览面板:vendored `MarkdownLabel` 渲染快照。

use crate::state::PreviewState;
use eframe::egui;
use egui_markdown::MarkdownLabel;

/// 绘制预览面板。
pub fn ui(panel: &mut egui::Ui, preview: &PreviewState) {
    egui::ScrollArea::vertical()
        .id_salt("preview-scroll")
        // 不收缩宽度,让 wrap 以面板宽为界
        .auto_shrink([false, false])
        .show(panel, |ui| {
            // widget id 必须是常量:绝不含内容长度/hash,否则每次编辑都
            // 清空 vendored 层临时缓存,增量高亮与分段缓存全部失效
            // (AGENTS.md §6.7)。内容变化已在上游按修订号节流,这里每帧
            // 拿到的都是"仅在变化时重建"的同一字符串。
            MarkdownLabel::new(egui::Id::new("preview-md"), &preview.text)
                .wrap()
                // heal:true = 每帧渲染前对整篇文本补闭合(vendored parser::heal),
                // AI 流式输出的残缺帧(未闭合 fence/加粗)语法合法,完整文档
                // 上是恒等变换(Cow::Borrowed 原样返回)。P1 流式的必需品
                // (AGENTS.md §6.5),岔路登记见 docs/decisions-pending.md #10。
                .heal(true)
                .show(ui);
        });
}
