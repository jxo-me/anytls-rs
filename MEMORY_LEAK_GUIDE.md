# 内存泄漏检查指南

## 工具选择

### 1. valgrind (Linux/macOS)

Valgrind 是经典的内存泄漏检测工具。

**安装**:
```bash
# macOS (使用 Homebrew)
brew install valgrind

# Linux (使用包管理器)
sudo apt-get install valgrind  # Debian/Ubuntu
sudo yum install valgrind      # CentOS/RHEL
```

**使用**:
```bash
# 检查内存泄漏
valgrind --leak-check=full --show-leak-kinds=all \
  ./target/release/anytls-server -l 127.0.0.1:8443 -p test_password

# 检查特定泄漏类型
valgrind --leak-check=full --show-leak-kinds=definite,possible \
  ./target/release/anytls-client -l 127.0.0.1:1080 -s 127.0.0.1:8443 -p test_password
```

### 2. cargo-valgrind (推荐用于 Rust)

专门为 Rust 项目优化的 Valgrind 包装器。

**安装**:
```bash
cargo install cargo-valgrind
```

**使用**:
```bash
# 运行测试并检查内存泄漏
cargo valgrind test

# 运行特定测试
cargo valgrind test --test basic_proxy

# 运行并生成报告
cargo valgrind --tool=memcheck test
```

### 3. tokio-console (检测任务泄漏)

用于检测 Tokio 异步任务的泄漏。

**安装**:
```bash
cargo install tokio-console
```

**使用**:

1. 在 `Cargo.toml` 中添加：
```toml
[target.'cfg(all())']
tokio_unstable = true

[tokio-console]
default = true
```

2. 运行程序：
```bash
RUSTFLAGS="--cfg tokio_unstable" cargo run --bin anytls-server
```

3. 在另一个终端查看任务：
```bash
tokio-console
```

### 4. dhat-rs (堆分析)

Rust 堆分析工具。

**添加到 `Cargo.toml`**:
```toml
[dev-dependencies]
dhat = "0.3"
```

**在代码中使用**:
```rust
use dhat::{Dhat, DhatAlloc};

#[global_allocator]
static ALLOCATOR: DhatAlloc = DhatAlloc;

fn main() {
    let _dhat = Dhat::start_heap_profiling();
    // ... your code ...
}
```

## 手动检查清单

### Arc 循环引用检查

1. **检查 Session → Stream → Session 引用**:
   - `Session` 持有 `Arc<Stream>`
   - `Stream` 通过 channel 与 `Session` 通信
   - 确保没有直接循环引用

2. **检查回调函数**:
   - 确保回调函数不会持有 `Session` 的引用导致循环

### 任务泄漏检查

1. **检查 spawn 的任务**:
   ```rust
   // 确保所有 spawn 的任务都有清理机制
   tokio::spawn(async move {
       // ... task code ...
   });
   ```

2. **检查 select! 宏**:
   - 确保所有分支都能正常退出
   - 避免无限循环导致任务无法完成

### Channel 泄漏检查

1. **检查 UnboundedReceiver**:
   - 确保所有 receiver 都被正确消费或关闭

2. **检查 Sender 生命周期**:
   - 确保 sender 在不需要时被正确 drop

## 长期运行测试

### 脚本示例

```bash
#!/bin/bash
# long_run_test.sh

echo "Starting long-running test..."

# 启动服务器
./target/release/anytls-server -l 127.0.0.1:8443 -p test_password > server.log 2>&1 &
SERVER_PID=$!

sleep 2

# 启动客户端
./target/release/anytls-client -l 127.0.0.1:1080 -s 127.0.0.1:8443 -p test_password > client.log 2>&1 &
CLIENT_PID=$!

sleep 2

# 运行 1 小时的负载测试
echo "Running load test for 1 hour..."
for i in {1..3600}; do
    curl -s --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get > /dev/null 2>&1
    sleep 1
    
    if [ $((i % 60)) -eq 0 ]; then
        echo "Progress: $i/3600 requests"
        # 检查内存使用
        ps -o pid,rss,vsz -p $SERVER_PID $CLIENT_PID
    fi
done

# 清理
kill $CLIENT_PID $SERVER_PID
```

### 监控内存使用

```bash
# 实时监控内存
watch -n 1 'ps -o pid,rss,vsz,comm -p $(pgrep -f anytls)'

# 记录内存使用历史
while true; do
    ps -o pid,rss -p $(pgrep -f anytls) >> memory.log
    sleep 5
done
```

## 常见内存泄漏模式

### 1. Arc 循环引用

**问题**:
```rust
struct A {
    b: Arc<B>,
}

struct B {
    a: Arc<A>,  // 循环引用！
}
```

**解决**: 使用 `Weak` 或重新设计结构

### 2. 未关闭的 Channel

**问题**:
```rust
let (tx, mut rx) = mpsc::unbounded_channel();
// tx 被 clone 到多个地方，但从未 drop
```

**解决**: 确保在不需要时显式关闭 channel

### 3. 未完成的 Tokio 任务

**问题**:
```rust
loop {
    tokio::spawn(async {
        loop { /* 无限循环 */ }
    });
}
```

**解决**: 添加取消机制或确保任务能够正常退出

## 检查步骤

### 步骤 1: 运行 cargo-valgrind

```bash
cargo valgrind test
```

### 步骤 2: 运行长期测试

```bash
./long_run_test.sh
```

### 步骤 3: 检查内存增长

监控内存使用，确保没有持续增长。

### 步骤 4: 检查任务数量

使用 `tokio-console` 检查任务数量是否稳定。

## 预期结果

### 正常情况

- 内存使用稳定（可能有小幅波动）
- 任务数量稳定
- 无 valgrind 报告的泄漏

### 异常情况

- 内存持续增长 → 可能存在泄漏
- 任务数量持续增长 → 可能存在任务泄漏
- valgrind 报告泄漏 → 需要修复

## 修复优先级

1. **P0 - 严重泄漏**: 内存持续增长，可能导致 OOM
2. **P1 - 轻微泄漏**: 内存缓慢增长，长期运行可能有问题
3. **P2 - 潜在问题**: Valgrind 警告但实际影响不大

---

*最后更新: 2025-11-02*

