# 🎉 16 个 PR 的整合审查与测试报告（最终更新，未发布二进制）

## 📌 最终更新说明

本公告记录 2026-10-23 对上游当前开放 PR 的重新审查及有限整合。上游 `main` 基线为 `e229abdbab71e377b86632e588e607413af4b93c`（2026-10-20）。本次在个人 Fork 的独立分支整合 PR #43；PR #52 曾试合并，但造成 UI 布局回归，已撤回并暂缓。其余 PR 均未被整合。

当前整合分支通过离线工作区测试和 all-target 检查。由于现有程序尚未满足本次审查采用的活动真实性发布边界，本次没有构建或发布公开二进制，也没有创建新版本标签。此公告不声称上游已采纳任何建议。

## 📋 全部 16 个 PR 审核状态

| PR | 标题 | 作者 | 当前状态 | 本次处理说明 |
|---|---|---|---|---|
| [#41](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/41) | 指标修复与已验证整合内容 | DemaciaForge | 开放；有文本冲突 | 不纳入：范围涉及轨迹生成、策略及活动提交，并与 #60/#106 重叠。建议拆分后单独审查。 |
| [#42](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/42) | 提交时围栏元数据可选 | DemaciaForge | 开放；GitHub 显示可合并 | 暂缓：需用协议夹具区分缺失、无效和显式空围栏；与 #53 相关。 |
| [#43](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/43) | Windows/Android 测试拆分与发布治理 | DemaciaForge | 开放；GitHub 显示可合并 | 已整合：CI/工作流维护改动，不修改活动协议。 |
| [#44](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/44) | 可选田径跑道模式 | DemaciaForge | 开放；有文本冲突 | 不纳入：增加生成活动轨迹的运行模式。 |
| [#51](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/51) | 道路模式必经点及普通点强制经过 | laotierocket（Skylar） | 开放；有文本冲突 | 不纳入：强制生成经过打卡点的轨迹并改变提交标记；与 #60/#107 相交。 |
| [#52](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/52) | 响应式免费分发水印 | DemaciaForge | 开放；GitHub 显示可合并 | 暂缓：整合后 10 项 UI 可达性测试失败；撤回后干净基线和整合分支均通过。应修复窄屏布局后再审。 |
| [#53](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/53) | 校园跑可选绿色电子围栏 | DemaciaForge | 开放；有文本冲突 | 不纳入：直接改变围栏、点位、轨迹生成和提交路径；与 #42 相关。 |
| [#57](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/57) | 跨平台定位、地图选点、在线路网与登录备份 | LightWhite520 | 开放；GitHub 显示可合并 | 暂缓：改动范围大；登录归档需单独隐私审查，路线功能需单独安全审查。 |
| [#60](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/60) | 保留 road 有效里程并补齐打卡完成标记 | DemaciaForge | 开放；有文本冲突 | 不纳入：改变有效里程、时间窗口及活动提交中的打卡状态；与 #41/#51/#106/#107 重叠。 |
| [#63](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/63) | 手绘路线、高德自定义路线及轨迹导入 | Autumn984 | 开放；有文本冲突 | 不纳入：导入/自定义路线进入活动轨迹生成；与 #57/#66 有功能重叠。 |
| [#66](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/66) | 将 leisure=track 跑道纳入路网并连接孤立图 | jie6886（Flip131） | 开放；GitHub 显示可合并 | 暂缓：路网加载改动服务于路线规划；尚未确认可安全独立的发布范围。 |
| [#79](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/79) | Windows 与 Android 青绿色卡片 UI 改版 | kek2233（keke） | 开放；GitHub 显示可合并 | 暂缓：涉及大范围 UI 重写及 vendored winit 修改，需平台回归。 |
| [#82](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/82) | 所有时间统一按北京时间 | cokeenet（Cokee） | 开放；有文本冲突 | 暂缓：影响几何、排名和 wire 时间；需补充跨时区及时间窗口边界测试。 |
| [#105](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/105) | 自由跑模式 | neo0y（yangzhiying） | 开放；GitHub 显示可合并 | 不纳入：无配置打卡点时增加独立活动提交路径，会绕过必经点要求。 |
| [#106](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/106) | 打卡点位置、10 秒窗口统计与点位拉取距离 | ConCosew（ConCose） | 开放；GitHub 显示可合并 | 不纳入：改变生成轨迹、点位位置和打卡时间窗口；与 #41/#60/#82/#107 相交。 |
| [#107](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/107) | 普通点位达标计数测试 | DemaciaForge | 开放；GitHub 显示可合并 | 不纳入：虽只改测试，但断言普通点位应计为打卡通过，涉及活动真实性计数语义。 |

“有文本冲突”来自 GitHub 对 PR 合并状态的查询；“可合并”仅表示文本层面可能合并，不代表行为正确。上述排除/暂缓是本次范围和安全边界下的建议，不代表这些 PR 必然存在已确认缺陷，也不代表上游维护者已作决定。

## ✅ 本次实际整合内容

本次仅保留 #43 的维护性改动：拆分 Windows 测试与 Android 冒烟工作流，并限制自动 Release 工作流触发范围。该 PR head `29e7fbe7580fcf740a4d708ab5a57cdab1a6d62c` 已进入 Fork 整合分支。

#52 的水印面板压缩了小屏页面高度。含 #52 时出现 10 个 UI 可达性断言失败；撤回 #52 后相关测试恢复通过。没有删除或弱化任何断言。

## 📦 Fork 分支与 Release

- Fork 整合分支：<https://github.com/DemaciaForge/NekoSportsWorldTool/tree/integration/upstream-main-pr-audit-2026-10-23>
- 当前分支远端 HEAD 为 `909118e7e1a337ee38736f8ac2917aa5fcfa2d21`；撤回 #52 的提交为 `8c95f6e`；水印替代修复提交为 `24f9663`，上游替代 PR 为 [#109](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/109)。
- 没有本次 Release、Tag、二进制下载文件或 SHA-256，因此不提供虚构版本号或下载链接。
- Fork 既有 v0.4.1 等历史 Release 并非本次审查的产物。
- 原因：本次只做维护性整合，未完成对程序整体活动真实性边界的改造；因此不发布可用于把合成活动记录作为真实活动提交的公开二进制。

## 🔬 验证结果

最终整合分支：

```text
cargo test --workspace --locked --offline       通过：182 + 8 + 7 项；失败 0
cargo check --all-targets --locked --offline    通过：exit code 0
git diff --check                                通过：exit code 0
cargo fmt --check                               未通过：报告大量格式差异；未做全仓自动格式化
Android 模拟器冒烟 / Android 交叉编译          未执行
Clippy / Release 构建                           未执行
```

干净 upstream 基线 `e229abdbab71e377b86632e588e607413af4b93c` 使用同一离线测试命令通过：182 项主库测试、8 项 route-planner 单测、7 项集成测试，0 失败。该基线对照确认含 #52 的 10 项布局失败是此次水印整合引入的回归；撤回后最终分支恢复全通过。测试产生 Windows linker stdout 警告，但无失败。

## ⚠️ 问题与剩余验证

1. #52 的小屏布局回归已通过对照复现并撤回；建议缩小水印高度或重做窄屏布局后再提交。
2. `cargo fmt --check` 未通过；差异遍及既有代码，本次没有进行与功能无关的全仓格式化。
3. Android 冒烟/交叉编译、Clippy、Release 构建尚未运行。
4. 本次没有向真实校园运动服务发送请求，也没有真实活动提交测试。
5. 整合分支不修改上游 `main`，Fork 默认分支保持原样。

## 📊 整合成果统计

| 项目 | 数量/结果 |
|---|---:|
| 审查开放 PR | 16 |
| 完整采用 | 1（#43） |
| 部分采用 | 0 |
| 修复后采用 | 0 |
| 暂缓 | 6（#42、#52、#57、#66、#79、#82） |
| 不纳入 | 9（#41、#44、#51、#53、#60、#63、#105、#106、#107） |
| 最终整合分支测试 | 197 项通过、0 失败；命令涵盖 182 + 8 + 7 项 |
| 基线测试 | 197 项通过、0 失败 |
| Release | 未发布 |

## 🎯 对上游维护者的建议

- #43：可直接审查维护性改动；本次已在 Fork 整合并通过本地全套工作区测试。
- #52：修复窄屏 UI 可达性回归后再合并。
- #42/#53：一并明确围栏字段缺失、无效和显式空值的协议语义，再用固定夹具验证。
- #41/#60/#106/#82：先协调指标、有效里程、时间窗口及打卡 wire 字段，再分 PR 验证。
- #51/#107：先明确普通点位、必经点和达标统计语义；当前不建议把测试通过等同于真实活动有效。
- #44/#63/#105：涉及生成的路线或活动提交；应先按项目真实性规则讨论功能边界。
- #57/#79：拆分隐私、定位、地图和界面改动，并提供 Android/Windows 回归证据。
- #66：说明路网连接行为如何避免影响校区边界及活动生成，再补充分区路网测试。

建议批次：先处理 #43；#52 完成 UI 修复后再处理；其余按围栏、时间/指标、路线/定位主题拆分审查。以上仅是社区审查建议。

## 📝 详细报告与相关链接

- 完整报告：<https://github.com/DemaciaForge/NekoSportsWorldTool/blob/integration/upstream-main-pr-audit-2026-10-23/PR_INTEGRATION_REPORT_2026-10-23.md>
- Fork 整合分支：<https://github.com/DemaciaForge/NekoSportsWorldTool/tree/integration/upstream-main-pr-audit-2026-10-23>
- 上游 PR：[#41](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/41)、[#42](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/42)、[#43](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/43)、[#44](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/44)、[#51](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/51)、[#52](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/52)、[#53](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/53)、[#57](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/57)、[#60](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/60)、[#63](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/63)、[#66](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/66)、[#79](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/79)、[#82](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/82)、[#105](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/105)、[#106](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/106)、[#107](https://github.com/YanamiNeko/NekoSportsWorldTool/pull/107)。
- 历史公告参考：<https://github.com/YanamiNeko/NekoSportsWorldTool/issues/68>。

## 🤝 开放协作与非官方声明

本次工作是社区贡献者基于公开代码开展的非官方审查和有限整合，不代表作者或维护者认可。只有作者或具备相应权限的维护者可以决定是否将建议合并到上游。欢迎维护者和其他贡献者针对报告中的代码证据、测试结果及处理建议继续讨论。




