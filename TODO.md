TODO List

- [ ] connections: filter 字段过滤语法
    - design: `field1:expr1 field2:"expr 2" global expr` (暂不考虑逻辑或)
- [ ] proxy: 清空不使用的 history 以优化内存占用; 展示 tcp/udp/provider-name/dialer-proxy ?
    - 目前链式代理相关 mihomo 核心缺失 API, dialer-proxy 做不了
- [ ] 主题色?
- [ ] 还原 rule 原始配置
- [ ] ~~popup 独立一种 trait~~
- [ ] ~~crate/jsonschema 校验~~, 不做, 低频场景让 api 返回错误即可
- [ ] ~~移除 mod.rs~~
