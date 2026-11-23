# Fork 提交盘点与主线纳入评估

**盘点日期：** 2026-11-23  
**上游仓库：** [`jxo-me/anytls-rs`](https://github.com/jxo-me/anytls-rs)  
**比较基准：** `main`，提交 `0528e92`（v0.5.5，排除示例私钥）

> 当前工作区另有 4 个尚未提交的 GitHub Actions Node.js 24 版本升级；比较基准使用 `0528e92` 的已提交内容，这些本地 workflow 修改不属于 fork 差异。

## 结论摘要

GitHub 当前列出 6 个 fork。相对于上游 `main`，3 个 fork 有新增提交；另外 3 个 fork 的 `main` 均落后上游 4 个提交，没有独有提交。活跃提交主要来自 `litecn` 和 `zhu`；`madeye` 的 3 个提交也出现在 `zhu` 的分支历史中，按功能评估时不重复计算。

建议优先从 fork 的实现思路中单独移植并测试以下修复：

1. DNS cache 只缓存 IP 地址，不缓存请求端口；同时限制缓存容量。
2. 证书状态以当前时间重新计算，并检查 `not_before`/`not_after` 的精确边界。
3. 拒绝超过协议 `u16` 长度字段上限的 frame payload。
4. 对 DNS 返回的多个 TCP/UDP 地址逐一连接，避免首个地址失败就放弃。
5. 给 TLS 握手和连接任务增加资源上限、超时和有界退出路径。

这些是**代码审查候选项**，不是已合并或已验证的补丁。fork 的测试没有在本次盘点中执行，也没有把任何 fork 分支合并进主线。

## Fork 清单

| Fork 用户 | 默认分支 | 默认分支相对上游 | 最近推送 | 备注 |
|---|---|---:|---|---|
| [`litecn/anytls-rs`](https://github.com/litecn/anytls-rs) | `cokvr` | 领先 14、落后 4 | 2026-11-12 | `main` 分支无新增提交；功能提交在 `cokvr` |
| [`jellybobbin/anytls-rs`](https://github.com/jellybobbin/anytls-rs) | `main` | 领先 0、落后 4 | 2025-11-11 | 无独有提交 |
| [`alawnchen/anytls-rs`](https://github.com/alawnchen/anytls-rs) | `main` | 领先 0、落后 4 | 2025-11-11 | 无独有提交 |
| [`zhu/anytls-rs`](https://github.com/zhu/anytls-rs) | `main` | 领先 35、落后 4 | 2026-11-19 | `main`、`custom/server-compat-v0.5.4.2`、`custom/client-issue-drafts` 高度重叠 |
| [`madeye/anytls-rs`](https://github.com/madeye/anytls-rs) | `main` | 领先 3、落后 4 | 2026-06-13 | Android hook 和资源清理提交也被 zhu 分支引用 |
| [`n1o1s1/anytls-rs`](https://github.com/n1o1s1/anytls-rs) | `main` | 领先 0、落后 4 | 2025-11-11 | 无独有提交 |

比较使用 GitHub 的 `上游 main...fork:branch` 提交比较；同时检查了以上 fork 的远端分支，而不只检查默认分支。zhu 的额外分支包括 `bugfix/backpressure-bounded-channels`（4 个领先提交）、`custom/client-issue-drafts`（34 个）和 `custom/server-compat-v0.5.4.2`（35 个）。这些分支大部分是相同提交序列；独有内容是备用 backpressure 提交、客户端 issue 草稿文档，以及 `893cca47` 的连接拒绝延时测试。

## 提交更新目录

下列清单按 fork 分组。提交标题为上游 API 返回的原始标题；zhu 的多个分支重复提交已去重，只有 SHA 不同但目标相同的 backpressure 实现另行标明。

### litecn：`cokvr` 分支（14 个领先提交）

| 提交 | 更新 |
|---|---|
| [`44f028f`](https://github.com/litecn/anytls-rs/commit/44f028fe80) | 添加 SOCKS5/HTTP 共用 mixed listener |
| [`031fb9e`](https://github.com/litecn/anytls-rs/commit/031fb9ef97) | 添加 `--forward`，转发未经 AnyTLS 认证的 HTTP/HTTPS 连接 |
| [`c34a824`](https://github.com/litecn/anytls-rs/commit/c34a8247f5) | `-l` 与 `-H` 地址相同时推断启用 mixed listener |
| [`78089f2`](https://github.com/litecn/anytls-rs/commit/78089f2183) | 添加显式 `-m/--mixed` 选项 |
| [`bcb386c`](https://github.com/litecn/anytls-rs/commit/bcb386cb79) | 修正手动 dispatch 版本提取，并移除 crates.io 自动发布 |
| [`e8b373d`](https://github.com/litecn/anytls-rs/commit/e8b373d960) | Release artifact 名称使用 dispatch 输入的版本 |
| [`3ca15f0`](https://github.com/litecn/anytls-rs/commit/3ca15f08a0) | rustls 从 aws-lc-rs 后端切换为 ring |
| [`2b185e8`](https://github.com/litecn/anytls-rs/commit/2b185e83fb) | Fork 自行准备 v0.5.5 release |
| [`0f96f0a`](https://github.com/litecn/anytls-rs/commit/0f96f0a0c0) | 限制 stream 到 wire 的队列，避免 OOM |
| [`e069af9`](https://github.com/litecn/anytls-rs/commit/e069af93da) | 将本地 release 输出目录加入 `.gitignore` |
| [`585db52`](https://github.com/litecn/anytls-rs/commit/585db52a3a) | accept 暂时性错误时退避，避免 CPU/log 热循环 |
| [`ea29621`](https://github.com/litecn/anytls-rs/commit/ea2962190c) | 添加 SOCKS5 上游 outbound proxy |
| [`7ad5b02`](https://github.com/litecn/anytls-rs/commit/7ad5b027fc) | Fork 自行准备 v0.5.6 release |
| [`8fc6847`](https://github.com/litecn/anytls-rs/commit/8fc6847ad6) | release 压缩包只附带二进制文件，不再附带 bundled docs |

### zhu：`main` 和兼容分支（35 个领先提交，其中含 1 个 merge commit）

| 提交 | 更新 |
|---|---|
| [`1d13bef`](https://github.com/zhu/anytls-rs/commit/1d13befb2f) | Android `VpnService.protect(fd)` socket hook |
| [`4766802`](https://github.com/zhu/anytls-rs/commit/47668026cc) | 修复 client session 和 server outbound fd/stream 泄漏 |
| [`e613488`](https://github.com/zhu/anytls-rs/commit/e6134889d3) | 合并上述 session fd/stream 清理 PR |
| [`f8b2d86`](https://github.com/zhu/anytls-rs/commit/f8b2d8698d) | 限制 stream 队列以施加 backpressure |
| [`02c4cf8`](https://github.com/zhu/anytls-rs/commit/02c4cf8531) | 添加自定义构建 workflow |
| [`4fe5c43`](https://github.com/zhu/anytls-rs/commit/4fe5c4351f) | 修正 release workflow 的版本提取 |
| [`9de6b20`](https://github.com/zhu/anytls-rs/commit/9de6b200a6) | release workflow 固定 Windows runner 到 `windows-2022` |
| [`9ed7462`](https://github.com/zhu/anytls-rs/commit/9ed74624fe) | DNS cache 改为只缓存 IP，端口按请求附加 |
| [`f7c0336`](https://github.com/zhu/anytls-rs/commit/f7c0336933) | 证书有效期按当前系统时钟重新计算 |
| [`e0131a9`](https://github.com/zhu/anytls-rs/commit/e0131a9acb) | 取消 transport task，避免关闭路径与 writer 死锁 |
| [`d8aaf8f`](https://github.com/zhu/anytls-rs/commit/d8aaf8f001) | 限制 TLS handshake 并监督连接 task 生命周期 |
| [`8d96295`](https://github.com/zhu/anytls-rs/commit/8d96295638) | 完成 stream closure 并释放等待中的操作 |
| [`423cd27`](https://github.com/zhu/anytls-rs/commit/423cd278fc) | 完善 UDP-over-TCP framing 和 dual-stack relay |
| [`784fd2b`](https://github.com/zhu/anytls-rs/commit/784fd2be69) | 暂时性 accept 错误时保留 active sessions |
| [`a39cf63`](https://github.com/zhu/anytls-rs/commit/a39cf63463) | 强制 session handshake 约束并保护 frame payload 长度 |
| [`86afef8`](https://github.com/zhu/anytls-rs/commit/86afef8c89) | 等待关闭清理完成并重试中断的 teardown |
| [`fb3af54`](https://github.com/zhu/anytls-rs/commit/fb3af545ed) | 无效 renewal 日期时保留当前活动证书 |
| [`389a668`](https://github.com/zhu/anytls-rs/commit/389a668224) | 测试按平台 UDP datagram 限制调整 |
| [`fea0e2b`](https://github.com/zhu/anytls-rs/commit/fea0e2bf26) | 完成 proxy stream 并尝试多个目标地址连接 |
| [`698730a`](https://github.com/zhu/anytls-rs/commit/698730a0e4) | abort 前等待 connection worker 清理 |
| [`45fdcea`](https://github.com/zhu/anytls-rs/commit/45fdcea738) | 等待 session map 清理前先通知已移除的 stream |
| [`2e62615`](https://github.com/zhu/anytls-rs/commit/2e62615047) | peer FIN teardown 前排空已排队 TCP payload |
| [`ad02d9f`](https://github.com/zhu/anytls-rs/commit/ad02d9fa2b) | 本地 UDP 地址失败时尝试下一个解析地址 |
| [`54b66c0`](https://github.com/zhu/anytls-rs/commit/54b66c004c) | stream cancellation 时保留共享 frame |
| [`e999868`](https://github.com/zhu/anytls-rs/commit/e9998682bc) | 添加对 pinned dae 和 anytls-go 客户端的互通测试 |
| [`9647684`](https://github.com/zhu/anytls-rs/commit/9647684c3e) | 准备 fork 私有 `custom-v0.5.4.2` release |
| [`f681a3a`](https://github.com/zhu/anytls-rs/commit/f681a3a061) | CI 继续运行所有平台并处理 beta Clippy lint |
| [`7d29b5e`](https://github.com/zhu/anytls-rs/commit/7d29b5eeac) | 限制 benchmark smoke test 并显式暴露失败 |
| [`bb963ea`](https://github.com/zhu/anytls-rs/commit/bb963ea8c9) | benchmark 消费有界队列并验证完整数据交付 |
| [`9424f3d`](https://github.com/zhu/anytls-rs/commit/9424f3d838) | CI 固定 Windows runner 到受支持的 VS image |
| [`84d1205`](https://github.com/zhu/anytls-rs/commit/84d1205a27) | 连接测试前释放 refused-port fixture |
| [`42b70f3`](https://github.com/zhu/anytls-rs/commit/42b70f36f2) | 根据 Shadowrocket 行为验证 padding negotiation |
| [`893cca4`](https://github.com/zhu/anytls-rs/commit/893cca47a1) | 为连接拒绝测试容纳平台延迟（仅兼容分支） |
| [`e408f26`](https://github.com/zhu/anytls-rs/commit/e408f26f94) | 更新依赖并将 DNS resolver 迁移到 Hickory |
| [`c250994`](https://github.com/zhu/anytls-rs/commit/c250994c46) | release 前强制 dependency audit |

其他分支内容：

- `bugfix/backpressure-bounded-channels` 有 [`3316ddf`](https://github.com/zhu/anytls-rs/commit/3316ddf44b)，是 backpressure 队列的另一版提交，与 `f8b2d86` 目标重复。
- `custom/client-issue-drafts` 额外有 [`32b9fde`](https://github.com/zhu/anytls-rs/commit/32b9fde4c5) 和 [`66c2c79`](https://github.com/zhu/anytls-rs/commit/66c2c797)，用于保存/复核客户端 issue 草稿和复现材料；是调查文档，不是已经修复的代码。

### madeye：主分支和 Android 分支（3 个领先提交）

| 提交 | 更新 |
|---|---|
| [`1d13bef`](https://github.com/madeye/anytls-rs/commit/1d13befb2f) | 添加 Android socket protect hook（也在 zhu 分支） |
| [`4766802`](https://github.com/madeye/anytls-rs/commit/47668026cc) | 修复 session/outbound fd 和 stream 清理（也在 zhu 分支） |
| [`e613488`](https://github.com/madeye/anytls-rs/commit/e6134889d3) | 合并资源清理 PR（也在 zhu 分支） |

## 主线纳入评估

### 建议优先单独移植

#### 1. 修正 DNS cache 的端口错误，并限制缓存内存

**来源：** zhu [`9ed7462`](https://github.com/zhu/anytls-rs/commit/9ed74624fe)，配套多地址改动 [`fea0e2b`](https://github.com/zhu/anytls-rs/commit/fea0e2bf26)、[`ad02d9f`](https://github.com/zhu/anytls-rs/commit/ad02d9fa2b)。

当前上游 `src/util/dns_cache.rs` 以 hostname 为 key，但缓存值是 `Vec<SocketAddr>`。同一主机先以端口 A 查询，随后以端口 B 查询时，cache hit 仍可能返回端口 A。这是可直接从当前代码确认的正确性问题。当前 cache 也没有条目数上限，目标 hostname 可来自客户端请求，持续输入不同主机名会使 HashMap 长期增长。

fork 的实现把 IP 与端口分开缓存，并设置 4096 条上限；proxy 还可以按顺序尝试多个解析地址。建议移植数据模型和上限设计，但评估清理/淘汰策略后再采用其实现。

**应补测试：**相同 hostname 连续以两个端口查询都得到各自端口；多个 A/AAAA 地址在首个目标不可连接时能连接后续地址；缓存超过上限后有界且有效项仍可命中；自定义 DNS 更新后缓存会失效。

#### 2. 证书状态改为按当前时刻计算

**来源：** zhu [`f7c0336`](https://github.com/zhu/anytls-rs/commit/f7c0336933)。

上游 `CertificateInfo::status()`、`is_expired()` 和 `is_expiring_soon()` 使用解析证书时写入的 `days_until_expiry` 快照。长期运行的证书监控对象不重新解析时，过期状态可能一直是旧值；把剩余时间向下取整也会让刚过期不到一天的证书仍显示 0 天而未过期。fork 改为当前时钟计算，并覆盖 `not_before` 尚未生效的状态。

建议独立移植 `status_at(now, ...)` 这类可注入时钟的 API，使边界测试稳定；设计需与现有 API 和 changelog/文档兼容。

**应补测试：**对象创建后时间推进到 warning 窗口、到达 `not_after`、超过 `not_after`；当前时间早于 `not_before`；过期不足 24 小时时仍判定 expired。

#### 3. 校验 frame 编码长度

**来源：** zhu [`a39cf63`](https://github.com/zhu/anytls-rs/commit/a39cf63463)。

协议头用 `u16` 写 payload 长度，而当前 `FrameCodec::encode()` 将 `usize` 直接 `as u16`。超过 65535 bytes 时长度会截断，编码流中的长度与实际 payload 不再一致，后续解码可能错位。fork 添加最大长度常量并在编码时拒绝超长 payload。

这是范围清晰、适合单独移植的防御性修复。

**应补测试：**恰好 65535 bytes 可编码；65536 bytes 返回 `InvalidInput`；失败后目标 buffer 不留下部分 frame。

#### 4. 限制握手资源并约束连接任务生命周期

**来源：** zhu [`d8aaf8f`](https://github.com/zhu/anytls-rs/commit/d8aaf8f001)、[`e0131a9`](https://github.com/zhu/anytls-rs/commit/e0131a9acb)、[`698730a`](https://github.com/zhu/anytls-rs/commit/698730a0e4)。

当前 server accept 后直接 spawn connection task，TLS handshake 没有可见的 deadline 或并发上限。慢连接会占用 task/连接资源；关闭路径和子 task 的所有权也值得专项审查。fork 为此做了跨 `server.rs`、`handler.rs`、`session.rs`、`stream.rs` 的较大重构，风险明显高于前 3 项。

建议先设计小步主线修复：TLS handshake timeout、并发 handshake semaphore、可等待或可取消的 task 集合；再分别评估 session/stream close 语义，不要 cherry-pick 整个 server/session 重写。

**应补测试：**不发送 TLS 数据的客户端在 deadline 后释放资源；超过上限的连接不会无限 spawn；server shutdown 后连接 worker 都结束；close 与 FIN/待写 frame 并发时无死锁、无 frame 损坏。

### 值得后续评审的改动

| 范围 | 来源 | 评估 |
|---|---|---|
| UDP 多地址 fallback、双栈和 UOT framing | zhu [`423cd27`](https://github.com/zhu/anytls-rs/commit/423cd278fc)、[`ad02d9f`](https://github.com/zhu/anytls-rs/commit/ad02d9fa2b) | 有互通价值；逐项比协议规范并做跨客户端测试后再合并。 |
| accept 暂时性错误退避 | litecn [`585db52`](https://github.com/litecn/anytls-rs/commit/585db52a3a)、zhu [`784fd2b`](https://github.com/zhu/anytls-rs/commit/784fd2be69) | 当前 accept error 会立即继续循环；建议加退避。但 errno 分类需用平台常量/适配，不应照搬 fork 中的硬编码数字。 |
| 多客户端互通测试 | zhu [`e999868`](https://github.com/zhu/anytls-rs/commit/e9998682bc)、padding 测试 [`42b70f3`](https://github.com/zhu/anytls-rs/commit/42b70f36f2) | 测试方向有价值；确认固定版本、许可证、网络可用性和本地 CI 可重复性后择取测试本身。 |
| benchmark/CI 可靠性 | zhu [`7d29b5e`](https://github.com/zhu/anytls-rs/commit/7d29b5eeac)、[`bb963ea`](https://github.com/zhu/anytls-rs/commit/bb963ea8c9)、[`f681a3a`](https://github.com/zhu/anytls-rs/commit/f681a3a061) | 把 smoke test 失败显式化、保证完整数据消费的方向值得评审；runner 固定版本需考虑维护成本。 |
| Android protect hook | madeye [`1d13bef`](https://github.com/madeye/anytls-rs/commit/1d13befb2f) | 仅在确认 Android/VPN 集成需求后合并；涉及平台专用 libc 和 socket 生命周期，需 Android target CI。 |
| SOCKS5/HTTP mixed listener | litecn [`44f028f`](https://github.com/litecn/anytls-rs/commit/44f028fe80)、[`c34a824`](https://github.com/litecn/anytls-rs/commit/c34a8247f5)、[`78089f2`](https://github.com/litecn/anytls-rs/commit/78089f2183) | 属于新功能，不是现有缺陷修复；应作为独立功能提案评估兼容性、监听地址冲突和 CLI 设计。 |
| SOCKS5 上游代理 | litecn [`ea29621`](https://github.com/litecn/anytls-rs/commit/ea2962190c) | 可选功能；需要单独设计认证、DNS 解析责任和代理链错误语义。 |
| TLS 后端切换 ring | litecn [`3ca15f0`](https://github.com/litecn/anytls-rs/commit/3ca15f08a0) | 不属于安全修复的必需项；需基于二进制大小、构建时间、平台支持、FIPS 需求和完整 TLS 测试作决定。 |
| 不认证的 `--forward` | litecn [`031fb9e`](https://github.com/litecn/anytls-rs/commit/031fb9ef97) | 默认不建议纳入。未经 AnyTLS 认证的 HTTP/HTTPS relay 会扩大开放代理面；若有需求，必须先明确绑定范围、访问控制、CONNECT 限制、速率/连接上限和 SSRF 防护。 |

### 不建议整支合并

- zhu 分支累计约 7000 行新增/改动，横跨协议、代理、session 生命周期、依赖、CI 和 release；应拆成可独立审查的 PR。
- zhu 分支将 `trust-dns-resolver` 迁移为 `hickory-resolver`，同时其 `Cargo.toml` 仍包含 `chrono`。直接合并依赖或整支代码会与上游已经完成的 RUSTSEC-2026-0543 修复冲突，可能重新引入被移除的 `chrono`。依赖升级应单独做 `cargo tree`、安全审计和目标平台验证。
- zhu 的 release 配置含 fork 仓库地址、`publish = false` 和 custom version；litecn 的版本号、手动 dispatch、移除 crates.io 发布等 release 变更也体现各自发布策略，不能直接覆盖上游。
- litecn 的 mixed listener、outbound proxy、forward 属于产品功能，不能与安全/稳定性修复捆绑合并。
- 目前上游在 `9786cd3` 已实现有界 stream 队列和 backpressure；litecn `0f96f0a` 与 zhu `f8b2d86`/`3316ddf` 的队列主题不能当作尚未修复的功能重复导入。应比较测试覆盖和关闭时的 FIN 行为，再决定是否只补测试。

## 建议的主线纳入顺序

1. **小补丁先行：**DNS cache 端口隔离/容量上限；证书实时有效期；frame payload 长度上限。每项独立提交，附对应回归测试。
2. **网络健壮性：**多地址连接 fallback、accept 退避、TLS handshake 并发上限和 deadline；每项增加可控超时/失败路径测试。
3. **跨实现验证：**先引入稳定可重复的协议互通测试，再审查 UDP-over-TCP、padding、session teardown 的较大改动。
4. **功能提案：**Android hook、mixed listener、outbound proxy、ring 后端分别依据产品需求讨论，不随修复补丁默认进入主线。

合并前需逐个候选在上游当前代码上重新实现/移植；不能把 fork 的旧版 `Cargo.toml`、`Cargo.lock`、release workflow 或整文件覆盖带回主线。本次仅生成了本盘点文档，没有从 fork 合并代码，也未运行 fork 分支测试。

## 主线开发进展

- 已先将当时工作区提交为 `ee93c61`（Actions Node.js 24 升级和本盘点文档），然后从该提交建立 `fix/fork-review-mainline` 开发分支。
- 已在开发分支实现 DNS IP-only cache、端口按请求附加、缓存条目上限、证书有效期实时计算、frame payload 长度校验、多地址 TCP fallback，以及 TLS/认证握手限额与超时、accept 错误退避。
- 新增相应回归测试；`cargo test --all-features`（67 个单元测试及集成测试，共 90 项）、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo fmt` 均已通过。
- 这些开发分支改动仍待代码审阅和提交；fork 自身的 CI 与测试状态仍未在本机验证。
