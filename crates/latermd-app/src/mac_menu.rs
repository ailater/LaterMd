//! macOS 系统菜单栏。
//!
//! winit 会提供一个默认菜单，但它只有通用的应用项。LaterMD 在这里安装
//! 一套稳定的 macOS 菜单结构，让菜单出现在屏幕顶部，而不是 egui 窗口内。

use objc2::sel;
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use objc2_foundation::{ns_string, MainThreadMarker, NSProcessInfo};

fn item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<objc2::runtime::Sel>,
) -> objc2::rc::Retained<NSMenuItem> {
    let title = objc2_foundation::NSString::from_str(title);
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(mtm.alloc(), &title, action, ns_string!(""))
    }
}

fn menu(
    mtm: MainThreadMarker,
    title: &str,
    entries: &[(&str, Option<objc2::runtime::Sel>)],
) -> objc2::rc::Retained<NSMenu> {
    let title = objc2_foundation::NSString::from_str(title);
    let menu = NSMenu::initWithTitle(mtm.alloc(), &title);
    for (label, action) in entries {
        menu.addItem(&item(mtm, label, *action));
    }
    menu
}

pub fn install() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    NSProcessInfo::processInfo().setProcessName(ns_string!("LaterMD"));
    let app = NSApplication::sharedApplication(mtm);
    let menubar = NSMenu::new(mtm);

    let app_item = item(mtm, "LaterMD", None);
    let app_menu = menu(
        mtm,
        "LaterMD",
        &[
            ("关于 LaterMD", Some(sel!(orderFrontStandardAboutPanel:))),
            ("退出 LaterMD", Some(sel!(terminate:))),
        ],
    );
    app_item.setSubmenu(Some(&app_menu));
    menubar.addItem(&app_item);

    for (title, entries) in [
        (
            "文件",
            vec![
                ("新建", Some(sel!(newDocument:))),
                ("打开…", Some(sel!(openDocument:))),
                ("保存", Some(sel!(saveDocument:))),
                ("另存为…", Some(sel!(saveDocumentAs:))),
            ],
        ),
        (
            "编辑",
            vec![
                ("撤销", Some(sel!(undo:))),
                ("重做", Some(sel!(redo:))),
                ("剪切", Some(sel!(cut:))),
                ("复制", Some(sel!(copy:))),
                ("粘贴", Some(sel!(paste:))),
            ],
        ),
        ("格式", vec![("加粗", None), ("斜体", None), ("代码", None)]),
        (
            "显示",
            vec![("切换侧栏", None), ("切换预览", None), ("切换 Live", None)],
        ),
        (
            "窗口",
            vec![
                ("最小化", Some(sel!(performMiniaturize:))),
                ("关闭", Some(sel!(performClose:))),
            ],
        ),
        ("帮助", vec![("LaterMD 帮助", None)]),
    ] {
        let top = item(mtm, title, None);
        let submenu = menu(mtm, title, &entries);
        top.setSubmenu(Some(&submenu));
        menubar.addItem(&top);
    }

    app.setMainMenu(Some(&menubar));

    // AppKit 会在 `setMainMenu` 时用进程名重写第一个菜单项的标题。
    // 二进制名保持小写 `latermd`（便于命令行调用），系统菜单则遵循
    // 产品名的大小写规范，显示为 `LaterMD`。
    let app_title = objc2_foundation::NSString::from_str("LaterMD");
    app_item.setTitle(&app_title);
}
