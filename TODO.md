TODO List

- [ ] proxy 相关页面 action 相互交织, 考虑重构, 使用全局状态管理, 以减少组件之间不必要的状态、事件耦合
- [x] 代码可读性问题: 命名统一, 代码组件名与展示内容一致(search -> filter)
- [ ] ~~popup 独立一种 trait~~
- [x] overlay 改名
- [x] overlay 弹出后无法 `ctrl+c`, `ctrl+c` 终止的逻辑放到 root_component 的 handle_key_event 开头
- [x] proxy provider 页面下方向键应该切换到下一行
- [x] api 异常时将 body 放在 Error 里返回
- [ ] 还原 rule 原始配置
- [ ] ~~crate/jsonschema 校验~~, 不做, 低频场景让 api 返回错误即可
- [ ] ~~移除 mod.rs~~
