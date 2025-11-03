# 基准测试待补全项目清单

**最后更新**: 2025-11-03  
**当前状态**: 52/28 测试已实现 (186%)  
**Phase 1 状态**: ✅ 已完成  
**Phase 2 状态**: ✅ 已完成  
**Phase 3 状态**: ✅ 已完成 (100%)

---

## 📊 快速概览

| 优先级 | 数量 | 完成度 |
|--------|------|--------|
| 🔴 高优先级 | 11 | 100% ✅ |
| 🟡 中优先级 | 7 | 100% ✅ |
| 🟢 低优先级 | 5 | 100% ✅ |
| **总计** | **23** | **100%** ✅ |

---

## 🔴 高优先级（核心性能路径）

### 1. Frame 编解码完整测试 ✅ 已完成
**文件**: `benches/session_bench.rs`

- [x] `FrameCodec::encode()` - 编码性能测试 (`bench_frame_encoding`)
- [x] `FrameCodec::decode()` - 解码性能测试 (`bench_frame_decode`)
- [x] 端到端编解码往返测试（通过 decode 测试间接覆盖）

---

### 2. Stream 读写性能测试 ✅ 已完成
**文件**: `benches/stream_bench.rs`

- [x] Stream 写入性能（不同大小数据）(`bench_stream_write`)
- [x] Stream 读取性能（不同大小数据）(`bench_stream_read`)
- [x] StreamReader 读取性能 (`bench_streamreader_read`)
- [x] 并发读写性能测试 (`bench_stream_concurrent_read_write`)

---

### 3. Session 完整流程测试 ✅ 已完成
**文件**: `benches/session_bench.rs`

- [x] `Session::start_client()` 完整启动性能 (`bench_session_startup_complete`)
- [x] `Session::write_frame()` 性能（含 padding）(`bench_session_write_frame`)
- [x] `Session::write_data_frame()` 性能 (`bench_session_write_data_frame`)
- [x] 控制帧写入性能 (`bench_session_control_frames`)
- [x] 多 Stream 管理性能 (`bench_session_multiple_streams`)

---

### 4. 端到端数据传输测试 ✅ 已完成
**文件**: `benches/e2e_bench.rs`

- [x] Stream 打开和数据发送性能 (`bench_e2e_stream_open_and_send`)
- [x] 多流并发性能 (`bench_e2e_multiple_streams_concurrent`)
- [x] 数据吞吐量测试 (`bench_e2e_data_throughput`)
- [x] 完整会话启动和流处理 (`bench_e2e_session_startup_and_streams`)

---

## 🟡 中优先级（重要但非关键）

### 5. 并发连接性能测试 ✅ 已完成
**文件**: `benches/concurrent_bench.rs`

- [x] 多 Session 并发创建性能 (`bench_concurrent_session_creation`)
- [x] 单 Session 多 Stream 并发性能 (`bench_concurrent_stream_creation`)
- [x] 多 Session 多 Stream 并发性能 (`bench_concurrent_multi_session_multi_stream`)
- [x] Session Pool 性能 (`session_pool_bench.rs`)

---

### 6. TLS 性能测试 ✅ 已完成
**文件**: `benches/tls_bench.rs`

- [x] TLS 证书和密钥生成性能 (`bench_tls_generate_key_pair`)
- [x] TLS 服务器配置创建性能 (`bench_tls_create_server_config`)
- [x] TLS 客户端配置创建性能 (`bench_tls_create_client_config`)
- [x] TLS 配置重用性能 (`bench_tls_config_reuse`)
- [x] 带服务器名的证书生成 (`bench_tls_generate_key_pair_with_name`)

---

### 7. Client/Server 性能测试 ✅ 已完成
**文件**: `benches/client_server_bench.rs`

- [x] Client 创建和初始化性能 (`bench_client_creation`)
- [x] Client 带自定义 Pool 配置 (`bench_client_with_pool_config`)
- [x] TLS Connector 创建和重用 (`bench_tls_connector_creation`, `bench_tls_connector_reuse`)
- [x] 密码哈希性能 (`bench_client_password_hashing`)
- [x] Client/Server 组件设置 (`bench_client_server_setup_components`)

---

## 🟢 低优先级（优化和对比）

### 8. 内存分配测试 ✅ 已完成
**文件**: `benches/memory_bench.rs`

- [x] Bytes 分配性能（3种方式：from_vec, copy_from_slice, from_static）
- [x] 零拷贝 vs 拷贝性能对比（clone vs copy_from_slice）
- [x] Bytes slice 操作性能
- [x] Vec vs Bytes 分配对比
- [x] 内存重用模式对比

---

### 9. 性能对比测试 ✅ 已完成
**文件**: `benches/comparison_bench.rs`

- [x] 基线性能指标 (`bench_baseline_frame_encoding`)
- [x] 编码策略对比 (`bench_frame_encoding_strategies`)
- [x] Stream 创建开销追踪 (`bench_stream_creation_overhead`)
- [x] Session 启动开销追踪 (`bench_session_startup_overhead`)
- [x] 数据帧吞吐量追踪 (`bench_data_frame_throughput`)
- [x] 关键路径操作性能 (`bench_critical_path_operations`)
- [ ] 与 Go 版本对比（需要外部工具或 Go 实现）

---

### 10. 特殊场景测试 ✅ 已完成
**文件**: `benches/edge_cases_bench.rs`

- [x] 小数据包性能（1-64 字节）(`bench_small_packets`)
- [x] 大数据包性能（1MB+）(`bench_large_packets`)
- [x] 高频率操作性能（心跳、SYNACK）(`bench_high_frequency_operations`)
- [x] 快速流创建性能 (`bench_rapid_stream_creation`)

---

## 📋 实施计划

### Phase 1: 核心性能路径（优先实施）
**时间**: 2-3 天  
**目标**: 完成高优先级项目 1-4

1. 改进现有 Frame 编码测试
2. 创建 Stream 读写性能测试
3. 完善 Session 完整流程测试
4. 创建端到端数据传输测试

### Phase 2: 并发和扩展性 ✅ 已完成
**时间**: 已完成  
**目标**: 完成中优先级项目 5-7 ✅

**完成内容**:
- ✅ 并发连接性能测试 (`concurrent_bench.rs` - 4 个测试)
- ✅ Session Pool 性能测试 (`session_pool_bench.rs` - 3 个测试)
- ✅ TLS 性能测试 (`tls_bench.rs` - 5 个测试)
- ✅ Client/Server 性能测试 (`client_server_bench.rs` - 6 个测试)

### Phase 3: 深度优化 ✅ 已完成
**时间**: 已完成  
**目标**: 完成低优先级项目 8-10 ✅

**完成内容**:
- ✅ 内存分配测试 (`memory_bench.rs` - 5 个测试)
- ✅ 特殊场景测试 (`edge_cases_bench.rs` - 4 个测试)
- ✅ 性能对比测试 (`comparison_bench.rs` - 6 个测试)
  - 基线性能指标和策略对比
  - 开销追踪和关键路径分析
  - 与 Go 版本对比需要外部工具（标记为未来工作）

---

## ✅ 当前已实现的测试

### Phase 1 测试 (19 个)
**session_bench.rs** (11 个):
- `bench_frame_encoding` ✅ 已改进为使用实际 FrameCodec
- `bench_frame_decode` ✅ 新增
- `bench_stream_creation` ✅ 完整
- `bench_session_startup` ✅ 完整
- `bench_session_startup_complete` ✅ 新增（包含 start_client）
- `bench_session_write_frame` ✅ 新增
- `bench_session_write_data_frame` ✅ 新增
- `bench_session_control_frames` ✅ 新增
- `bench_session_multiple_streams` ✅ 新增
- `bench_padding_factory` ✅ 完整
- `bench_password_hashing` ✅ 完整

**stream_bench.rs** (4 个):
- `bench_stream_write` ✅ 新增
- `bench_stream_read` ✅ 新增（已修复阻塞问题）
- `bench_streamreader_read` ✅ 新增
- `bench_stream_concurrent_read_write` ✅ 新增

**e2e_bench.rs** (4 个):
- `bench_e2e_stream_open_and_send` ✅ 新增
- `bench_e2e_multiple_streams_concurrent` ✅ 新增
- `bench_e2e_data_throughput` ✅ 新增
- `bench_e2e_session_startup_and_streams` ✅ 新增

### Phase 2 测试 (18 个)
**concurrent_bench.rs** (4 个):
- `bench_concurrent_session_creation` ✅
- `bench_concurrent_stream_creation` ✅
- `bench_concurrent_stream_data_send` ✅
- `bench_concurrent_multi_session_multi_stream` ✅

**session_pool_bench.rs** (3 个):
- `bench_session_pool_add_and_get` ✅
- `bench_session_pool_concurrent_get` ✅
- `bench_session_pool_cleanup` ✅

**tls_bench.rs** (5 个):
- `bench_tls_generate_key_pair` ✅
- `bench_tls_generate_key_pair_with_name` ✅
- `bench_tls_create_server_config` ✅
- `bench_tls_create_client_config` ✅
- `bench_tls_config_reuse` ✅

**client_server_bench.rs** (6 个):
- `bench_client_creation` ✅（已修复异步问题）
- `bench_client_with_pool_config` ✅（已修复异步问题）
- `bench_tls_connector_creation` ✅
- `bench_tls_connector_reuse` ✅
- `bench_client_password_hashing` ✅
- `bench_client_server_setup_components` ✅

**Phase 1 + Phase 2 总计: 37 个基准测试函数**

### Phase 3 测试 (9 个)
**memory_bench.rs** (5 个):
- `bench_bytes_allocation` ✅ 测试 3 种 Bytes 分配方式
- `bench_bytes_clone_vs_copy` ✅ 零拷贝 vs 拷贝对比
- `bench_bytes_slice` ✅ Bytes slice 操作性能
- `bench_vec_vs_bytes_allocation` ✅ Vec vs Bytes 分配对比
- `bench_memory_reuse_patterns` ✅ 内存重用模式对比

**edge_cases_bench.rs** (4 个):
- `bench_small_packets` ✅ 小数据包性能（1-64 字节）
- `bench_large_packets` ✅ 大数据包性能（1MB+）
- `bench_high_frequency_operations` ✅ 高频率操作（心跳、SYNACK）
- `bench_rapid_stream_creation` ✅ 快速流创建

**Phase 3 测试 (15 个)**
**memory_bench.rs** (5 个):
- `bench_bytes_allocation` ✅ 测试 3 种 Bytes 分配方式
- `bench_bytes_clone_vs_copy` ✅ 零拷贝 vs 拷贝对比
- `bench_bytes_slice` ✅ Bytes slice 操作性能
- `bench_vec_vs_bytes_allocation` ✅ Vec vs Bytes 分配对比
- `bench_memory_reuse_patterns` ✅ 内存重用模式对比

**edge_cases_bench.rs** (4 个):
- `bench_small_packets` ✅ 小数据包性能（1-64 字节）
- `bench_large_packets` ✅ 大数据包性能（1MB+）
- `bench_high_frequency_operations` ✅ 高频率操作（心跳、SYNACK）
- `bench_rapid_stream_creation` ✅ 快速流创建

**comparison_bench.rs** (6 个):
- `bench_baseline_frame_encoding` ✅ 基线性能指标
- `bench_frame_encoding_strategies` ✅ 编码策略对比
- `bench_stream_creation_overhead` ✅ Stream 创建开销追踪
- `bench_session_startup_overhead` ✅ Session 启动开销追踪
- `bench_data_frame_throughput` ✅ 数据帧吞吐量追踪
- `bench_critical_path_operations` ✅ 关键路径操作性能

**总计: 52 个基准测试函数，10 个基准测试文件，1,991 行代码**

---

## 📈 性能目标指标

### 测量说明

所有性能指标现在都可以通过基准测试进行测量。运行 `cargo bench` 获取详细性能数据。

### 核心性能指标

| 指标 | 目标值 | 测量测试 | 当前状态 |
|------|--------|----------|----------|
| **Frame 编码吞吐量** | >500 MB/s | `bench_frame_encoding` | ✅ 已具备测量能力 |
| **Frame 解码吞吐量** | >500 MB/s | `bench_frame_decode` | ✅ 已具备测量能力 |
| **Stream 创建延迟** | <1ms | `bench_stream_creation` | ✅ 已具备测量能力 |
| **Session 启动延迟** | <10ms | `bench_session_startup_complete` | ✅ 已具备测量能力 |
| **数据帧写入吞吐量** | >100 MB/s | `bench_session_write_data_frame` | ✅ 已具备测量能力 |
| **端到端吞吐量** | >100 MB/s | `bench_e2e_data_throughput` | ✅ 已具备测量能力 |
| **并发连接数** | >1000 | `bench_concurrent_session_creation` | ✅ 已具备测量能力 |
| **内存分配效率** | 零拷贝优先 | `bench_bytes_clone_vs_copy` | ✅ 已具备测量能力 |

### 性能追踪指标

| 指标 | 测量测试 | 用途 |
|------|----------|------|
| **基线性能** | `bench_baseline_frame_encoding` | 追踪性能回归 |
| **关键路径操作** | `bench_critical_path_operations` | 优化重点识别 |
| **小数据包性能** | `bench_small_packets` | 极端场景分析 |
| **大数据包性能** | `bench_large_packets` | 大负载处理能力 |
| **高频率操作** | `bench_high_frequency_operations` | 心跳等高频操作优化 |

### 如何测量性能指标

#### 1. Frame 编码/解码吞吐量
```bash
# 运行 Frame 编码测试
cargo bench --bench session_bench frame_encoding

# 运行 Frame 解码测试  
cargo bench --bench session_bench frame_decode

# 结果会在 target/criterion/ 目录下生成 HTML 报告
# 吞吐量 = (数据大小 / 耗时) × 迭代次数
```

#### 2. Stream 创建延迟
```bash
cargo bench --bench session_bench stream_creation
# 查看平均延迟时间（ns/iter 或 μs/iter）
```

#### 3. Session 启动延迟
```bash
cargo bench --bench session_bench session_startup_complete
```

#### 4. 数据吞吐量
```bash
cargo bench --bench session_bench session_write_data_frame
cargo bench --bench e2e_bench e2e_data_throughput
```

#### 5. 并发性能
```bash
cargo bench --bench concurrent_bench concurrent_session_creation
# 测试不同并发数量（1, 5, 10, 20, 50, 100）
```

#### 6. 内存使用（需要外部工具）
```bash
# 使用 valgrind 或 perf 工具
valgrind --tool=massif cargo bench --bench memory_bench
# 或使用 Rust 的 memory profiler
```

### 性能目标验证

运行完整基准测试套件：
```bash
# 运行所有基准测试（指定基准测试名称）
cargo bench --bench session_bench
cargo bench --bench stream_bench
# ... 等等

# 保存基线（需要指定基准测试名称）
cargo bench --bench session_bench -- --save-baseline main

# 后续运行对比（需要指定基准测试名称）
cargo bench --bench session_bench -- --baseline main

# 或者使用脚本批量运行所有基准测试
for bench in session_bench stream_bench e2e_bench concurrent_bench session_pool_bench tls_bench client_server_bench memory_bench edge_cases_bench comparison_bench; do
    cargo bench --bench $bench -- --save-baseline main
done
```

### 性能回归检测

建议在 CI/CD 中集成基准测试，设置性能阈值：
```bash
# 检测性能回归（允许 ±5% 波动）
cargo bench -- --baseline main --threshold 0.05
```

---

## 🔗 相关文档

- [基准测试指南](BENCHMARK_GUIDE.md) - 如何运行基准测试
- [基准测试详细分析](BENCHMARK_ANALYSIS.md) - 完整分析和建议
- [测试指南](TEST_GUIDE.md) - 功能测试指南

