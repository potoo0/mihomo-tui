TODO List

- [ ] header 宽度不足: 显示的 tab 自动隐藏 或者 版本号精简?
- [ ] 优化 docs/localization.md
- [ ] 快捷键渲染优化
    - [ ] 优化 `Shortcut::from` 为宏? Fragment 可以 static str
    - [ ] Compact 是否可以优化到 Fragment 上?
    - [ ] 统一 key 和 label 顺序
- [ ] locales 优化
    - [ ] 允许多个 `t!` 组合使用,
      例如 `help.proxies.refresh: refresh proxies` 是否可以去掉,
      使用 `t!(help.proxies) t!(refresh)` 组合?
    - [ ] 尽量减少条目，否则维护起来太复杂!
- [ ] proxy / proxy provider 支持展示 api 返回的所有内容, 参考 connection detail
- [ ] 还原 rule 原始配置
- [ ] ~~popup 独立一种 trait~~
- [ ] ~~crate/jsonschema 校验~~, 不做, 低频场景让 api 返回错误即可
- [ ] ~~移除 mod.rs~~
