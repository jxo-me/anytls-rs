# 性能基准测试指南

## 概述

本项目使用 `criterion` 库进行性能基准测试，可以测量关键操作的性能指标。

## 运行基准测试

### 运行所有基准测试

```bash
cargo bench --bench session_bench
```

### 运行特定基准测试

```bash
# Frame 编码基准
cargo bench --bench session_bench frame_encoding

# Stream 创建基准
cargo bench --bench session_bench stream_creation

# Session 启动基准
cargo bench --bench session_bench session_startup

# Padding Factory 基准
cargo bench --bench session_bench padding_factory

# 密码哈希基准
cargo bench --bench session_bench password_hashing
```

### 快速测试模式（验证基准测试可以运行）

```bash
cargo bench --bench session_bench -- --test
```

## 基准测试内容

### 1. Frame 编码 (`bench_frame_encoding`)

测试不同大小的 Frame 编码性能：
- 64 字节
- 256 字节
- 1024 字节
- 4096 字节
- 16384 字节

### 2. Stream 创建 (`bench_stream_creation`)

测试创建新 Stream 的性能。

### 3. Session 启动 (`bench_session_startup`)

测试创建新 Session 的性能。

### 4. Padding Factory (`bench_padding_factory`)

测试 Padding Factory 的两个操作：
- 获取默认工厂
- 生成记录负载大小

### 5. 密码哈希 (`bench_password_hashing`)

测试不同长度密码的 SHA256 哈希性能。

## 查看结果

运行 `cargo bench` 后，结果会显示在终端，同时会在 `target/criterion/` 目录下生成 HTML 报告：

```bash
open target/criterion/frame_encoding/report/index.html
```

## 基准测试目标

当前的基准测试主要用于：
1. 建立性能基线
2. 检测性能回归
3. 优化关键路径
4. 追踪性能指标

## 可用的基准测试

### Phase 1: 核心性能路径
- `session_bench.rs` - Frame 编解码、Session 管理、Stream 创建
- `stream_bench.rs` - Stream 读写性能
- `e2e_bench.rs` - 端到端数据传输

### Phase 2: 并发和扩展性
- `concurrent_bench.rs` - 并发连接和流处理
- `session_pool_bench.rs` - Session Pool 管理
- `tls_bench.rs` - TLS 配置和证书生成
- `client_server_bench.rs` - Client/Server 组件创建

### Phase 3: 深度优化
- `memory_bench.rs` - 内存分配和优化
- `edge_cases_bench.rs` - 特殊场景（小/大数据包、高频率操作）
- `comparison_bench.rs` - 性能对比和追踪

## 性能指标测量

所有性能目标指标都可以通过相应的基准测试进行测量，详见 [BENCHMARK_TODO.md](BENCHMARK_TODO.md#-性能目标指标)。

## 查看详细报告

运行基准测试后，在 `target/criterion/` 目录下查看 HTML 报告：

```bash
# 打开特定测试的报告
open target/criterion/frame_encoding/report/index.html
```

## 保存和比较基线

### 保存基线

```bash
# 为特定基准测试保存基线
cargo bench --bench session_bench -- --save-baseline main

# 基线数据会保存在 target/criterion/<benchmark_name>/<baseline_name>/ 目录
```

### 与基线对比

```bash
# 使用保存的基线进行对比
cargo bench --bench session_bench -- --baseline main

# 报告会显示性能变化（提升/下降百分比）
```

### 批量保存基线（使用脚本）

```bash
# 为所有基准测试保存基线
for bench in session_bench stream_bench e2e_bench concurrent_bench session_pool_bench tls_bench client_server_bench memory_bench edge_cases_bench comparison_bench; do
    cargo bench --bench $bench -- --save-baseline main
done
```

报告包含：
- 性能统计（平均值、中位数、标准差）
- 性能趋势图
- 不同输入大小的对比

