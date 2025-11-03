# Linux版本编译和使用指南

## 🚀 快速开始

### 前置要求

1. **Docker**: 必须安装并运行Docker Desktop（macOS/Windows）或Docker（Linux）
2. **Rust**: 已安装Rust工具链
3. **cross工具**: 脚本会自动安装，或手动安装：`cargo install cross --git https://github.com/cross-rs/cross`

---

### 方法1: 使用编译脚本（推荐）

```bash
cd anytls-rs
./build-linux.sh
```

脚本会自动：
1. 检查并安装cross工具（如果需要）
2. 检查Docker是否运行
3. 检查并安装必要的target
4. 使用cross编译Linux版本的二进制文件（在Docker容器中）
5. 显示文件位置和信息

**注意**: 首次编译需要下载Docker镜像，可能需要几分钟。

---

### 方法2: 手动使用cross编译

```bash
cd anytls-rs

# 1. 安装cross（如果未安装）
cargo install cross --git https://github.com/cross-rs/cross

# 2. 安装musl target（如果未安装）
rustup target add x86_64-unknown-linux-musl

# 3. 确保Docker运行
docker info

# 4. 使用cross编译
cross build --release --bins --target x86_64-unknown-linux-musl
```

---

## 📦 编译结果

编译成功后，二进制文件位于：

```
target/x86_64-unknown-linux-musl/release/anytls-server
target/x86_64-unknown-linux-musl/release/anytls-client
```

**特点**:
- ✅ 静态链接（使用musl），无需额外依赖
- ✅ 可在大多数Linux系统上运行（x86_64架构）
- ✅ 文件大小约6-7MB

---

## 📤 传输到Linux服务器

### 使用SCP

```bash
# 传输服务器
scp target/x86_64-unknown-linux-musl/release/anytls-server user@host:/usr/local/bin/

# 传输客户端
scp target/x86_64-unknown-linux-musl/release/anytls-client user@host:/usr/local/bin/
```

### 使用其他方式

- FTP/SFTP客户端
- 通过USB等存储设备
- 在Linux服务器上直接编译（如果安装了Rust）

---

## 🐧 Linux服务器使用

### 1. 设置执行权限

```bash
chmod +x /usr/local/bin/anytls-server
chmod +x /usr/local/bin/anytls-client
```

### 2. 启动服务器

```bash
# 前台运行
./anytls-server -l 0.0.0.0:8443 -p your_password

# 后台运行（使用nohup）
nohup ./anytls-server -l 0.0.0.0:8443 -p your_password > server.log 2>&1 &

# 使用systemd（可选）
# 创建服务文件：/etc/systemd/system/anytls-server.service
```

### 3. 启动客户端

```bash
# 前台运行
./anytls-client -l 127.0.0.1:1080 -s server_ip:8443 -p your_password

# 后台运行
nohup ./anytls-client -l 127.0.0.1:1080 -s server_ip:8443 -p your_password > client.log 2>&1 &
```

---

## 🔧 编译选项说明

### musl vs glibc

我们使用 `x86_64-unknown-linux-musl` 而不是 `x86_64-unknown-linux-gnu`：

- **musl优势**:
  - ✅ 静态链接，无需系统库依赖
  - ✅ 文件体积小
  - ✅ 兼容性好（可在各种Linux发行版运行）
  - ✅ 使用cross工具编译简单（在Docker容器中）

- **glibc缺点**:
  - ❌ 需要动态链接系统库
  - ❌ 在macOS上编译需要安装交叉编译工具链
  - ❌ 可能在较老的Linux系统上无法运行

### 为什么使用cross工具？

由于rustls依赖的底层库（ring、aws-lc-sys）包含C代码，需要C编译器来编译。使用cross工具的好处：

- ✅ 自动提供完整的交叉编译环境（在Docker容器中）
- ✅ 无需手动安装交叉编译工具链
- ✅ 支持多种target
- ✅ 编译环境隔离，更可靠

---

## 🛠️ 故障排查

### 问题1: 编译失败 "linker not found" 或 "x86_64-linux-musl-gcc not found"

**原因**: rustls依赖的C库需要C编译器，直接使用cargo无法找到交叉编译器。

**解决**: 使用cross工具（推荐）

```bash
# 安装cross
cargo install cross --git https://github.com/cross-rs/cross

# 使用cross编译
cross build --release --bins --target x86_64-unknown-linux-musl
```

### 问题2: Docker未运行

**错误信息**: `Cannot connect to the Docker daemon`

**解决**: 
1. 启动Docker Desktop（macOS/Windows）
2. 或启动Docker服务（Linux）：`sudo systemctl start docker`
3. 验证：`docker info`

### 问题2: Linux服务器上无法运行

**检查**:
```bash
# 检查文件类型
file anytls-server

# 检查架构
uname -m  # 应该是 x86_64

# 检查权限
ls -l anytls-server
```

**解决**: 确保是x86_64架构，并且文件有执行权限

### 问题3: 需要其他架构的二进制文件

如需ARM64版本：

```bash
rustup target add aarch64-unknown-linux-musl
cargo build --release --bins --target aarch64-unknown-linux-musl
```

---

## 📋 编译检查清单

- [ ] Rust工具链已安装 (`rustc --version`)
- [ ] musl target已安装 (`rustup target list --installed | grep musl`)
- [ ] 编译成功 (`cargo build --release --bins --target x86_64-unknown-linux-musl`)
- [ ] 二进制文件存在 (`ls target/x86_64-unknown-linux-musl/release/anytls-*`)
- [ ] 文件类型正确 (`file target/x86_64-unknown-linux-musl/release/anytls-server`)
- [ ] 已传输到Linux服务器
- [ ] 在Linux服务器上测试成功

---

## 💡 提示

1. **第一次编译可能较慢**: 需要编译所有依赖项，后续会更快
2. **文件大小**: musl静态链接版本约6-7MB，适合传输
3. **测试**: 建议先在本地macOS测试功能，再传输到Linux服务器测试
4. **日志**: 使用 `RUST_LOG=debug` 环境变量查看详细日志

---

*最后更新: 2025-11-02*

