//! 把应用图标嵌进 Windows exe 的资源段(#74)。
//!
//! 为什么需要它:`main.rs` 挂的 `ViewportBuilder::with_icon` 只管**运行时
//! 窗口**图标(任务栏 / 标题栏 / Alt-Tab),那走的是 WM_SETICON。资源管理器
//! 列表、任务管理器的「详细信息」列、开始菜单 pinned 项读的是**exe 文件
//! 自己的资源段**—— 不嵌就是Windows 默认毛坯,两者互不替代。
//!
//! 为什么只在 Windows 编译:`winresource` 调的是 `windres`/`rc.exe` 资源
//! 编译器,其它平台既用不上也不该编。故整段`#[cfg]` 圈死,非 Windows 上
//! 本build.rs 退化为空,不留一行无效代码。
//!
//! 路径用 `CARGO_MANIFEST_DIR` 拼而非 `cwd`:build.rs 的工作目录是包根,
//! 但经`cargo package` 解包后相对路径会失效,绝对拼装才稳。素材在库由
//! `.gitignore` 未排除(见仓库 assets/logo/),`include_bytes` 的编译期
//! 校验不适用于 build.rs,故这里 `assert!` 存在性,素材被挪 → 构建红。

#[cfg(windows)]
fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let icon =
        std::path::Path::new(&manifest).join("../../assets/logo/deliverables/windows/latermd.ico");
    // 只在文件缺失时报错,不在每次构建都打印(cargo 对 build.rs 输出有缓存,
    // 打印反而每次都重编)。缺素材是仓库问题,该红。
    assert!(
        icon.exists(),
        "Windows 图标素材缺失:{} —— 跑 assets/logo/build-deliverables.py 生成",
        icon.display()
    );

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon.to_str().expect("图标路径应为 UTF-8"));
    // 图标名保持默认(IDI_APPLICATION 槽位)。改名会让资源管理器与
    // 开始菜单的引用对不上,表现为「图标时有时无」。
    resource
        .compile()
        .expect("嵌入 Windows 图标失败(rc.exe 调用异常)");
    println!("cargo:rerun-if-changed={}", icon.display());
}

#[cfg(not(windows))]
fn main() {
    // 非 Windows 平台:无事可做。留一个空的 main 而非条件编译整个文件,
    // 是为了让上面的文档注释在所有平台上都能被 rustdoc 读到。
}
