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

## 未来改进

- [ ] 添加端到端数据传输基准
- [ ] 添加并发连接基准
- [ ] 添加内存分配基准
- [ ] 与 Go 版本性能对比

