TODO List

- [ ] ~~移除 mod.rs~~
- [x] root.draw 限制最小视图
- [ ] 自身日志补充; trace 级别仅针对当前 crate?
- [x] 更换 `tokio::select!` 为 `futures_util::stream::take_until`, 链式写法更清晰, select 宏代码提示/格式化不友好
- [x] component 统一成 self.render xxx
- [x] shortcut 与 highlight 合并
- [x] help / shortcuts 补充
- [x] overview traffic 上下布局
- [x] connections: left/right 跳过不可排序列
- [x] cli help 补充 default config path
- [x] 移除 `Component.draw` 的 AppState 参数
- [x] 销毁后台 tab
- [x] 关闭连接
- [ ] 代理: 查看/切换/测延迟
- [x] api 禁用 proxy
- [x] connections / logs 显示当前选择的索引
- [ ] logs 左右滚动
- [ ] 修复 tab 切换后搜索/排序异常
