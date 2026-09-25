//! 预览面板:vendored `MarkdownLabel` 渲染快照。

use crate::ai_link::{self, SCHEME};
use crate::state::{Message, PreviewState};
use eframe::egui;
use egui_markdown::link::{LinkHandler, LinkStyle};
use egui_markdown::MarkdownLabel;
use std::cell::RefCell;

/// ai:// 链接的样式色(紫罗兰,与默认超链接色区分),按明暗主题取两档。
fn ai_link_color(dark_mode: bool) -> egui::Color32 {
    if dark_mode {
        egui::Color32::from_rgb(0xC9, 0x9B, 0xF5)
    } else {
        egui::Color32::from_rgb(0x8B, 0x2F, 0xC9)
    }
}

/// vendored [`LinkHandler`] 的 ai:// 扩展点:样式区分 + 点击拦截。
///
/// `click` 只有 `&self`,产出的消息暂存 [`RefCell`],帧末由 [`ui`] 并入
/// outbox(下一帧归约执行)。返回 true 拦下 vendored 层的默认
/// `open_url`;非 ai:// 前缀返回 false,链接照常走系统浏览器。协议语义见
/// `ai_link` 模块文档。
struct AiLinkHandler {
    clicked: RefCell<Vec<Message>>,
    color: egui::Color32,
}

impl AiLinkHandler {
    /// 帧末收口:把点击消息并入 outbox。
    fn drain_into(&self, outbox: &mut Vec<Message>) {
        outbox.append(&mut self.clicked.borrow_mut());
    }
}

impl LinkHandler for AiLinkHandler {
    /// ai:// 链接换色;`underline: true` 只是声明意图 —— vendored 层当前
    /// 未消费该字段(hover 下划线对全部链接无条件绘制),见 decisions-pending #11。
    fn link_style(&self, href: &str) -> Option<LinkStyle> {
        href.starts_with(SCHEME).then_some(LinkStyle {
            color: Some(self.color),
            underline: true,
        })
    }

    fn click(&self, _text: &str, href: &str, _ui: &mut egui::Ui) -> bool {
        match ai_link::parse(href) {
            Some(prompt) => {
                self.clicked
                    .borrow_mut()
                    .push(Message::AiLinkClicked { prompt });
                true
            }
            None => false,
        }
    }
}

/// 绘制预览面板。
pub fn ui(panel: &mut egui::Ui, preview: &PreviewState, outbox: &mut Vec<Message>) {
    egui::ScrollArea::vertical()
        .id_salt("preview-scroll")
        // 不收缩宽度,让 wrap 以面板宽为界
        .auto_shrink([false, false])
        .show(panel, |ui| {
            // widget id 必须是常量:绝不含内容长度/hash,否则每次编辑都
            // 清空 vendored 层临时缓存,增量高亮与分段缓存全部失效
            // (AGENTS.md §6.7)。内容变化已在上游按修订号节流,这里每帧
            // 拿到的都是"仅在变化时重建"的同一字符串。
            let handler = AiLinkHandler {
                clicked: RefCell::new(Vec::new()),
                color: ai_link_color(ui.visuals().dark_mode),
            };
            MarkdownLabel::new(egui::Id::new("preview-md"), &preview.text)
                .wrap()
                // heal:true = 每帧渲染前对整篇文本补闭合(vendored parser::heal),
                // AI 流式输出的残缺帧(未闭合 fence/加粗)语法合法,完整文档
                // 上是恒等变换(Cow::Borrowed 原样返回)。P1 流式的必需品
                // (AGENTS.md §6.5),岔路登记见 docs/decisions-pending.md #10。
                .heal(true)
                .link_handler(&handler)
                .show(ui);
            handler.drain_into(outbox);
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::RawInput;

    /// handler 三态:ai:// 链接解析暂存且被拦截;非 ai:// 前缀不拦截(返回
    /// false,vendored 默认走系统浏览器);link_style 只对 ai:// 换色。
    #[test]
    fn handler_intercepts_ai_links_and_passes_through_others() {
        let ctx = egui::Context::default();
        let mut outbox = Vec::new();
        ctx.run_ui(RawInput::default(), |ui| {
            let handler = AiLinkHandler {
                clicked: RefCell::new(Vec::new()),
                color: ai_link_color(ui.visuals().dark_mode),
            };
            assert!(handler.click("续写", "ai://write?prompt=%E7%BB%AD%E5%86%99", ui));
            assert!(
                !handler.click("LaterMD", "https://github.com/ailater/LaterMd", ui),
                "非 ai:// 前缀不拦截"
            );
            let style = handler.link_style("ai://write?prompt=x").unwrap();
            assert_eq!(style.color, Some(ai_link_color(true)));
            assert!(style.underline);
            assert!(
                handler.link_style("https://example.com").is_none(),
                "普通链接保持默认超链接样式"
            );
            handler.drain_into(&mut outbox);
        })
        .drop_without_applying_deltas();
        assert_eq!(
            outbox,
            vec![Message::AiLinkClicked {
                prompt: Ok("续写".into())
            }]
        );
    }
}
