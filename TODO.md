TODO List

- [x] root.draw 限制最小视图
- [ ] 自身日志修复, 级别仅针对当前 crate?
- [ ] 自身日志补充
- [x] component 统一成 self.render xxx
- [ ] ~~移除弃用的 mod.rs~~
- [ ] shortcut 渲染组件
- [x] overview traffic 上下布局
- [ ] connection detail shortcuts
- [x] connections: 只在 live mode 发生更改时发送事件
- [x] connections: left/right 跳过不可排序列
- [x] app: `Action::Ordering` filter/order 排序修复; 非 live mode 时立即 filter/order
- [x] app: `load_connections` skip filter/order if `tab != connections`
- [x] 修复滚动条遮挡最后一列
- [x] cli help 补充 default config path
- [ ] 移除 `Component.draw` 的 AppState 参数

```rust
fn title_span(title: &str, title_style: Style, border_style: Style) -> Spans {
    Spans::from(
        vec![
            Span::styled(format!("{} ", TOP_RIGHT), border_style),
            Span::styled(format!("{}", title), title_style),
            Span::styled(format!(" {}", TOP_LEFT), border_style)
        ]
    )
}
```
