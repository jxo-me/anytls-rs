# ADR 0001：sing-box outbound ⇄ anytls-rs 服务端对接

- 状态：Proposed
- 作者：<你的姓名>
- 日期：2025-11-08
- 决策类型：架构/集成

## 背景

- 目标：提供 sing-box outbound → anytls-rs 服务端 的最小可行对接，覆盖 TCP、UDP-over-TCP、心跳和空闲会话回收。
- 约束：低延迟、稳定复用、可回滚、易观测，保留现有 CLI 行为。

## 决策

1. **首选方案**：保持 anytls-rs 作为服务端角色，兼容 sing-box outbound。  
   - 修改点集中在文档、示例、观测补丁，核心协议无需破坏性变更。  
   - 扩展 tracing 埋点，补齐 UDP/心跳日志。  
   - 提供 e2e 示例与基准脚本。
2. **备选方案**：anytls-rs 作为客户端接入 sing-box inbound（互通回归）。  
   - 暂不立即实施，仅记录需求；后续可在 client 模块引入 `--mode singbox` 动态开关。

## 考量

- 兼容性：对齐 sing-box AnyTLS 字段（`password`, `idle_session_check_interval`, `min_idle_session`, `tls.*`）。
- 安全性：保留 SHA256 + padding；评估 `md5` 可替换可能。
- 可观测性：增加握手、心跳、流关闭、超时的 span 与字段。
- 回滚：所有新增功能通过文档/脚本与 feature flag 控制；不破坏核心行为。

## 后续计划（摘要）

- 文档化配置与示例。
- 扩展 tracing 与指标输出。
- 编写 UDP-over-TCP 回环测试与基准。
- 补充 FAQ 与故障排查指南。

## 风险

- UDP-over-TCP 与 sing-box v1.12 行为差异。
- 会话池参数默认值与 sing-box 不一致。
- 观测不足导致排查困难。

## 参考

- sing-box AnyTLS outbound 配置：<https://sing-box.sagernet.org/configuration/outbound/anytls/>
- anytls-go 实现：<https://github.com/anytls/anytls-go>
- anytls-rs `server/`, `session/`, `client/` 模块源码

